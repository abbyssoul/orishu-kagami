#!/usr/bin/env python3
"""Pure helpers for the network-placement wire proof.

Kept free of subprocess and root so the argument construction, counter
arithmetic and verdicts can be unit tested without privilege. The privileged
topology and worker driving live in `check-worker-network-placement.py`.
"""

# The same counters the Pi lab samples, so both evidence sets read alike.
COUNTERS = ('rx_bytes', 'tx_bytes', 'rx_errors', 'tx_errors', 'rx_dropped', 'tx_dropped')

# Both links sit on one subnet and the excluded one is preferred, reproducing
# the physical lab's Ethernet/Wi-Fi failure rather than inventing a new shape.
SUBNET = '10.177.0.0/24'
SELECTED_METRIC = 600
EXCLUDED_METRIC = 100

# A veth name must survive `InterfaceName` validation: at most 15 bytes.
NAMESPACES = ('orishu-pl-a', 'orishu-pl-b')
LINKS = {
    'orishu-pl-a': {'selected': 'plright-a', 'excluded': 'plwrong-a'},
    'orishu-pl-b': {'selected': 'plright-b', 'excluded': 'plwrong-b'},
}
ADDRESSES = {
    'plright-a': '10.177.0.1',
    'plwrong-a': '10.177.0.2',
    'plright-b': '10.177.0.11',
    'plwrong-b': '10.177.0.12',
}

PEER_PORT = 6655
CLIENT_PORT = 6680

# The applicant dials the introducer at the address on the introducer's own
# selected link, so a correct run has no reason to touch the excluded one.
INTRODUCER_ADDRESS = ADDRESSES[LINKS[NAMESPACES[0]]['selected']]


def require(condition, message):
    """Fail loudly. The harness never downgrades a failed check to a warning."""
    if not condition:
        raise AssertionError(message)


def selected_address(namespace):
    """The address on that namespace's selected link."""
    require(namespace in LINKS, f'unknown namespace {namespace!r}')
    return ADDRESSES[LINKS[namespace]['selected']]


def worker_arguments(binary, namespace, state_dir, socket_path, *,
                     peer_placed, accepts_peers,
                     client_listen=False, client_placed=False, client_tls=None,
                     client_role='selected'):
    """Command line for one worker, with each role placed or deliberately not.

    Placement always pairs a wildcard bind with a named device: a concrete bind
    address plus an interface is refused by the worker, because the kernel would
    accept an address owned by a different device and then match no ingress.

    Each worker advertises *its own* selected-link address. Advertising the
    introducer's address from both would publish a peer endpoint that does not
    belong to the advertising node.
    """
    require(namespace in LINKS, f'unknown namespace {namespace!r}')
    require(client_role in LINKS[namespace], 'unknown client interface role')
    require(not (client_placed and not client_listen),
            'client placement needs a TCP client listener to place')
    require(not (client_listen and client_tls is None),
            'a TCP client listener requires a TLS certificate and key')
    arguments = [
        str(binary),
        '--state-dir', str(state_dir),
        '--listen.clients', str(socket_path),
        '--listen.peers', f'0.0.0.0:{PEER_PORT}',
        '--advertise.peers', f'{selected_address(namespace)}:{PEER_PORT}',
        '--accepts.peers', 'true' if accepts_peers else 'false',
    ]
    if peer_placed:
        arguments += ['--interface.peers', LINKS[namespace]['selected']]
    if client_listen:
        certificate, key = client_tls
        arguments += ['--listen.clients', f'0.0.0.0:{CLIENT_PORT}',
                      '--tls-cert', str(certificate), '--tls-key', str(key)]
    if client_placed:
        arguments += ['--interface.clients', LINKS[namespace][client_role]]
    return arguments


def parse_counters(reader):
    """Read one interface's counters. `reader(name)` returns the raw sysfs text."""
    sample = {}
    for counter in COUNTERS:
        raw = reader(counter).strip()
        require(raw.isdigit(), f'non-numeric {counter}: {raw!r}')
        sample[counter] = int(raw)
    return sample


def delta(before, after):
    """Per-counter increase. A counter must never move backwards mid-run."""
    require(set(before) == set(after) == set(COUNTERS), 'counter sets must match')
    result = {}
    for counter in COUNTERS:
        change = after[counter] - before[counter]
        require(change >= 0, f'{counter} went backwards: {before} -> {after}')
        result[counter] = change
    return result


# Background chatter (ARP, IPv6 multicast) is unavoidable on an up link, so a
# link is judged "carried traffic" by a threshold rather than by any movement.
# The two roles move very different volumes, and one threshold cannot serve
# both: a peer formation exchanges tens of kilobytes, while a client check is
# a TLS handshake plus a few small authenticated responses. Sizing the client
# threshold at the peer volume would read real client traffic as "neither".
PEER_CARRIED_BYTES = 8192
# Measured, not estimated: four fresh connections, each a TLS 1.3 handshake with
# an EC certificate plus one authenticated request and its CBOR response, moved
# 20,981 bytes against 0 bytes over an equivalent idle window (loopback, which
# like the summed veth counters accounts for both directions). The threshold
# sits an order of magnitude below that and far above quiet-link ARP and
# multicast, so neither a real check nor background noise is misread.
CLIENT_CARRIED_BYTES = 2048


def carried_traffic(link_delta, threshold):
    """Did this link carry the run's traffic, as opposed to background noise?"""
    require(threshold > 0, 'a carried-traffic threshold must be positive')
    return max(link_delta['tx_bytes'], link_delta['rx_bytes']) >= threshold


def classify(selected_delta, excluded_delta, threshold=PEER_CARRIED_BYTES):
    """Which link carried the run. Never infer isolation from link state alone."""
    on_selected = carried_traffic(selected_delta, threshold)
    on_excluded = carried_traffic(excluded_delta, threshold)
    if on_selected and on_excluded:
        return 'both'
    if on_selected:
        return 'selected'
    if on_excluded:
        return 'excluded'
    return 'neither'


# Only `joined` is success. `catchingUp` means admitted but still installing the
# admission baseline and target credential: catch-up has not completed, so the
# constrained path has not been shown to carry it. Accepting `catchingUp` would
# let a run pass while the very traffic this harness exists to prove was still
# outstanding. It is retained separately so a stalled catch-up is reported as
# such rather than as correct exclusion.
JOINED_STATE = 'joined'
ADMITTED_STATES = ('catchingUp', JOINED_STATE)

# `orishuctl cluster info --output json` is a CLI presentation projection, not
# the typed `cluster::Summary` wire shape: the protocol's
# `memberCount`/`aliveCount`/`membershipLocked` are projected as
# `nodes`/`alive`/`locked`. The same omission produced a first-summary-read
# `KeyError` once before; see the mapping in
# `diagnose-formation-convergence.py` and the write-up in
# `docs/measurements/formation-convergence-diagnostic-2026-09-10.md`.
# Read these names from the binary, never from the Rust struct.
MEMBERS_FIELD = 'nodes'
ALIVE_FIELD = 'alive'
READY_FIELD = 'introducerReady'
REQUIRED_FIELDS = (
    'formationId', 'participation', MEMBERS_FIELD, ALIVE_FIELD, READY_FIELD,
)

# This topology has exactly two workers. `>= 2` would accept a third, which in
# a sequential harness means a previous scenario's worker leaked into this one
# and the counters no longer describe the run being judged.
EXPECTED_MEMBERS = 2


def summary_field(view, field):
    """Read one field, naming what was actually present when it is missing."""
    if field not in view:
        raise AssertionError(
            f'cluster summary has no {field!r}; `orishuctl cluster info` returned '
            f'{sorted(view)}. Its JSON field names are a CLI contract and may have '
            'been renamed.'
        )
    return view[field]


def converged(views):
    """Did both workers complete a live two-member formation, catch-up included?

    Matching formation IDs alone only proves the applicant accepted an
    assignment, which can happen before any sustained peer traffic. This
    harness is evidence about the wire, so it requires catch-up to have
    finished, both sides to agree on *exactly* the two members this topology
    has, and both to report themselves operationally ready. Only the applicant
    reports `joined`: the introducer created the formation and never joins one.
    """
    if views is None or len(views) != 2:
        return False
    introducer, applicant = views
    for view in views:
        for field in REQUIRED_FIELDS:
            summary_field(view, field)
    return (
        applicant['formationId'] == introducer['formationId']
        and applicant['participation'] == JOINED_STATE
        and all(view[MEMBERS_FIELD] == EXPECTED_MEMBERS for view in views)
        and all(view[ALIVE_FIELD] == EXPECTED_MEMBERS for view in views)
        and all(view[READY_FIELD] is True for view in views)
    )


def admitted_but_not_joined(views):
    """Admission completed but catch-up did not. Not success; not exclusion."""
    if views is None or len(views) != 2:
        return False
    introducer, applicant = views
    return (
        applicant.get('formationId') == introducer.get('formationId')
        and applicant.get('participation') in ADMITTED_STATES
        and not converged(views)
    )


def formation_verified(views, memberships, formation, expected):
    """Exact pinned formation, source identities, certificates and live membership."""
    if not converged(views) or len(expected) != EXPECTED_MEMBERS or len(memberships) != 2:
        return False
    if any(view['formationId'] != formation for view in views):
        return False
    if [view.get('sourceNodeId') for view in views] != list(expected):
        return False
    wanted = {node: (certificate, 'alive') for node, certificate in expected.items()}
    for rows in memberships:
        if not isinstance(rows, list) or len(rows) != len(wanted):
            return False
        if any(not isinstance(row, dict) or not isinstance(row.get('nodeId'), str) for row in rows):
            return False
        actual = {row['nodeId']: (row.get('certFingerprint'), row.get('liveness')) for row in rows}
        if actual != wanted:
            return False
    return True


def evidence_row(label, selected_delta, excluded_delta, joined):
    """One result row, in the shape the measurement documents already use."""
    return {
        'scenario': label,
        'joined': joined,
        'selected_tx_bytes': selected_delta['tx_bytes'],
        'selected_rx_bytes': selected_delta['rx_bytes'],
        'excluded_tx_bytes': excluded_delta['tx_bytes'],
        'excluded_rx_bytes': excluded_delta['rx_bytes'],
        'carried_by': classify(selected_delta, excluded_delta),
    }
