#!/usr/bin/env python3
"""Pilot orchestration with local clock/worker doubles; never SSH or Pi load."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

import pi_lab_experiment as experiment
from test_pi_lab import inventory, lab
from test_pi_lab_experiment import FakeRemote
from test_pi_lab_environment import receipt, series
from test_pi_lab_overlap import fixture, shift
from test_pi_lab_results import measurement


def policy():
    return {'schema_version': 1, 'max_elapsed_seconds': 300,
            'start': {'lead_ns': 8_000_000_000, 'max_age_ns': 1_000_000_000,
                      'max_round_trip_ns': 1_000_000, 'max_offset_width_ns': 1_000_000,
                      'max_wall_change_ns': 10_000, 'max_relative_drift_ppm': 0,
                      'max_start_uncertainty_ns': 1_000_000, 'max_local_start_lateness_ns': 1_000_000,
                      'max_pairwise_start_skew_ns': 3_000_000},
            'overlap': fixture()[2] | {'max_collection_span_ns': 30_000_000_000}}


class Clock:
    ns = 1_000_000_000
    wall_origin = 1_700_000_000_000_000_000
    def mono(self):
        self.ns += 100
        return self.ns
    def wall(self):
        return self.mono() + self.wall_origin
    def seconds(self):
        return self.ns / 1e9
    def sleep(self, seconds):
        self.ns += max(1, int(seconds * 1e9))


class PilotRemote(FakeRemote):
    pool, history = [], []
    fail_prepare = fail_measure = fail_cleanup = bad_artifact = expire_prepare = None
    rate = 5000
    clock = None
    barrier = None

    def __init__(self, node, run_id, mode, deadline):
        super().__init__(node, run_id, mode, deadline)
        self.run_id, self.mode, self.deadline = run_id, mode, deadline
        self.initial_deadline = deadline
        self.prepared, self.closed, self.measurement_attempted = False, False, False
        self.calls = []
        binary = 'orishu-worker-omitted' if mode == 'omitted' else 'orishu-worker'
        self.ready['artifacts'] = {binary: 'a' * 64, 'orishuctl': 'b' * 64}
        if self.index == self.bad_artifact:
            self.ready['artifacts']['orishuctl'] = 'd' * 64

    def call(self, op, **args):
        self.calls.append(op)
        if self.clock.seconds() >= self.deadline:
            raise TimeoutError('original session deadline')
        if op == 'clock':
            if not all(node.prepared for node in self.pool):
                raise AssertionError('clock exchange before all probes READY')
        if op == 'prepare-load':
            if self.index == self.fail_prepare:
                raise OSError('DO_NOT_EXPORT_SECRET')
            self.prepared = True
            if self.index == self.expire_prepare:
                self.clock.ns += 120_000_000_000
            self.prepared_probe_sha256 = args['probe_sha256']
            self.expected_measurement = {'schema_version': 2, 'workers': args['workers'], 'role': args['role'],
                                         'target': {'formation': args['formation'], 'node': args['node'],
                                                    'socket': str(Path(self.node['root']) / 'runs' / self.run_id / 'api.sock')}}
        if op == 'environment':
            return receipt(self.run_id, self.node['interface'])
        if op == 'measure':
            if self.measurement_attempted:
                raise AssertionError('measurement replay')
            self.measurement_attempted = True
            self.start = args['start_unix_ns']
            self.barrier.wait(timeout=2)
            if self.index == self.fail_measure:
                raise OSError('DO_NOT_EXPORT_SECRET')
            value = measurement(len(self.pool), self.index, self.rate)
            value['target'] = dict(self.expected_measurement['target'])
            value['environment_series'] = series(self.run_id, self.node['interface'])
            shift(value, self.start - value['scheduled_start_unix_ns'])
            value['end_unix_ns'] += 10_000
            value['load_control_bracket_ns'] += 10_000
            return value
        return super().call(op, **args)

    def stop(self):
        self.closed = True
        return {'clean': self.index != self.fail_cleanup}


class PilotContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.reset()

    def reset(self, count=4):
        PilotRemote.pool, PilotRemote.history = [], []
        PilotRemote.fail_prepare = PilotRemote.fail_measure = PilotRemote.fail_cleanup = PilotRemote.bad_artifact = PilotRemote.expire_prepare = None
        PilotRemote.rate = 5000
        self.clock = PilotRemote.clock = Clock()
        def end_window():
            self.clock.ns = max(node.start for node in PilotRemote.pool) - self.clock.wall_origin + 11_000_000_000
        PilotRemote.barrier = threading.Barrier(count, action=end_window)

    def run_pilot(self, count=4, directory='out', selected=None, mode='compiled_off'):
        with patch.object(experiment.time, 'monotonic_ns', self.clock.mono), \
             patch.object(experiment.time, 'time_ns', self.clock.wall), \
             patch.object(experiment.time, 'monotonic', self.clock.seconds), \
             patch.object(experiment.time, 'sleep', self.clock.sleep):
            return experiment.pilot(inventory(count), self.root / directory, selected or policy(), 'c' * 64,
                                    mode, remote_factory=PilotRemote)

    def test_three_to_five_full_paths_one_window_fresh_clocks_and_cleanup(self):
        for count in (3, 4, 5):
            self.reset(count)
            result = self.run_pilot(count, str(count))
            self.assertEqual(result['status'], 'complete_unqualified', result.get('findings'))
            self.assertTrue(result['cleanup_clean'])
            self.assertTrue(result['timed_window_started'])
            self.assertEqual(result['measurement_dispatch_attempts'], count)
            self.assertEqual(result['findings'], [])
            self.assertIs(result['acceptance_run'], False)
            self.assertIs(result['clock_uncertainty_qualified'], False)
            self.assertIn('after-window', [row['stage'] for row in result['events']])
            self.assertEqual(len({node.initial_deadline for node in PilotRemote.pool}), 1)
            for node in PilotRemote.pool:
                self.assertTrue(node.closed)
                self.assertEqual(node.calls.count('prepare-load'), 1)
                self.assertEqual(node.calls.count('measure'), 1)
                self.assertEqual(node.calls.count('clock'), 5)
                self.assertNotIn('leave', node.calls)
                self.assertNotIn('stop-load', node.calls)
                self.assertLessEqual(node.deadline, node.initial_deadline)
            for path in (self.root / str(count)).rglob('*.json'):
                self.assertNotIn('NEVER_EXPORT_SECRET', path.read_text())
                self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_omitted_mode_uses_omitted_worker_profile(self):
        result = self.run_pilot(mode='omitted')
        self.assertEqual(result['status'], 'complete_unqualified')
        self.assertTrue(all('orishu-worker-omitted' in node.ready['artifacts'] for node in PilotRemote.pool))

    def test_failed_preparation_never_dispatches_and_stops_every_session(self):
        PilotRemote.fail_prepare = 1
        result = self.run_pilot()
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse(result['timed_window_started'])
        self.assertEqual(result['measurement_dispatch_attempts'], 0)
        self.assertTrue(all(node.closed and 'measure' not in node.calls for node in PilotRemote.pool))
        self.assertNotIn('DO_NOT_EXPORT_SECRET', json.dumps(result))

    def test_failed_collection_preserves_other_reports_without_retry(self):
        PilotRemote.fail_measure = 2
        result = self.run_pilot()
        self.assertEqual(result['status'], 'incomplete')
        self.assertTrue(result['timed_window_started'])
        self.assertEqual(result['measurement_dispatch_attempts'], 4)
        self.assertTrue(all(node.closed and node.calls.count('measure') == 1 for node in PilotRemote.pool))
        self.assertTrue((self.root / 'out/window/node-0.json').exists())
        self.assertFalse((self.root / 'out/window/node-2.json').exists())
        self.assertNotIn('DO_NOT_EXPORT_SECRET', json.dumps(result))

    def test_unmet_rate_is_completed_with_findings_not_acceptance_or_retry(self):
        PilotRemote.rate = 4900
        result = self.run_pilot()
        self.assertEqual(result['status'], 'complete_with_findings')
        self.assertEqual(len(result['findings']), 4)
        self.assertTrue(result['cleanup_clean'])
        self.assertEqual(result['measurement_dispatch_attempts'], 4)

    def test_cleanup_failure_prevents_success_even_with_valid_measurements(self):
        PilotRemote.fail_cleanup = 0
        result = self.run_pilot()
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse(result['cleanup_clean'])
        self.assertEqual(result['collection']['status'], 'complete_unqualified')

    def test_mixed_artifacts_and_failed_start_policy_cannot_reach_load(self):
        PilotRemote.bad_artifact = 1
        result = self.run_pilot(directory='artifacts')
        self.assertEqual(result['status'], 'incomplete')
        self.assertFalse(result['timed_window_started'])
        self.assertTrue(all(node.closed and 'prepare-load' not in node.calls for node in PilotRemote.pool))
        self.reset()
        selected = policy()
        selected['start']['max_round_trip_ns'] = 0
        result = self.run_pilot(directory='clock', selected=selected)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['measurement_dispatch_attempts'], 0)
        self.assertTrue(all(node.closed for node in PilotRemote.pool))
        self.assertTrue(result['clock_exchanges'])

    def test_invalid_policy_mode_or_probe_fails_before_files_or_sessions(self):
        for selected in ({}, policy() | {'max_elapsed_seconds': 120}, policy() | {'schema_version': True},
                         policy() | {'overlap': {}}):
            with self.assertRaises(ValueError):
                experiment.pilot(inventory(), self.root / 'invalid', selected, 'c' * 64, remote_factory=PilotRemote)
        for mode, digest in (('metrics', 'c' * 64), ('compiled_off', None), ('compiled_off', 'bad')):
            with self.assertRaises(ValueError):
                experiment.pilot(inventory(), self.root / 'invalid', policy(), digest, mode, remote_factory=PilotRemote)
        self.assertFalse(PilotRemote.pool)
        self.assertFalse((self.root / 'invalid').exists())

    def test_original_setup_deadline_is_not_reset_after_warmup(self):
        PilotRemote.expire_prepare = 0
        result = self.run_pilot()
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['measurement_dispatch_attempts'], 0)
        self.assertTrue(all(node.closed for node in PilotRemote.pool))
        self.assertTrue(all(node.deadline == node.initial_deadline for node in PilotRemote.pool))

    def test_collection_clock_bracket_includes_start_wait_not_only_load(self):
        selected = policy()
        selected['overlap']['max_collection_span_ns'] = 10_000_000_000
        result = self.run_pilot(selected=selected)
        self.assertEqual(result['status'], 'complete_with_findings')
        self.assertEqual(result['findings'], ['timing_bounds_not_met_or_unavailable'])
        self.assertEqual(result['measurement_dispatch_attempts'], 4)
        self.assertTrue(result['cleanup_clean'])

    def test_documented_policy_example_has_the_actual_required_contract(self):
        path = Path(__file__).resolve().parents[1] / 'etc/experiments/pi-pilot-policy.example.json'
        selected = json.loads(path.read_text())
        experiment.validate_pilot_policy(selected)
        self.assertEqual(selected['max_elapsed_seconds'], 300)
        self.assertFalse(PilotRemote.pool)

    def test_cli_requires_policy_and_routes_explicit_pilot_without_inventing_approval(self):
        inventory_path, policy_path = self.root / 'inventory.json', self.root / 'policy.json'
        inventory_path.write_text(json.dumps(inventory()))
        policy_path.write_text(json.dumps(policy()))
        command = ['pi-lab.py', 'pilot', '--inventory', str(inventory_path), '--output', str(self.root / 'cli'),
                   '--probe-sha256', 'c' * 64]
        with patch.object(sys, 'argv', command), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(lab.main(), 2)
        for status, code in (('complete_unqualified', 0), ('complete_with_findings', 2), ('incomplete', 2)):
            with patch.object(sys, 'argv', command + ['--pilot-policy', str(policy_path)]), \
                 patch.object(experiment, 'pilot', return_value={'status': status, 'acceptance_run': False}) as pilot, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(lab.main(), code)
                self.assertEqual(pilot.call_args.args[2], policy())
        for raw in ('{"schema_version":1,"schema_version":1}', ' ' * 4097):
            policy_path.write_text(raw)
            with patch.object(sys, 'argv', command + ['--pilot-policy', str(policy_path)]), \
                 patch.object(experiment, 'pilot') as pilot, contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(lab.main(), 2)
                pilot.assert_not_called()


if __name__ == '__main__':
    unittest.main()
