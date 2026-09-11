#!/usr/bin/env python3
"""Prove worker network placement on real sockets over a deliberately misrouting topology.

Two network namespaces are joined by two veth pairs on one subnet, with the
*excluded* link preferred by route metric. That is the physical lab's failure
reproduced locally: a server bound to the right address still answers over the
wrong link.

Seven scenarios run in order, and each role's negative control must fail the old
way before any placed run for that role is allowed to count:

1. peers, neither placed — must misroute onto the excluded link
2. peers, introducer placed only — must *not* form; exclusion works
3. peers, both placed — must form over the selected link
4. peers, both placed, selected link dropped — traffic stops, never relocates
5. clients, unplaced — authenticated requests must reach over the excluded link
6. clients, placed — excluded link refused, selected link serves them
7. peers and clients placed on different devices — both roles work concurrently

The client scenarios are not redundant with the peer ones: client placement
binds different sockets and its replies leave over accepted connections, so it
needs its own evidence rather than inheriting the peer verdict. Each client
check opens fresh connections and issues authenticated `GET /api/v1/cluster`
requests, so a pass means the worker's application replies traversed the
selected device, not merely that a handshake completed.

Scenario 4 mutates shared topology and always restores the link and its route,
so later scenarios never inherit a broken one. A peer run counts as successful
only when catch-up has completed on both sides; a run that was admitted but
stalled in `catchingUp` is reported as such rather than as correct exclusion.

Requires CAP_NET_ADMIN in its network namespace, either through root or an
isolated user/mount/network namespace where unprivileged user namespaces are
permitted (see the measurement report linked from the placement task). This is an
opt-in target and never part of `make check`. Resources are never reclaimed
implicitly; if a previous run crashed, remove its leftovers with `--cleanup`.
"""
import argparse
import contextlib
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from worker_placement_topology import (
    ADDRESSES,
    CLIENT_CARRIED_BYTES,
    CLIENT_PORT,
    EXCLUDED_METRIC,
    INTRODUCER_ADDRESS,
    LINKS,
    NAMESPACES,
    SELECTED_METRIC,
    SUBNET,
    admitted_but_not_joined,
    classify,
    converged,
    delta,
    evidence_row,
    formation_verified,
    parse_counters,
    require,
    selected_address,
    worker_arguments,
)

VETH_PAIRS = (('plright-a', 'plright-b'), ('plwrong-a', 'plwrong-b'))
cleanup_receipts = []


def run(command, **kwargs):
    """Run a bounded command, failing loudly with its stderr."""
    result = subprocess.run(command, capture_output=True, timeout=30, **kwargs)
    require(
        result.returncode == 0,
        f'{command!r} failed ({result.returncode}): {result.stderr.decode(errors="replace")}',
    )
    return result.stdout.decode()


def ip(*arguments, namespace=None):
    prefix = ['ip', 'netns', 'exec', namespace] if namespace else []
    return run(prefix + ['ip', *arguments])


def namespace_exists(name):
    return Path('/var/run/netns', name).exists()


def root_device_exists(name):
    return Path('/sys/class/net', name).exists()


def existing_resources():
    """Anything this harness's fixed names already refer to, whoever owns it."""
    found = [f'namespace {name}' for name in NAMESPACES if namespace_exists(name)]
    found += [
        f'link {device}'
        for pair in VETH_PAIRS
        for device in pair
        if root_device_exists(device)
    ]
    return found


def remove(created):
    """Delete only what this run created, in reverse order of creation.

    Deliberately not a blanket "remove anything with these names": a concurrent
    run, or an unrelated namespace that happens to share a name, must not be
    destroyed by this harness's cleanup. `--cleanup` is the explicit opt-in for
    removing leftovers from a previous crash.
    """
    for kind, name in reversed(created):
        if kind == 'namespace':
            subprocess.run(['ip', 'netns', 'delete', name], capture_output=True, timeout=30)
        else:
            subprocess.run(['ip', 'link', 'delete', name], capture_output=True, timeout=30)
    created.clear()


def force_cleanup():
    """Explicitly remove leftovers from an earlier crashed run."""
    removed = existing_resources()
    for name in NAMESPACES:
        if namespace_exists(name):
            subprocess.run(['ip', 'netns', 'delete', name], capture_output=True, timeout=30)
    for pair in VETH_PAIRS:
        for device in pair:
            if root_device_exists(device):
                subprocess.run(['ip', 'link', 'delete', device], capture_output=True, timeout=30)
    return removed


def build_topology(created):
    """Two namespaces, two links each, excluded link preferred by metric.

    Every resource is appended to `created` as soon as it exists, so a failure
    part-way through still removes exactly this run's own resources.
    """
    for name in NAMESPACES:
        ip('netns', 'add', name)
        created.append(('namespace', name))
    for left, right in VETH_PAIRS:
        # A veth pair is one object: deleting either end removes both. Keep
        # exactly one root-visible handle tracked at each step, because an end
        # that has moved into a namespace can no longer be deleted from here,
        # and is removed with that namespace anyway.
        ip('link', 'add', left, 'type', 'veth', 'peer', 'name', right)
        created.append(('link', left))
        ip('link', 'set', left, 'netns', NAMESPACES[0])
        created.remove(('link', left))
        created.append(('link', right))
        ip('link', 'set', right, 'netns', NAMESPACES[1])
        created.remove(('link', right))
    for namespace in NAMESPACES:
        ip('link', 'set', 'lo', 'up', namespace=namespace)
        for role in ('selected', 'excluded'):
            device = LINKS[namespace][role]
            address = ADDRESSES[device]
            # `noprefixroute` keeps the kernel from installing its own
            # connected route, so the metrics below are the only preference.
            ip('addr', 'add', f'{address}/24', 'dev', device, 'noprefixroute',
               namespace=namespace)
            ip('link', 'set', device, 'up', namespace=namespace)
            install_route(namespace, role)
        # Accept packets that arrive on the link the peer chose; strict reverse
        # path filtering would mask the misrouting this harness must observe.
        for device in ('all', *LINKS[namespace].values()):
            subprocess.run(
                ['ip', 'netns', 'exec', namespace, 'sysctl', '-qw',
                 f'net.ipv4.conf.{device}.rp_filter=0'],
                capture_output=True, timeout=30)


def install_route(namespace, role):
    """(Re-)install one link's subnet route at its role's metric."""
    device = LINKS[namespace][role]
    address = ADDRESSES[device]
    metric = SELECTED_METRIC if role == 'selected' else EXCLUDED_METRIC
    ip('route', 'add', SUBNET, 'dev', device, 'src', address,
       'metric', str(metric), namespace=namespace)


def set_link_up(namespace, role, up):
    """Bring one link down or back up, restoring its route on the way up.

    Downing a device makes the kernel withdraw routes that reference it, so
    bringing it back is not just `link set up`: without re-installing the route
    the link is present but unusable, and a later scenario would fail for a
    reason that has nothing to do with placement.
    """
    device = LINKS[namespace][role]
    ip('link', 'set', device, 'up' if up else 'down', namespace=namespace)
    if not up:
        return
    routes = ip('route', 'show', SUBNET, namespace=namespace)
    if f'dev {device}' not in routes:
        install_route(namespace, role)
    require(f'dev {device}' in ip('route', 'show', SUBNET, namespace=namespace),
            f'could not restore the {role} route for {device} in {namespace}')


def route_evidence():
    """Exact-source route lookups, the same check the Pi runs record."""
    lookups = {}
    for namespace in NAMESPACES:
        for role in ('selected', 'excluded'):
            device = LINKS[namespace][role]
            source = ADDRESSES[device]
            destination = INTRODUCER_ADDRESS if source != INTRODUCER_ADDRESS else ADDRESSES['plright-b']
            output = ip('route', 'get', destination, 'from', source, namespace=namespace)
            lookups[f'{namespace}:{source}->{destination}'] = output.strip().splitlines()[0]
    return lookups


def counters(namespace, device):
    def reader(counter):
        return run(['ip', 'netns', 'exec', namespace, 'cat',
                    f'/sys/class/net/{device}/statistics/{counter}'])
    return parse_counters(reader)


def sample_links():
    """Both links in both namespaces, as one snapshot."""
    return {
        (namespace, role): counters(namespace, LINKS[namespace][role])
        for namespace in NAMESPACES
        for role in ('selected', 'excluded')
    }


def combine(before, after, role):
    """Sum one role's counters across both namespaces."""
    totals = {counter: 0 for counter in before[(NAMESPACES[0], role)]}
    for namespace in NAMESPACES:
        for counter, value in delta(before[(namespace, role)], after[(namespace, role)]).items():
            totals[counter] += value
    return totals


def client_certificate(directory, address):
    """A disposable self-signed server certificate for one TCP client listener.

    Matches the shape the Pi network harness already uses: short-lived, bound to
    one literal IP SAN, never a shared or wildcard trust anchor.
    """
    certificate, key = directory / 'client.crt', directory / 'client.key'
    run(['openssl', 'req', '-x509', '-newkey', 'ec',
         '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes', '-days', '1',
         '-subj', '/CN=orishu-placement-check',
         '-addext', f'subjectAltName=IP:{address}',
         '-addext', 'basicConstraints=critical,CA:FALSE',
         '-addext', 'keyUsage=critical,digitalSignature',
         '-addext', 'extendedKeyUsage=serverAuth',
         '-keyout', str(key), '-out', str(certificate)],
        stdin=subprocess.DEVNULL)
    key.chmod(0o600)
    return certificate, key


class Worker:
    """One worker process inside a namespace, with its own private state."""

    def __init__(self, binary, namespace, root, index, *, peer_placed, accepts_peers,
                 environment, client_listen=False, client_placed=False, client_role='selected'):
        self.namespace = namespace
        self.state = root / str(index)
        self.socket = root / f'{index}.sock'
        client_tls = None
        if client_listen:
            tls_root = root / f'tls-{index}'
            tls_root.mkdir(mode=0o700)
            client_tls = client_certificate(tls_root, ADDRESSES[LINKS[namespace][client_role]])
        arguments = worker_arguments(
            binary, namespace, self.state, self.socket,
            peer_placed=peer_placed, accepts_peers=accepts_peers,
            client_listen=client_listen, client_placed=client_placed,
            client_tls=client_tls,
            client_role=client_role,
        )
        self.client_tls = client_tls
        self.log = (root / f'{index}.log').open('wb')
        self.process = subprocess.Popen(
            ['ip', 'netns', 'exec', namespace, *arguments],
            stdout=self.log, stderr=subprocess.STDOUT, env=environment,
        )

    def stop(self):
        forced = False
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                forced = True
                self.process.kill()
                self.process.wait(timeout=10)
        self.log.close()
        cleanup_receipts.append({'pid': self.process.pid, 'namespace': self.namespace,
                                 'exit': self.process.returncode, 'forced': forced,
                                 'socket_removed': not self.socket.exists()})

    def output(self):
        path = Path(self.log.name)
        return path.read_text(errors='replace') if path.exists() else ''


def control(ctl_binary, worker, command, *, authenticated=False, environment=None):
    """Drive a worker over its Unix socket. Sockets are namespace independent."""
    arguments = [str(ctl_binary), '--host', str(worker.socket),
                 '--output', 'json', '--timeout', '5s']
    if authenticated:
        arguments += ['--operator-token-file', str(worker.state / 'operator.token')]
    return subprocess.run(arguments + command, capture_output=True, timeout=10,
                          env=environment)


# Diagnostic context for runs that did not converge, so a failure to form is
# never silently indistinguishable from correct exclusion.
formation_detail = []


def poll(action, accept, description, *, deadline=20.0):
    """Wait for a bounded condition, reporting what was still true at timeout."""
    limit = time.monotonic() + deadline
    result = None
    while time.monotonic() < limit:
        result = action()
        if accept(result):
            return result
        time.sleep(0.2)
    return result


def verify_link_recovery(collect, matches, sample, baseline, switch, change,
                         *, wait=poll, pause=time.sleep):
    """Prove established traffic, isolation of a fresh mutation, then recovery."""
    settled = wait(collect, matches, 'formation before loss', deadline=60)
    require(matches(settled), 'link loss requires a verified formation')
    require(all(view['locked'] is False for view in settled['views']),
            'loss challenge requires an initially unlocked formation')
    before = sample()
    established = (combine(baseline, before, 'selected'), combine(baseline, before, 'excluded'))
    require(classify(*established) == 'selected', 'no established selected-link traffic')
    try:
        switch(False)
        lost = sample()
        change()
        pause(4)
        during = collect()
        require(during is not None and during['views'][0]['locked'] is True
                and during['views'][1]['locked'] is False,
                'fresh lock must apply locally but cannot cross the disconnected link')
        after = sample()
        selected, excluded = combine(lost, after, 'selected'), combine(lost, after, 'excluded')
        require(classify(selected, excluded) in ('neither', 'selected'),
                'traffic moved to excluded interface during loss')
    finally:
        switch(True)
    def recovered(value):
        return matches(value) and all(view['locked'] is True for view in value['views'])
    recovery = wait(collect, recovered, 'fresh lock propagation after recovery', deadline=60)
    require(recovered(recovery), 'restored link did not recover exact membership and fresh state')
    recovered_links = sample()
    recovery_selected = combine(after, recovered_links, 'selected')
    recovery_excluded = combine(after, recovered_links, 'excluded')
    require(recovery_selected['tx_bytes'] > 0 and
            classify(recovery_selected, recovery_excluded) not in ('excluded', 'both'),
            'recovery did not remain on the selected path')
    return selected, excluded, {
        'established_carried_by': 'selected', 'recovery_verified': True,
        'challenge': 'fresh replicated lock', 'before': settled, 'during': during,
        'recovered': recovery, 'recovery_selected': recovery_selected,
        'recovery_excluded': recovery_excluded,
    }


def attempt_formation(worker_binary, ctl_binary, root, environment, *,
                      introducer_placed, applicant_placed, drop_selected_link=False,
                      split_clients=False):
    """Start two workers, try to join, and report which link carried the run."""
    # Each scenario owns a fresh directory; the workers' state, sockets and
    # logs are created inside it as soon as they start.
    root.mkdir(parents=True, exist_ok=True)
    os.chmod(root, 0o700)
    workers = []
    try:
        workers.append(Worker(worker_binary, NAMESPACES[0], root, 0,
                              peer_placed=introducer_placed, accepts_peers=True,
                              environment=environment, client_listen=split_clients,
                              client_placed=split_clients, client_role='excluded'))
        workers.append(Worker(worker_binary, NAMESPACES[1], root, 1,
                              peer_placed=applicant_placed, accepts_peers=True,
                              environment=environment, client_listen=split_clients,
                              client_placed=split_clients, client_role='excluded'))
        views = []
        for worker in workers:
            result = poll(lambda w=worker: control(ctl_binary, w, ['cluster', 'info'],
                                                   environment=environment),
                          lambda r: r.returncode == 0, 'startup')
            require(result is not None and result.returncode == 0,
                    f'worker in {worker.namespace} never served cluster info:\n{worker.output()}')
            views.append(json.loads(result.stdout))
        require(views[0]['formationId'] != views[1]['formationId'],
                'workers must start as separate formations')
        def read(worker, command, authenticated=False):
            result = control(ctl_binary, worker, command, authenticated=authenticated,
                             environment=environment)
            require(result.returncode == 0, f'control read failed: {command}: {result.stderr!r}')
            return json.loads(result.stdout)
        initial_members = [read(worker, ['ls']) for worker in workers]
        fingerprints = []
        for view, members in zip(views, initial_members):
            require(len(members) == 1 and members[0]['nodeId'] == view['sourceNodeId'],
                    'fresh worker has unexpected membership')
            fingerprints.append(members[0]['certFingerprint'])

        material = control(ctl_binary, workers[0], ['token'], authenticated=True,
                           environment=environment)
        require(material.returncode == 0, f'join material unavailable: {material.stderr!r}')
        path = root / 'join.json'
        if path.exists():
            path.unlink()
        with path.open('x') as output:
            os.chmod(path, 0o600)
            output.write(material.stdout.decode())

        before = sample_links()
        command = ['join', '--join-material-file', str(path),
                   '--formation-id', views[1]['formationId'],
                   '--operation-id', 'placement-check']
        receipt = control(ctl_binary, workers[1], command, authenticated=True,
                          environment=environment)
        require(receipt.returncode == 0, f'join could not be submitted: {receipt.stderr!r}')

        def summaries():
            return {'views': [read(worker, ['cluster', 'info']) for worker in workers],
                    'memberships': [read(worker, ['ls']) for worker in workers],
                    'operation': read(workers[1], ['join-status', 'placement-check'], True)}

        def verified(snapshot):
            if snapshot is None:
                return False
            operation = snapshot['operation']
            if operation['state']['phase'] != 'joined':
                return False
            if (operation['sourceFormationId'] != views[1]['formationId'] or
                    operation['sourceNodeId'] != views[1]['sourceNodeId'] or
                    operation['targetFormationId'] != views[0]['formationId']):
                return False
            expected = {views[0]['sourceNodeId']: fingerprints[0],
                        operation['state']['nodeId']: fingerprints[1]}
            return formation_verified(snapshot['views'], snapshot['memberships'],
                                      views[0]['formationId'], expected)

        if drop_selected_link:
            def lock():
                read(workers[0], ['cluster', 'lock', '--formation-id', views[0]['formationId'],
                                 '--operation-id', 'placement-loss-lock'], True)
            selected, excluded, detail = verify_link_recovery(
                summaries, verified, sample_links, before,
                lambda up: set_link_up(NAMESPACES[0], 'selected', up), lock)
            return True, selected, excluded, detail

        # Adopting the introducer's formation ID only proves the applicant
        # accepted an assignment. Sustained peer traffic is what this harness
        # must observe, so require catch-up to have completed on both sides.
        # The deadline covers admission and catch-up, not adoption alone.
        settled = poll(summaries, verified, 'formation convergence', deadline=60.0)
        joined = verified(settled)
        after = sample_links()
        if split_clients:
            require(joined, 'split-role scenario requires exact peer formation first')
            address = ADDRESSES[LINKS[NAMESPACES[0]]['excluded']]
            code, detail = client_probe(NAMESPACES[1], LINKS[NAMESPACES[1]]['excluded'],
                                        address, workers[0].client_tls[0],
                                        workers[0].state / 'operator.token')
            require(code == 0, f'separate client interface failed: {detail}')
            denied, reason = client_probe(NAMESPACES[1], LINKS[NAMESPACES[1]]['selected'],
                                          address, workers[0].client_tls[0],
                                          workers[0].state / 'operator.token', attempts=1)
            require(denied == PROBE_UNREACHABLE, f'client accepted peer interface: {reason}')
            require(verified(summaries()), 'split-role requests disturbed peer membership')
            return True, combine(before, after, 'selected'), combine(before, after, 'excluded'), {
                'snapshot': settled, 'client_interface': 'excluded', 'client_requests': detail,
                'peer_interface_client_refused': True}
        if not joined and settled is not None:
            # Retain why it did not converge; a bare False hides a broken
            # harness behind what looks like correct exclusion.
            formation_detail.append({
                'scenario': str(root.name),
                'admitted_but_not_joined': admitted_but_not_joined(settled['views']),
                'snapshot': settled,
            })
        return (joined, combine(before, after, 'selected'),
                combine(before, after, 'excluded'), {'snapshot': settled})
    finally:
        for worker in reversed(workers):
            worker.stop()


# Separate connections, each completing a TLS handshake and one authenticated
# HTTP request against the real client API. A handshake alone proves only that
# the transport was reachable; an authenticated 200 with a body proves the
# worker's *application replies* traversed the selected device too. Each
# iteration is a fresh connection, so every accept() inherits the placement.
CLIENT_REQUESTS = 4

# Exit codes the harness distinguishes. Only a refused or timed-out connection
# is the expected result of exclusion; a protocol, authorization or certificate
# identity failure means something other than placement is wrong and must never
# be read as successful exclusion. `ssl.SSLError` subclasses `OSError`, so
# identity failures need their own code rather than falling into "unreachable".
PROBE_UNREACHABLE = 1
PROBE_BAD_RESPONSE = 2
PROBE_BAD_IDENTITY = 3

CLIENT_PROBE = """
import http.client, socket, ssl, sys

device, host, port, certificate, token_path, attempts = (
    sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5], int(sys.argv[6]))
token = open(token_path).read().strip()
mode = sys.argv[7] if len(sys.argv) > 7 else 'valid'
verify_host = sys.argv[8] if len(sys.argv) > 8 else host
if mode == 'wrong-token':
    token = '0' * 64 if token != '0' * 64 else '1' * 64
if mode not in ('valid', 'missing-token', 'wrong-token'):
    sys.exit(%(bad_response)d)
context = ssl.create_default_context(cafile=certificate)
# Verify the served identity, not merely that some pinned key answered. The
# certificate carries an IP SAN for the endpoint being dialled, and the task
# requires certificate names to cover actual client endpoints, so the SAN is
# checked against the address rather than waived.
context.check_hostname = True


def once():
    raw = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    raw.settimeout(5)
    if device:
        raw.setsockopt(socket.SOL_SOCKET, 25, device.encode())
    raw.connect((host, port))
    with context.wrap_socket(raw, server_hostname=verify_host) as tls:
        authorization = '' if mode == 'missing-token' else f'Authorization: Bearer {token}\\r\\n'
        tls.sendall((
            'GET /api/v1/cluster HTTP/1.1\\r\\n'
            f'Host: {host}\\r\\n'
            f'{authorization}'
            'Connection: close\\r\\n\\r\\n').encode())
        response = http.client.HTTPResponse(tls)
        response.begin()
        body = response.read(65537)
        if len(body) > 65536:
            raise ValueError('oversized response')
        return response.status, len(body)


try:
    results = [once() for _ in range(attempts)]
except ssl.SSLError as error:
    # Subclasses OSError, so it must be caught first or a served-but-wrong
    # identity would be reported as an unreachable endpoint.
    print(f'tls identity failure: {error}', file=sys.stderr)
    sys.exit(%(bad_identity)d)
except (http.client.HTTPException, ValueError) as error:
    print(f'protocol failure: {error}', file=sys.stderr)
    sys.exit(%(bad_response)d)
except OSError as error:
    print(f'unreachable: {error}', file=sys.stderr)
    sys.exit(%(unreachable)d)

for status, length in results:
    expected = 200 if mode == 'valid' else 401
    if status != expected or (expected == 200 and length == 0):
        print(f'unexpected response: {status!r} body={length}', file=sys.stderr)
        sys.exit(%(bad_response)d)
print(f'{len(results)} authenticated responses, '
      f'{sum(length for _, length in results)} body bytes')
""" % {'unreachable': PROBE_UNREACHABLE, 'bad_response': PROBE_BAD_RESPONSE,
       'bad_identity': PROBE_BAD_IDENTITY}


def client_probe(namespace, device, address, certificate, token_path, *,
                 mode='valid', verify_host=None, attempts=CLIENT_REQUESTS):
    """Issue authenticated client requests from `namespace`, optionally pinned."""
    result = subprocess.run(
        ['ip', 'netns', 'exec', namespace, 'python3', '-c', CLIENT_PROBE,
         device or '', address, str(CLIENT_PORT), str(certificate),
         str(token_path), str(attempts), mode, verify_host or address],
        capture_output=True, timeout=40,
    )
    detail = (result.stderr.decode(errors='replace').strip()
              or result.stdout.decode(errors='replace').strip())
    return result.returncode, detail


def attempt_client_reach(worker_binary, ctl_binary, root, environment, *, client_placed):
    """Exercise the *client* role: a placed TCP listener and its replies.

    The peer scenarios cannot cover this. Client placement binds a different
    socket, and replies leave over connections the kernel accepted, so it needs
    its own evidence rather than being assumed to follow from the peer result.
    """
    root.mkdir(parents=True, exist_ok=True)
    os.chmod(root, 0o700)
    worker = None
    try:
        worker = Worker(worker_binary, NAMESPACES[0], root, 0,
                        peer_placed=False, accepts_peers=False,
                        environment=environment,
                        client_listen=True, client_placed=client_placed)
        ready = poll(lambda: control(ctl_binary, worker, ['cluster', 'info'],
                                     environment=environment),
                     lambda r: r.returncode == 0, 'client listener startup')
        require(ready is not None and ready.returncode == 0,
                f'worker never served cluster info:\n{worker.output()}')
        certificate = worker.client_tls[0]

        token = worker.state / 'operator.token'
        results = {}
        for role in ('excluded', 'selected'):
            device = LINKS[NAMESPACES[1]][role]
            before = sample_links()
            code, detail = client_probe(
                NAMESPACES[1], device, INTRODUCER_ADDRESS, certificate, token)
            after = sample_links()
            # A protocol, authorization or certificate identity failure is not
            # evidence of exclusion, so it never reaches the verdicts below.
            require(code in (0, PROBE_UNREACHABLE),
                    f'client probe over the {role} link failed for a non-network '
                    f'reason (exit {code}): {detail}')
            results[role] = {
                'reached': code == 0,
                'detail': detail,
                'carried_by': classify(combine(before, after, 'selected'),
                                       combine(before, after, 'excluded'),
                                       CLIENT_CARRIED_BYTES),
                'selected_delta': combine(before, after, 'selected'),
                'excluded_delta': combine(before, after, 'excluded'),
            }
        if results['selected']['reached']:
            security = {}
            device = LINKS[NAMESPACES[1]]['selected']
            for mode in ('missing-token', 'wrong-token'):
                code, detail = client_probe(NAMESPACES[1], device, INTRODUCER_ADDRESS,
                                            certificate, token, mode=mode, attempts=1)
                require(code == 0, f'{mode} negative control failed: {detail}')
                security[mode] = '401'
            code, detail = client_probe(NAMESPACES[1], device, INTRODUCER_ADDRESS,
                                        certificate, token, verify_host='192.0.2.99', attempts=1)
            require(code == PROBE_BAD_IDENTITY, f'wrong-IP certificate check failed: {detail}')
            other = root / 'untrusted'
            other.mkdir(mode=0o700)
            untrusted, _ = client_certificate(other, INTRODUCER_ADDRESS)
            code, detail = client_probe(NAMESPACES[1], device, INTRODUCER_ADDRESS,
                                        untrusted, token, attempts=1)
            require(code == PROBE_BAD_IDENTITY, f'untrusted certificate check failed: {detail}')
            results['security'] = security | {'wrong_ip_rejected': True, 'untrusted_rejected': True}
        return results
    finally:
        if worker is not None:
            worker.stop()


def write_report(path, record):
    """Retain private evidence without ever replacing an existing report."""
    with open(path, 'x', opener=lambda name, flags: os.open(name, flags, 0o600)) as output:
        json.dump(record, output, indent=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=Path, default=Path('target/debug/orishu-worker'))
    parser.add_argument('--ctl', type=Path, default=Path('target/debug/orishuctl'))
    parser.add_argument('--output', type=Path, default=None,
                        help='optional path for the JSON evidence record')
    parser.add_argument('--cleanup', action='store_true',
                        help='remove leftovers from an earlier crashed run, then exit')
    options = parser.parse_args()
    if options.output:
        require(not options.output.exists(), 'output already exists; never overwrite evidence')

    require(sys.platform == 'linux', 'network placement enforcement is Linux-only')
    require(os.geteuid() == 0,
            'root in the current user namespace is required; use sudo or the '
            'isolated unshare recipe in the placement measurement report')
    require(shutil.which('ip') is not None, 'iproute2 is required')

    if options.cleanup:
        removed = force_cleanup()
        print('removed: ' + (', '.join(removed) if removed else 'nothing'))
        return

    for binary in (options.worker, options.ctl):
        require(binary.is_file(), f'missing binary {binary}; run `make build` first')
    require(shutil.which('openssl') is not None,
            'openssl is required for the disposable client listener certificate')

    # Never delete pre-existing resources implicitly: they may belong to a
    # concurrent run, or to something unrelated that shares these names.
    # Refusing is recoverable; destroying another run's namespaces is not.
    conflicting = existing_resources()
    require(not conflicting,
            'refusing to run: ' + ', '.join(conflicting) + ' already exist. '
            'Another run may be in progress; if not, remove them with '
            '`sudo python3 scripts/check-worker-network-placement.py --cleanup`')

    environment = {k: v for k, v in os.environ.items() if not k.startswith('ORISHU_')}
    rows = []
    created = []
    started = time.monotonic()
    record = {'schema_version': 2, 'status': 'running', 'scenarios': rows,
              'artifacts': {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in (options.worker, options.ctl)},
              'workers_cleanup': cleanup_receipts}
    try:
        build_topology(created)
        routes = route_evidence()
        # The topology must genuinely prefer the excluded link, or every later
        # verdict is meaningless. Assert it from the route table, not intent.
        for key, line in routes.items():
            namespace = key.split(':', 1)[0]
            if key.endswith(f'->{INTRODUCER_ADDRESS}') and namespace == NAMESPACES[1]:
                require(LINKS[namespace]['excluded'] in line,
                        f'topology does not misroute as intended: {key} -> {line}')

        if options.output:
            evidence = options.output.with_suffix('.evidence')
            evidence.mkdir(mode=0o700)
            directory_context = contextlib.nullcontext(str(evidence.resolve()))
            record['private_evidence'] = str(evidence.resolve())
        else:
            directory_context = tempfile.TemporaryDirectory(prefix='orishu-placement-')
        with directory_context as directory:
            root = Path(directory)
            os.chmod(root, 0o700)

            # 1. Negative control. An unplaced worker must reproduce the
            #    physical failure, otherwise the later runs prove nothing.
            joined, selected, excluded, detail = attempt_formation(
                options.worker, options.ctl, root / 'control', environment,
                introducer_placed=False, applicant_placed=False)
            rows.append(evidence_row('negative-control', selected, excluded, joined))
            rows[-1].update(detail)
            require(joined, 'negative control must form a cluster; the topology is unusable')
            require(rows[-1]['carried_by'] == 'excluded',
                    'negative control did not misroute; this harness cannot prove enforcement '
                    f'({rows[-1]})')

            # 2. Enforcement. The applicant still prefers the excluded link, so
            #    a placed introducer must not receive it at all.
            joined, selected, excluded, detail = attempt_formation(
                options.worker, options.ctl, root / 'excluded', environment,
                introducer_placed=True, applicant_placed=False)
            rows.append(evidence_row('placed-introducer-only', selected, excluded, joined))
            rows[-1].update(detail)
            require(not joined,
                    'a placed worker accepted traffic arriving on an excluded interface')
            # `joined` is now false for two very different reasons: nothing got
            # through (the point of this scenario), or admission got through and
            # only catch-up stalled. The second is a failure of exclusion, so it
            # must not be allowed to pass as the first.
            admitted = [entry for entry in formation_detail
                        if entry['scenario'] == 'excluded' and entry['admitted_but_not_joined']]
            require(not admitted,
                    'a placed worker completed admission over an excluded interface; '
                    f'exclusion did not hold ({admitted})')

            # 3. Positive. Both roles placed: the device overrides the route
            #    preference, which is the physical fix without touching routes.
            joined, selected, excluded, detail = attempt_formation(
                options.worker, options.ctl, root / 'placed', environment,
                introducer_placed=True, applicant_placed=True)
            rows.append(evidence_row('both-placed', selected, excluded, joined))
            rows[-1].update(detail)
            require(joined, 'placed workers could not form a cluster over the selected link')
            require(rows[-1]['carried_by'] == 'selected',
                    f'placed run did not use the selected link ({rows[-1]})')

            # 4. Interface loss must stop traffic, never relocate it. The
            #    scenario itself refuses to proceed unless a formation was
            #    running over the selected link first, so a post-drop stop is
            #    evidence rather than a run that never started.
            joined, selected, excluded, detail = attempt_formation(
                options.worker, options.ctl, root / 'loss', environment,
                introducer_placed=True, applicant_placed=True, drop_selected_link=True)
            row = evidence_row('selected-link-down', selected, excluded, joined)
            row.update(detail)
            rows.append(row)
            require(joined and detail.get('established_carried_by') == 'selected',
                    f'link-loss precondition was not established ({row})')
            require(row['carried_by'] in ('neither', 'selected'),
                    'losing the selected interface moved traffic onto the excluded one '
                    f'({row})')

            # 5. The client role has its own sockets and its own reply path, so
            #    it needs its own evidence rather than inheriting the peer one.
            unplaced_clients = attempt_client_reach(
                options.worker, options.ctl, root / 'clients-unplaced', environment,
                client_placed=False)
            require(unplaced_clients['excluded']['reached'],
                    'client negative control did not reach the listener over the excluded '
                    f'link; the topology is unusable ({unplaced_clients["excluded"]})')

            placed_clients = attempt_client_reach(
                options.worker, options.ctl, root / 'clients-placed', environment,
                client_placed=True)
            require(not placed_clients['excluded']['reached'],
                    'a placed client listener served a connection arriving on an excluded '
                    f'interface ({placed_clients["excluded"]})')
            require(placed_clients['selected']['reached'],
                    'a placed client listener refused the selected interface '
                    f'({placed_clients["selected"]})')
            # A completed handshake needs server->client bytes, so this is
            # reply-path evidence, not only ingress.
            require(placed_clients['selected']['carried_by'] == 'selected',
                    'placed client replies did not use the selected link '
                    f'({placed_clients["selected"]})')

            joined, selected, excluded, detail = attempt_formation(
                options.worker, options.ctl, root / 'split-roles', environment,
                introducer_placed=True, applicant_placed=True, split_clients=True)
            rows.append(evidence_row('split-peer-client-interfaces', selected, excluded, joined) | detail)
            require(rows[-1]['carried_by'] == 'selected',
                    'split-role peers did not stay on their selected interface')

        record.update({
            'status': 'passed',
            'routes': routes,
            'scenarios': rows,
            'client_scenarios': {
                'unplaced': unplaced_clients,
                'placed': placed_clients,
            },
            'unconverged_detail': formation_detail,
        })
        require(all(row['exit'] == 0 and not row['forced'] and row['socket_removed']
                    for row in cleanup_receipts), 'unclean worker shutdown')
    except BaseException as error:
        record.update(status='failed', error=str(error))
        raise
    finally:
        try:
            remove(created)
            leftover = existing_resources()
            record['network_cleanup_clean'] = not leftover
            require(not leftover, 'cleanup left resources behind: ' + ', '.join(leftover))
        except BaseException as error:
            record.update(status='failed', cleanup_error=str(error))
            raise
        finally:
            record['elapsed_seconds'] = time.monotonic() - started
            if options.output:
                write_report(options.output, record)
            print(json.dumps({'status': record['status'], 'elapsed_seconds': record['elapsed_seconds'],
                              'scenarios': [row['scenario'] for row in rows],
                              'output': str(options.output)}))


if __name__ == '__main__':
    main()
