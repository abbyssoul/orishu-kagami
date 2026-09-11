#!/usr/bin/env python3
"""Physical result reader contracts; no Pi session or worker load."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import pi_lab_experiment as experiment
import pi_lab_results as results
from test_pi_lab_measurement import report
from test_pi_lab_environment import series


def config(workers=4, role=3):
    return {'schema_version': 2, 'workers': workers, 'role': role,
            'target': {'socket': '/private/runs/p-run/api.sock', 'formation': 'f', 'node': 'n'}}


def measurement(workers=4, role=3, completed=5000):
    resource = {'cpu_nanoseconds': 250_000_000, 'cpu_seconds': 0.25, 'cpu_bracket_ns': 10_010_000_000,
                'rss_peak_kib': 120, 'swap_peak_kib': 0, 'setup_hwm_kib': 130, 'lifetime_hwm_kib': 140}
    return {'schema_version': 2, 'kind': 'pi-node-measurement', 'load': report(workers, role, completed),
            'environment_series': series(),
            'target': config(workers, role)['target'],
            'resources': {name: dict(resource) for name in ('worker', 'probe', 'supervisor')},
            'scheduled_start_unix_ns': 10_000_000_000, 'start_unix_ns': 10_001_000_000,
            'end_unix_ns': 20_001_000_000, 'local_start_lateness_ns': 1_000_000,
            'local_wall_mapping_change_ns': 0, 'window_wall_change_ns': 0,
            'load_control_bracket_ns': 10_000_000_000, 'samples': 200, 'missed_samples': 0,
            'issues': ['offered_rate_not_sustained'] if completed < 4950 else [],
            'clock_uncertainty_qualified': False, 'acceptance_run': False}


class ReaderContracts(unittest.TestCase):
    def test_three_through_five_nodes_preserve_separate_accounting(self):
        for workers in (3, 4, 5):
            value = measurement(workers, workers - 1)
            value['resources']['probe']['cpu_nanoseconds'] *= 2
            value['resources']['probe']['cpu_seconds'] *= 2
            reviewed = results.review(value, config(workers, workers - 1))
            self.assertEqual(reviewed['status'], 'valid_unqualified')
            self.assertEqual(reviewed['qualification_issues'], ['distributed_window_unqualified'])
            rates = reviewed['cpu_percent_of_one_core']
            self.assertAlmostEqual(rates['worker'], 100 * 0.25 / 10.01)
            self.assertEqual(rates['probe'], 2 * rates['worker'])
            self.assertIs(reviewed['acceptance_run'], False)

    def test_missing_node_wrong_run_identity_role_or_profile_rejected(self):
        for key, wrong in (('socket', '/private/runs/other/api.sock'), ('formation', 'other'), ('node', 'other')):
            value = measurement()
            value['target'][key] = wrong
            with self.assertRaises(ValueError):
                results.review(value, config())
        for expected in (config(3, 2), config(4, 2), config() | {'role': True}, config() | {'workers': 6}):
            with self.assertRaises(ValueError):
                results.review(measurement(), expected)

    def test_overclaim_unknown_fields_and_malformed_nested_values_rejected(self):
        for value in (measurement() | {'acceptance_run': True}, measurement() | {'schema_version': True},
                      measurement() | {'schema_version': 1},
                      measurement() | {'clock_uncertainty_qualified': True}, measurement() | {'secret': 'no'},
                      measurement() | {'resources': []}, measurement() | {'load': None}):
            with self.assertRaises(ValueError):
                results.review(value, config())
        for nested in (None, 'secret', [], True):
            for key in ('latency', 'scheduled_latency', 'scheduling_delay'):
                value = measurement()
                value['load']['workers'][0][key] = nested
                with self.assertRaises(ValueError):
                    results.review(value, config())

    def test_unmet_rate_and_sampler_gates_remain_valid_evidence(self):
        for completed in (0, 4949, 4950, 5000):
            value = measurement(completed=completed)
            value['samples'], value['missed_samples'] = 199, 1
            value['issues'].append('resource_sampling_missed')
            reviewed = results.review(value, config())
            self.assertEqual(reviewed['measurement_issues'], value['issues'])
            self.assertEqual(reviewed['status'], 'valid_unqualified')
        value['issues'] = []
        with self.assertRaises(ValueError):
            results.review(value, config())

    def test_resource_types_missing_instruments_and_accounting_rejected(self):
        for key, bad in (('cpu_seconds', float('nan')), ('cpu_seconds', True), ('cpu_seconds', 0.3),
                         ('cpu_nanoseconds', -1), ('cpu_bracket_ns', 0), ('rss_peak_kib', 0),
                         ('lifetime_hwm_kib', 119), ('swap_peak_kib', None), ('extra', 'secret')):
            value = measurement()
            value['resources']['worker'][key] = bad
            with self.assertRaises(ValueError):
                results.review(value, config())
        value = measurement()
        del value['resources']['supervisor']
        with self.assertRaises(ValueError):
            results.review(value, config())

    def test_wall_changes_retained_but_false_time_accounting_rejected(self):
        value = measurement()
        value['window_wall_change_ns'] = -11_000_000_000
        value['end_unix_ns'] -= 11_000_000_000
        self.assertEqual(results.review(value, config())['status'], 'valid_unqualified')
        for key, bad in (('start_unix_ns', 10_000_000_000), ('window_wall_change_ns', 0),
                         ('samples', 201), ('samples', 0), ('samples', True), ('missed_samples', -1),
                         ('local_start_lateness_ns', 2**63)):
            with self.assertRaises(ValueError):
                results.review(value | {key: bad}, config())

    def test_file_digest_duplicate_json_and_byte_bounds(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'report.json'
            raw = json.dumps(measurement()).encode()
            path.write_bytes(raw)
            digest = hashlib.sha256(raw).hexdigest()
            self.assertEqual(results.read_report(path, config(), digest)['source_sha256'], digest)
            with self.assertRaises(ValueError):
                results.read_report(path, config(), '0' * 64)
            for raw in (b'{"schema_version":1,"schema_version":1}', b'{"x":NaN}', b' ' * 65537):
                path.write_bytes(raw)
                with self.assertRaises(ValueError):
                    results.read_report(path, config(), hashlib.sha256(raw).hexdigest())

    def test_real_cli_exits_zero_only_for_valid_receipt_not_acceptance(self):
        with tempfile.TemporaryDirectory() as root:
            report_path, expected_path = Path(root) / 'report.json', Path(root) / 'expected.json'
            raw = json.dumps(measurement()).encode()
            report_path.write_bytes(raw)
            expected_path.write_text(json.dumps(config()))
            command = [sys.executable, results.__file__, '--report', str(report_path), '--expected-config',
                       str(expected_path), '--sha256', hashlib.sha256(raw).hexdigest()]
            completed = subprocess.run(command, capture_output=True, timeout=3)
            self.assertEqual(completed.returncode, 0)
            self.assertIs(json.loads(completed.stdout)['acceptance_run'], False)
            report_path.write_text('DO_NOT_EXPORT_SECRET')
            completed = subprocess.run(command, capture_output=True, timeout=3)
            self.assertEqual(completed.returncode, 1)
            self.assertEqual(json.loads(completed.stdout)['status'], 'invalid_or_unavailable')
            self.assertNotIn(b'DO_NOT_EXPORT_SECRET', completed.stdout + completed.stderr)

    def test_remote_measure_call_rejects_mismatched_received_target(self):
        remote = object.__new__(experiment.Remote)
        remote.measurement_attempted = False
        remote.expected_measurement = config()
        remote.write = lambda _: None
        value = measurement()
        value['target']['node'] = 'other'
        remote.read = lambda _: {'ok': True, 'result': value}
        with self.assertRaises(ValueError):
            remote.call('measure', start_unix_ns=10_000_000_000)
        # A fresh session is required after an attempted measurement, even if
        # its reply was invalid. Reset only this test double for the next case.
        self.assertTrue(remote.measurement_attempted)
        with self.assertRaises(ValueError):
            remote.call('measure', start_unix_ns=10_000_000_000)
        remote.measurement_attempted = False
        remote.read = lambda _: {'ok': True, 'result': measurement()}
        remote.poisoned = False
        self.assertEqual(remote.call('measure', start_unix_ns=10_000_000_000), measurement())
        remote.measurement_attempted = False
        remote.poisoned = False
        with self.assertRaises(ValueError):
            remote.call('measure', start_unix_ns=11_000_000_000)


if __name__ == '__main__':
    unittest.main()
