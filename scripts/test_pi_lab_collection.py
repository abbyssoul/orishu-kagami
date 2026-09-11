#!/usr/bin/env python3
"""Concurrent physical collection contracts, no hardware or timed load."""
import hashlib
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import pi_lab_clock as clock
import pi_lab_environment as environment
import pi_lab_experiment as experiment
import pi_lab_session as node_session
from test_pi_lab_results import config, measurement
from test_pi_lab_environment import receipt as environment_receipt
from test_pi_lab_overlap import fixture as overlap_fixture, exchange as overlap_exchange


class Prepared:
    def __init__(self, role, workers, barrier):
        self.run_id, self.mode = 'p-run', 'compiled_off'
        self.node = {'root': '/private', 'ssh_host': f'pi-{role}', 'peer_address': f'192.0.2.{role + 1}', 'interface': 'eth0'}
        self.expected_measurement = config(workers, role)
        self.expected_measurement['target']['node'] = f'n{role}'
        self.ready = {'artifacts': {'orishu-worker': 'a' * 64, 'orishuctl': 'b' * 64}}
        self.prepared_probe_sha256 = 'c' * 64
        self.measurement_attempted, self.deadline = False, time.monotonic() + 120
        self.barrier, self.calls = barrier, []
        self.failure, self.bad_target, self.bad_clock_after, self.completed = False, False, False, 5000
        self.bad_interface = False

    def call(self, operation, **arguments):
        self.calls.append(operation)
        if operation == 'environment':
            return environment_receipt(self.run_id, self.node['interface'])
        if operation == 'clock':
            if self.bad_clock_after and self.measurement_attempted:
                raise OSError('DO_NOT_EXPORT_SECRET')
            return clock.receipt(self.run_id, arguments['nonce'])
        if operation != 'measure':
            raise AssertionError(operation)
        if self.measurement_attempted:
            raise ValueError('replayed measurement')
        self.measurement_attempted = True
        self.barrier.wait(timeout=2)  # A serial implementation cannot pass.
        if self.failure:
            raise TimeoutError('DO_NOT_EXPORT_SECRET')
        value = measurement(self.expected_measurement['workers'], self.expected_measurement['role'], self.completed)
        value['end_unix_ns'] += 10_000
        value['load_control_bracket_ns'] += 10_000
        value['target'] = dict(self.expected_measurement['target'])
        if self.bad_target:
            value['target']['node'] = 'wrong'
        if self.bad_interface:
            value['environment_series']['interface'] = 'eth1'
            for row in value['environment_series']['samples']:
                row['snapshot']['interface'] = 'eth1'
        return value


class CollectionContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def cohort(self, count=4):
        barrier = threading.Barrier(count)
        return [Prepared(role, count, barrier) for role in range(count)]

    def collect(self, nodes, path='out', **kwargs):
        return experiment.collect_window(nodes, [10_000_000_000] * len(nodes), self.root / path, **kwargs)

    def test_explicit_timing_review_integrates_without_promoting_rate_or_acceptance(self):
        nodes = self.cohort()
        nodes[3].completed = 4900
        policy = overlap_fixture()[2]
        def exchange(call, run_id):
            self.assertEqual(run_id, 'p-run')
            node = call.__self__
            return overlap_exchange(node.expected_measurement['role'], int(node.measurement_attempted))
        with patch.object(clock, 'exchange', exchange):
            result = self.collect(nodes, timing_policy=policy)
        self.assertEqual(result['status'], 'complete_unqualified')
        timing = result['timing_review']
        self.assertEqual(timing['status'], 'within_conditional_bounds')
        self.assertIn('offered_rate_not_sustained', timing['nodes'][3]['measurement_issues'])
        self.assertIs(timing['acceptance_run'], False)
        self.assertIs(result['cleanup_verified'], False)
        manifest = json.loads((self.root / 'out' / 'manifest.json').read_text())
        self.assertEqual(manifest['timing_policy'], policy)
        self.assertEqual(json.loads((self.root / 'out' / 'collection.json').read_text())['timing_review'], timing)

    def test_invalid_timing_policy_fails_before_io_partial_collection_is_unqualified(self):
        nodes = self.cohort()
        with self.assertRaises(ValueError):
            self.collect(nodes, timing_policy={})
        self.assertFalse(any(node.calls for node in nodes))
        self.assertFalse((self.root / 'out').exists())
        nodes[0].bad_clock_after = True
        result = self.collect(nodes, timing_policy=overlap_fixture()[2])
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['timing_review']['status'], 'invalid_or_unavailable')
        self.assertTrue((self.root / 'out' / 'node-0.json').exists())

    def test_failed_overlap_does_not_discard_complete_raw_collection(self):
        # These test doubles deliberately return old measurement timestamps
        # with current exchange clocks. The raw receipts are valid individually
        # but cannot qualify a shared window within their clock brackets.
        result = self.collect(self.cohort(), timing_policy=overlap_fixture()[2])
        self.assertEqual(result['status'], 'complete_unqualified')
        self.assertEqual(result['timing_review']['status'], 'outside_conditional_bounds')
        self.assertTrue(all((self.root / 'out' / f'node-{role}.json').exists() for role in range(4)))

    def test_three_to_five_concurrent_roles_private_digest_checked_files(self):
        for count in (3, 4, 5):
            nodes = self.cohort(count)
            result = self.collect(nodes, str(count))
            self.assertEqual(result['status'], 'complete_unqualified')
            self.assertEqual([row['role'] for row in result['nodes']], list(range(count)))
            self.assertIs(result['acceptance_run'], False)
            self.assertIs(result['clock_uncertainty_qualified'], False)
            self.assertIs(result['cleanup_verified'], False)
            for node, row in zip(nodes, result['nodes']):
                self.assertEqual(node.calls, ['environment', 'clock', 'measure', 'clock', 'environment'])
                path = self.root / str(count) / row['file']
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), row['sha256'])
                self.assertEqual(row['review']['target']['node'], node.expected_measurement['target']['node'])
                self.assertIn('clock_before', row)
                self.assertIn('clock_after', row)
                self.assertIn('environment', row)
            for path in (self.root / str(count)).glob('*.json'):
                self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_failed_role_retains_peers_without_retry_or_secret_leak(self):
        nodes = self.cohort()
        nodes[1].failure = True
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['nodes'][1]['status'], 'invalid_or_unavailable')
        self.assertEqual(result['nodes'][1]['error_type'], 'TimeoutError')
        self.assertEqual(sum(row['status'] == 'valid_unqualified' for row in result['nodes']), 3)
        self.assertFalse((self.root / 'out' / 'node-1.json').exists())
        for node in nodes:
            self.assertEqual(node.calls.count('measure'), 1)
        for path in (self.root / 'out').glob('*.json'):
            self.assertNotIn('DO_NOT_EXPORT_SECRET', path.read_text())

    def test_invalid_target_is_not_persisted_as_measurement(self):
        nodes = self.cohort()
        nodes[0].bad_target = True
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse((self.root / 'out' / 'node-0.json').exists())
        self.assertEqual(result['nodes'][0]['error_type'], 'ValueError')

    def test_periodic_interface_must_match_trusted_inventory(self):
        nodes = self.cohort()
        nodes[0].bad_interface = True
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse((self.root / 'out' / 'node-0.json').exists())
        self.assertEqual(result['nodes'][0]['error_type'], 'ValueError')

    def test_failed_post_clock_retains_valid_measurement_without_completing(self):
        nodes = self.cohort()
        nodes[2].bad_clock_after = True
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertIn('review', result['nodes'][2])
        self.assertTrue((self.root / 'out' / 'node-2.json').exists())
        self.assertNotIn('clock_after', result['nodes'][2])

    def test_throughput_failure_not_hidden_by_complete_collection(self):
        nodes = self.cohort()
        nodes[3].completed = 4900
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'complete_unqualified')
        self.assertIn('offered_rate_not_sustained', result['nodes'][3]['review']['measurement_issues'])
        self.assertIs(result['acceptance_run'], False)

    def test_mixed_manifest_inputs_fail_before_any_dispatch(self):
        changes = [lambda n: setattr(n[1], 'run_id', 'other'),
                   lambda n: setattr(n[1], 'mode', 'omitted'),
                   lambda n: setattr(n[1], 'measurement_attempted', True),
                   lambda n: n[1].expected_measurement.update(role=0),
                   lambda n: n[1].expected_measurement.update(workers=3),
                   lambda n: n[1].expected_measurement['target'].update(node='n0'),
                   lambda n: n[1].expected_measurement['target'].update(formation='other'),
                   lambda n: n[1].expected_measurement['target'].update(socket='/other/api.sock'),
                   lambda n: n[1].ready['artifacts'].update(orishuctl='d' * 64),
                   lambda n: setattr(n[1], 'prepared_probe_sha256', 'd' * 64),
                   lambda n: n[1].node.update(ssh_host='pi-0'),
                   lambda n: n[1].node.update(peer_address='192.0.2.1')]
        for change in changes:
            nodes = self.cohort()
            change(nodes)
            with self.assertRaises(ValueError):
                self.collect(nodes)
            self.assertFalse(any(node.calls for node in nodes))
            self.assertFalse((self.root / 'out').exists())

    def test_existing_output_and_expired_deadlines_do_not_replay(self):
        nodes = self.cohort()
        with self.assertRaises(FileExistsError):
            experiment.collect_window(nodes, [10_000_000_000] * 4, self.root)
        self.assertFalse(any(node.calls for node in nodes))
        for node in nodes:
            node.deadline = time.monotonic() - 1
        result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse(any(node.calls for node in nodes))

    def test_partial_artifact_write_failure_retains_other_roles(self):
        nodes = self.cohort()
        original = experiment.write_new_json
        def write(path, value):
            if path.name == 'node-1.json':
                raise OSError('disk full secret')
            original(path, value)
        with patch.object(experiment, 'write_new_json', write):
            result = self.collect(nodes)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['nodes'][1]['error_type'], 'OSError')
        self.assertTrue((self.root / 'out' / 'node-3.json').exists())

    def test_concurrent_serialized_remote_and_node_session_paths(self):
        # Real coordinator framing, deadlines, one-shot guard and result reader
        # meet the real node request allowlist. Only the prepared load itself
        # is a fixture; this does not claim a hardware or sampler timing run.
        prepared, remotes, streams, threads, errors = self.cohort(3), [], [], [], []
        for fixture in prepared:
            to_read, to_write = os.pipe()
            from_read, from_write = os.pipe()
            incoming, outgoing = os.fdopen(to_read, 'rb', buffering=0), os.fdopen(from_write, 'wb', buffering=0)
            client_in, client_out = os.fdopen(to_write, 'wb', buffering=0), os.fdopen(from_read, 'rb', buffering=0)
            streams.extend((incoming, outgoing, client_in, client_out))
            remote = object.__new__(experiment.Remote)
            remote.__dict__.update(fixture.__dict__)
            remote.child = SimpleNamespace(stdin=client_in, stdout=client_out)
            remotes.append(remote)
            node = object.__new__(node_session.Session)
            node.run_id = fixture.run_id
            node.interface = fixture.node['interface']
            node.measurement = SimpleNamespace(run=lambda start, f=fixture: f.call('measure', start_unix_ns=start))
            def serve(node=node, incoming=incoming, outgoing=outgoing):
                try:
                    while True:
                        try:
                            raw = node_session.line(incoming, time.monotonic() + 3)
                        except EOFError:
                            break
                        value = node.request(node_session.decode(raw))
                        outgoing.write(json.dumps({'ok': True, 'result': value}).encode() + b'\n')
                except Exception as error:
                    errors.append(type(error).__name__)
            thread = threading.Thread(target=serve)
            thread.start()
            threads.append(thread)
        try:
            with patch.object(environment, 'snapshot', side_effect=environment_receipt):
                result = self.collect(remotes, timing_policy=overlap_fixture()[2])
            self.assertEqual(result['status'], 'complete_unqualified')
            # Real current clock RPCs cannot qualify fixture-era load stamps.
            self.assertEqual(result['timing_review']['status'], 'outside_conditional_bounds')
            self.assertTrue(all(remote.measurement_attempted for remote in remotes))
        finally:
            for remote in remotes:
                remote.child.stdin.close()
            for thread in threads:
                thread.join(timeout=4)
            for stream in streams:
                stream.close()
        self.assertFalse(errors)
        self.assertFalse(any(thread.is_alive() for thread in threads))


if __name__ == '__main__':
    unittest.main()
