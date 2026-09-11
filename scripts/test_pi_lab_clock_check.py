#!/usr/bin/env python3
"""Read-only clock session and collection; no real SSH/Pi dependency."""
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

import pi_lab_clock as clock
import pi_lab_experiment as experiment
from pi_lab_node import bounded_command


class ClockRemote:
    instances = []
    fail_role = None

    def __init__(self, node, run_id, mode, deadline):
        self.role, self.run_id, self.mode = node['role'], run_id, mode
        self.calls, self.closed, self.child = [], False, SimpleNamespace(returncode=None)
        self.instances.append(self)

    def call(self, operation, **args):
        self.calls.append(operation)
        if operation != 'clock' or self.mode != 'clock_only':
            raise AssertionError('non-clock action')
        if self.role == self.fail_role and len(self.calls) == 2:
            raise OSError('DO_NOT_EXPORT_SECRET')
        return clock.receipt(self.run_id, args['nonce'])

    def close(self):
        self.closed, self.child.returncode = True, 0


class ClockCheckContracts(unittest.TestCase):
    def setUp(self):
        ClockRemote.instances, ClockRemote.fail_role = [], None

    def bundled(self, requests, config=None):
        source = experiment.bundle()
        config = config or {'run_id': 'clock-test', 'mode': 'clock_only'}
        data = source + json.dumps(config).encode() + b'\n'
        data += b''.join(json.dumps(request).encode() + b'\n' for request in requests)
        response = bounded_command([sys.executable, '-u', '-c', experiment.BOOTSTRAP, str(len(source))],
                                   data=data, timeout=5)
        self.assertEqual(response['exit'], 0)
        self.assertNotIn('Traceback', response['stderr'])
        return [json.loads(line) for line in response['stdout'].splitlines()]

    def test_bundled_clock_only_session_needs_no_root_and_exits_cleanly_on_eof(self):
        rows = self.bundled([{'operation': 'clock', 'arguments': {'nonce': 'a' * 32}}])
        self.assertEqual(rows[0], {'ok': True, 'ready': True, 'run_id': 'clock-test', 'clock_only': True})
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[1]['result']['nonce'], 'a' * 32)
        clock.validate_stamp(rows[1]['result']['stamp'])

    def test_read_only_session_rejects_worker_filesystem_and_unrelated_operations(self):
        for operation in ('start', 'info', 'environment', 'prepare-load', 'measure', 'stop', 'shell'):
            rows = self.bundled([{'operation': operation, 'arguments': {}}])
            self.assertEqual(rows[-1], {'ok': False, 'error_type': 'ValueError'})
        rows = self.bundled([], {'run_id': 'clock-test', 'mode': 'clock_only', 'root': '/must-not-write'})
        self.assertEqual(rows, [{'ok': False, 'error_type': 'ValueError'}])

    def test_clock_only_nonce_and_command_count_are_bounded(self):
        request = {'operation': 'clock', 'arguments': {'nonce': 'a' * 32}}
        rows = self.bundled([request] * 17)
        self.assertEqual(sum('result' in row for row in rows), 16)
        self.assertEqual(rows[-1], {'ok': False, 'error_type': 'ValueError'})
        rows = self.bundled([{'operation': 'clock', 'arguments': {'nonce': True}}])
        self.assertEqual(rows[-1], {'ok': False, 'error_type': 'ValueError'})

    def test_three_to_five_nodes_capture_three_exchanges_and_no_worker_operations(self):
        with tempfile.TemporaryDirectory() as root:
            for count in (3, 4, 5):
                ClockRemote.instances = []
                output = Path(root) / str(count)
                value = experiment.clock_check({'nodes': [{'role': role} for role in range(count)]},
                                               output, remote_factory=ClockRemote)
                self.assertEqual(value['status'], 'complete_unqualified')
                self.assertIs(value['workers_started'], False)
                self.assertIs(value['clock_uncertainty_qualified'], False)
                self.assertTrue(all(len(row['exchanges']) == 3 for row in value['nodes']))
                self.assertTrue(all(remote.closed and remote.calls == ['clock'] * 3 for remote in ClockRemote.instances))
                for path in output.glob('*.json'):
                    self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_partial_exchange_failure_is_retained_and_all_sessions_close(self):
        ClockRemote.fail_role = 1
        with tempfile.TemporaryDirectory() as root:
            value = experiment.clock_check({'nodes': [{'role': role} for role in range(4)]},
                                           Path(root) / 'out', remote_factory=ClockRemote)
            self.assertEqual(value['status'], 'incomplete')
            self.assertEqual(len(value['nodes'][1]['exchanges']), 1)
            self.assertEqual(value['nodes'][1]['error_type'], 'OSError')
            self.assertNotIn('DO_NOT_EXPORT_SECRET', json.dumps(value))
            self.assertTrue(all(remote.closed for remote in ClockRemote.instances))

    def test_optional_policy_proposes_but_never_dispatches_or_retries(self):
        policy = {'lead_ns': 3_000_000_000, 'max_age_ns': 2_000_000_000,
                  'max_round_trip_ns': 100_000_000, 'max_offset_width_ns': 100_000_000,
                  'max_wall_change_ns': 1_000_000, 'max_relative_drift_ppm': 500,
                  'max_start_uncertainty_ns': 100_000_000, 'max_local_start_lateness_ns': 1_000_000,
                  'max_pairwise_start_skew_ns': 250_000_000}
        with tempfile.TemporaryDirectory() as root:
            for index, proposed in enumerate((policy, policy | {'max_round_trip_ns': 0})):
                ClockRemote.instances = []
                value = experiment.clock_check({'nodes': [{'role': role} for role in range(4)]},
                                               Path(root) / str(index), remote_factory=ClockRemote, start_policy=proposed)
                self.assertEqual(value['status'], 'complete_unqualified')
                self.assertEqual(value['proposed_start_policy'], proposed)
                if index == 0:
                    self.assertIs(value['start_proposal']['dispatch_authorized'], False)
                    self.assertEqual(len(value['start_proposal']['roles']), 4)
                else:
                    self.assertNotIn('start_proposal', value)
                    self.assertEqual(value['start_proposal_error_type'], 'ValueError')
                self.assertTrue(all(remote.calls == ['clock'] * 3 for remote in ClockRemote.instances))

    def test_invalid_policy_rejected_before_output_or_remote_session_creation(self):
        with tempfile.TemporaryDirectory() as root:
            output = Path(root) / 'out'
            with self.assertRaises(ValueError):
                experiment.clock_check({'nodes': [{'role': role} for role in range(4)]}, output,
                                       remote_factory=ClockRemote, start_policy={})
            self.assertFalse(output.exists())
            self.assertFalse(ClockRemote.instances)


if __name__ == '__main__':
    unittest.main()
