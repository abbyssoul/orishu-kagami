#!/usr/bin/env python3
"""Unit tests for the network-placement harness helpers.

These cover the parts that decide a verdict. The privileged topology itself is
exercised by `check-worker-network-placement.py`, which needs root.
"""
import unittest

from worker_placement_topology import (
    ADDRESSES,
    ALIVE_FIELD,
    CLIENT_CARRIED_BYTES,
    CLIENT_PORT,
    COUNTERS,
    EXCLUDED_METRIC,
    INTRODUCER_ADDRESS,
    LINKS,
    MEMBERS_FIELD,
    NAMESPACES,
    PEER_CARRIED_BYTES,
    REQUIRED_FIELDS,
    SELECTED_METRIC,
    admitted_but_not_joined,
    carried_traffic,
    classify,
    converged,
    delta,
    evidence_row,
    parse_counters,
    require,
    selected_address,
    summary_field,
    worker_arguments,
)


def sample(**overrides):
    values = {counter: 0 for counter in COUNTERS}
    values.update(overrides)
    return values


class TopologyConstants(unittest.TestCase):
    def test_excluded_link_is_preferred_so_the_control_can_misroute(self):
        # A lower metric wins. If this inverts, the negative control would pass
        # for the wrong reason and every later verdict would be vacuous.
        self.assertLess(EXCLUDED_METRIC, SELECTED_METRIC)

    def test_device_names_fit_the_worker_interface_grammar(self):
        for namespace in NAMESPACES:
            for device in LINKS[namespace].values():
                self.assertLessEqual(len(device), 15, device)
                self.assertTrue(device[0].isalnum(), device)
                self.assertTrue(
                    all(c.isalnum() or c in '_.-' for c in device[1:]), device
                )


class WorkerArguments(unittest.TestCase):
    def test_peer_and_client_roles_can_select_different_interfaces(self):
        args = worker_arguments('worker', NAMESPACES[0], '/state', '/sock',
                                peer_placed=True, accepts_peers=True, client_listen=True,
                                client_placed=True, client_tls=('/cert', '/key'), client_role='excluded')
        self.assertEqual(args[args.index('--interface.peers') + 1], LINKS[NAMESPACES[0]]['selected'])
        self.assertEqual(args[args.index('--interface.clients') + 1], LINKS[NAMESPACES[0]]['excluded'])

    def test_placement_always_pairs_a_wildcard_bind_with_a_device(self):
        arguments = worker_arguments(
            'worker', 'orishu-pl-a', '/state', '/api.sock',
            peer_placed=True, accepts_peers=True,
        )
        bind = arguments[arguments.index('--listen.peers') + 1]
        self.assertTrue(bind.startswith('0.0.0.0:'), bind)
        self.assertEqual(
            arguments[arguments.index('--interface.peers') + 1],
            LINKS['orishu-pl-a']['selected'],
        )

    def test_unplaced_workers_carry_no_interface_selection(self):
        arguments = worker_arguments(
            'worker', 'orishu-pl-b', '/state', '/api.sock',
            peer_placed=False, accepts_peers=False,
        )
        self.assertNotIn('--interface.peers', arguments)
        self.assertNotIn('--interface.clients', arguments)
        self.assertIn('--advertise.peers', arguments)

    def test_each_worker_advertises_its_own_selected_link_address(self):
        # Advertising the introducer's address from the applicant would publish
        # a peer endpoint that does not belong to the advertising node.
        advertised = {}
        for namespace in NAMESPACES:
            arguments = worker_arguments(
                'worker', namespace, '/state', '/api.sock',
                peer_placed=True, accepts_peers=True,
            )
            value = arguments[arguments.index('--advertise.peers') + 1]
            advertised[namespace] = value
            self.assertTrue(value.startswith(selected_address(namespace) + ':'), value)
        self.assertNotEqual(advertised[NAMESPACES[0]], advertised[NAMESPACES[1]])
        # The introducer is still the address the applicant is told to dial.
        self.assertEqual(selected_address(NAMESPACES[0]), INTRODUCER_ADDRESS)

    def test_selected_address_belongs_to_the_selected_link(self):
        for namespace in NAMESPACES:
            self.assertEqual(
                selected_address(namespace),
                ADDRESSES[LINKS[namespace]['selected']],
            )
            self.assertNotEqual(
                selected_address(namespace),
                ADDRESSES[LINKS[namespace]['excluded']],
            )

    def test_client_placement_adds_a_wildcard_tls_listener_and_device(self):
        arguments = worker_arguments(
            'worker', 'orishu-pl-a', '/state', '/api.sock',
            peer_placed=False, accepts_peers=False,
            client_listen=True, client_placed=True,
            client_tls=('/c.crt', '/c.key'),
        )
        self.assertIn(f'0.0.0.0:{CLIENT_PORT}', arguments)
        self.assertEqual(
            arguments[arguments.index('--interface.clients') + 1],
            LINKS['orishu-pl-a']['selected'],
        )
        self.assertEqual(arguments[arguments.index('--tls-cert') + 1], '/c.crt')
        self.assertEqual(arguments[arguments.index('--tls-key') + 1], '/c.key')
        # The private Unix admin socket survives alongside the TCP listener.
        self.assertEqual(arguments.count('--listen.clients'), 2)
        self.assertIn('/api.sock', arguments)

    def test_unenforceable_client_combinations_are_refused_by_the_harness(self):
        # Placing clients without a TCP listener, or a TCP listener without
        # TLS, are both rejected by the worker; the harness must not build them
        # and then report the worker's refusal as a placement failure.
        with self.assertRaises(AssertionError):
            worker_arguments('worker', 'orishu-pl-a', '/s', '/a.sock',
                             peer_placed=False, accepts_peers=False, client_placed=True)
        with self.assertRaises(AssertionError):
            worker_arguments('worker', 'orishu-pl-a', '/s', '/a.sock',
                             peer_placed=False, accepts_peers=False, client_listen=True)

    def test_unknown_namespace_is_rejected(self):
        with self.assertRaises(AssertionError):
            worker_arguments('worker', 'nope', '/state', '/s',
                             peer_placed=True, accepts_peers=True)


class Counters(unittest.TestCase):
    def test_parsing_rejects_non_numeric_sysfs_content(self):
        with self.assertRaises(AssertionError):
            parse_counters(lambda counter: 'not-a-number')
        parsed = parse_counters(lambda counter: '  7\n')
        self.assertEqual(parsed, sample(**{c: 7 for c in COUNTERS}))

    def test_counters_may_not_move_backwards(self):
        with self.assertRaises(AssertionError):
            delta(sample(tx_bytes=10), sample(tx_bytes=4))
        self.assertEqual(delta(sample(tx_bytes=10), sample(tx_bytes=30))['tx_bytes'], 20)

    def test_background_chatter_does_not_count_as_carried_traffic(self):
        self.assertFalse(
            carried_traffic(sample(tx_bytes=PEER_CARRIED_BYTES - 1), PEER_CARRIED_BYTES)
        )
        self.assertTrue(
            carried_traffic(sample(rx_bytes=PEER_CARRIED_BYTES), PEER_CARRIED_BYTES)
        )


class Thresholds(unittest.TestCase):
    # Measured against a real worker on loopback: the probe's four authenticated
    # connections moved this much, and an equivalent idle window moved nothing.
    MEASURED_CLIENT_BYTES = 20981
    MEASURED_IDLE_BYTES = 0

    def test_the_client_threshold_sits_between_idle_and_a_real_check(self):
        self.assertLess(CLIENT_CARRIED_BYTES, PEER_CARRIED_BYTES)
        # A real client check must clear the threshold with room to spare...
        self.assertGreater(self.MEASURED_CLIENT_BYTES, CLIENT_CARRIED_BYTES * 4)
        # ...and an idle link must stay far below it.
        self.assertLess(self.MEASURED_IDLE_BYTES, CLIENT_CARRIED_BYTES // 4)

    def test_the_client_threshold_still_excludes_link_background_noise(self):
        # ARP and IPv6 multicast over a few seconds on an idle veth.
        self.assertFalse(carried_traffic(sample(tx_bytes=512, rx_bytes=512),
                                         CLIENT_CARRIED_BYTES))

    def test_classification_honours_the_threshold_it_is_given(self):
        client_sized = sample(tx_bytes=CLIENT_CARRIED_BYTES + 1)
        quiet = sample(tx_bytes=16)
        self.assertEqual(classify(client_sized, quiet, CLIENT_CARRIED_BYTES), 'selected')
        # The same delta judged at the peer threshold is not carried traffic.
        self.assertEqual(classify(client_sized, quiet, PEER_CARRIED_BYTES), 'neither')

    def test_a_nonsense_threshold_is_rejected(self):
        with self.assertRaises(AssertionError):
            carried_traffic(sample(tx_bytes=10), 0)


class Verdicts(unittest.TestCase):
    def test_classification_names_every_outcome_including_both_and_neither(self):
        heavy = sample(tx_bytes=PEER_CARRIED_BYTES * 4)
        quiet = sample(tx_bytes=16)
        self.assertEqual(classify(heavy, quiet), 'selected')
        self.assertEqual(classify(quiet, heavy), 'excluded')
        self.assertEqual(classify(heavy, heavy), 'both')
        self.assertEqual(classify(quiet, quiet), 'neither')

    def test_leaking_onto_the_excluded_link_is_never_reported_as_selected(self):
        # A placed run that also moved real traffic over the excluded link is
        # a failure, so it must not collapse into the passing verdict.
        leaked = classify(sample(tx_bytes=PEER_CARRIED_BYTES * 4),
                          sample(rx_bytes=PEER_CARRIED_BYTES * 2))
        self.assertEqual(leaked, 'both')
        self.assertNotEqual(leaked, 'selected')

    def test_evidence_rows_retain_the_raw_byte_counts(self):
        carried = PEER_CARRIED_BYTES * 3
        row = evidence_row('both-placed', sample(tx_bytes=carried, rx_bytes=carried - 1),
                           sample(tx_bytes=1, rx_bytes=2), True)
        self.assertEqual(row['scenario'], 'both-placed')
        self.assertTrue(row['joined'])
        self.assertEqual(row['selected_tx_bytes'], carried)
        self.assertEqual(row['excluded_rx_bytes'], 2)
        self.assertEqual(row['carried_by'], 'selected')


def view(formation='f1', participation='joined', members=2, alive=2):
    return {
        'formationId': formation,
        'participation': participation,
        MEMBERS_FIELD: members,
        ALIVE_FIELD: alive,
        'introducerReady': True,
    }


# Captured verbatim from `orishuctl cluster info --output json` against a
# source-built worker. If the CLI's presentation changes, this fixture is what
# should fail first, rather than a namespace run that needs root to reproduce.
REAL_CLUSTER_INFO = {
    'alive': 1,
    'clusterName': 'standalone',
    'formationId': 'ffcb66c2089b8358f379983cd7fa269d7db24a6a895d323ce9aa857efa243b15',
    'introducerReady': False,
    'locked': False,
    'nodes': 1,
    'participation': 'standalone',
    'sourceNodeId': 'dfc64a22987481a06a74a6745bfa15056eb74219114fc14d91bc9a45b41c24c8',
    'view': 'localAtRequest',
    'workload': 'none',
}


# Captured from a real two-worker formation on loopback, after catch-up. Note
# the introducer stays `standalone`: it created the formation and never joins
# one, so only the applicant's participation is a success signal.
REAL_CONVERGED_PAIR = [
    {
        'alive': 2, 'clusterName': 'standalone',
        'formationId': '4ad65f2574f006c2f7b9ba012e0043d715bf74877b0d7b0348bfd10147670448',
        'introducerReady': True, 'locked': False, 'nodes': 2,
        'participation': 'standalone',
        'sourceNodeId': 'd50613460ba782e51215303d40ed62054946e6bbad90c91d4c91d9b7772998cd',
        'view': 'localAtRequest', 'workload': 'none',
    },
    {
        'alive': 2, 'clusterName': 'standalone',
        'formationId': '4ad65f2574f006c2f7b9ba012e0043d715bf74877b0d7b0348bfd10147670448',
        'introducerReady': True, 'locked': False, 'nodes': 2,
        'participation': 'joined',
        'sourceNodeId': 'e6e35efeb5eb51d4da99405ad082dce40a2ec4f68e23989769b5c493cfd85404',
        'view': 'localAtRequest', 'workload': 'none',
    },
]


class SummaryContract(unittest.TestCase):
    def test_a_real_completed_formation_is_recognised_as_converged(self):
        # The verdict must accept what the product actually produces; a bar
        # nothing can clear would fail every placed run, not just broken ones.
        self.assertTrue(converged(REAL_CONVERGED_PAIR))
        self.assertFalse(admitted_but_not_joined(REAL_CONVERGED_PAIR))

    def test_the_introducer_is_not_required_to_report_joined(self):
        # It created the formation and never joins one.
        self.assertEqual(REAL_CONVERGED_PAIR[0]['participation'], 'standalone')

    def test_every_field_the_verdict_needs_exists_in_real_ctl_output(self):
        for field in REQUIRED_FIELDS:
            self.assertIn(field, REAL_CLUSTER_INFO, field)

    def test_the_wire_type_spellings_are_not_what_the_cli_emits(self):
        # `cluster::Summary` serialises `memberCount`/`aliveCount`; orishuctl
        # reshapes them. Reading the Rust struct instead of the binary is how
        # this went wrong once already.
        self.assertNotIn('memberCount', REAL_CLUSTER_INFO)
        self.assertNotIn('aliveCount', REAL_CLUSTER_INFO)
        self.assertEqual(MEMBERS_FIELD, 'nodes')
        self.assertEqual(ALIVE_FIELD, 'alive')

    def test_a_renamed_field_names_what_was_actually_returned(self):
        with self.assertRaises(AssertionError) as caught:
            summary_field({'formationId': 'f1', 'memberCount': 2}, MEMBERS_FIELD)
        message = str(caught.exception)
        self.assertIn(MEMBERS_FIELD, message)
        self.assertIn('memberCount', message)

    def test_a_real_standalone_view_is_neither_converged_nor_admitted(self):
        pair = [REAL_CLUSTER_INFO, REAL_CLUSTER_INFO]
        self.assertFalse(converged(pair))
        self.assertFalse(admitted_but_not_joined(pair))


class Convergence(unittest.TestCase):
    def test_exact_membership_requires_pinned_sources_and_certificates(self):
        from worker_placement_topology import formation_verified
        import copy
        pair = copy.deepcopy(REAL_CONVERGED_PAIR)
        ids = [row['sourceNodeId'] for row in pair]
        pins = {ids[0]: 'cert-a', ids[1]: 'cert-b'}
        rows = [{'nodeId': node, 'certFingerprint': cert, 'liveness': 'alive'}
                for node, cert in pins.items()]
        formation = pair[0]['formationId']
        self.assertTrue(formation_verified(pair, [rows, rows], formation, pins))
        for field, value in [('nodeId', 'impostor'), ('certFingerprint', 'wrong'),
                             ('liveness', 'suspect')]:
            bad = copy.deepcopy(rows)
            bad[1][field] = value
            self.assertFalse(formation_verified(pair, [rows, bad], formation, pins))
        self.assertFalse(formation_verified(pair, [rows, [rows[0], rows[0]]], formation, pins))
        pair[1]['sourceNodeId'] = pair[0]['sourceNodeId']
        self.assertFalse(formation_verified(pair, [rows, rows], formation, pins))

    def test_unready_or_extra_members_never_pass(self):
        for field, value in [('introducerReady', False), (MEMBERS_FIELD, 3), (ALIVE_FIELD, 3)]:
            pair = [view(), view()]
            pair[1][field] = value
            self.assertFalse(converged(pair))

    def test_two_live_joined_members_on_both_sides_is_success(self):
        self.assertTrue(converged([view(), view()]))

    def test_catching_up_is_not_success(self):
        # `catchingUp` means admitted but still installing the admission
        # baseline: catch-up has not completed, so the constrained path has not
        # been shown to carry it.
        views = [view(), view(participation='catchingUp')]
        self.assertFalse(converged(views))
        # ...and it must be distinguishable from nothing getting through.
        self.assertTrue(admitted_but_not_joined(views))

    def test_exclusion_is_distinguishable_from_a_stalled_catch_up(self):
        # Nothing got through: separate formations, no admission at all.
        excluded = [view(formation='f1', members=1, alive=1),
                    view(formation='f2', participation='standalone',
                         members=1, alive=1)]
        self.assertFalse(converged(excluded))
        self.assertFalse(admitted_but_not_joined(excluded))

    def test_a_fully_joined_formation_is_not_flagged_as_stalled(self):
        self.assertFalse(admitted_but_not_joined([view(), view()]))

    def test_matching_formation_id_alone_is_not_success(self):
        # The applicant adopted the assignment but no peer traffic followed:
        # neither side sees a second live member.
        self.assertFalse(converged([
            view(members=1, alive=1),
            view(participation='catchingUp', members=1, alive=1),
        ]))

    def test_a_half_converged_formation_is_not_success(self):
        # The introducer counts the applicant but the applicant never caught up.
        self.assertFalse(converged([view(), view(members=1, alive=1)]))

    def test_pre_admission_states_are_not_success(self):
        for participation in ('standalone', 'joining', 'joinUnresolved'):
            self.assertFalse(
                converged([view(), view(participation=participation)]),
                participation,
            )

    def test_join_unresolved_is_not_treated_as_admission(self):
        # `joinUnresolved` means local adoption was never proven, so it is not
        # evidence that anything traversed the placed path.
        self.assertFalse(admitted_but_not_joined(
            [view(), view(participation='joinUnresolved')]
        ))

    def test_separate_formations_are_not_success(self):
        self.assertFalse(converged([view(formation='f1'), view(formation='f2')]))

    def test_missing_or_malformed_views_do_not_pass(self):
        self.assertFalse(converged(None))
        self.assertFalse(converged([view()]))
        # A renamed summary field must fail loudly rather than read as False,
        # which would look exactly like correct exclusion.
        with self.assertRaises(AssertionError):
            converged([view(), {'formationId': 'f1'}])


class Require(unittest.TestCase):
    def test_failures_raise_rather_than_warn(self):
        with self.assertRaises(AssertionError):
            require(False, 'boom')
        require(True, 'fine')


if __name__ == '__main__':
    unittest.main()
