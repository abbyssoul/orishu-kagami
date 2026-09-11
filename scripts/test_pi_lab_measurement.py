#!/usr/bin/env python3
"""Local sampler/physical-report tests; no worker or SSH load experiment."""
import os
import io
import json
from pathlib import Path
import subprocess
import sys
import time
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import pi_lab_measurement as measurement
import pi_lab_environment as environment


def report(workers=4, role=3, completed=5000):
    latency = {'requests': completed, 'median_us': 10 if completed else None, 'p95_us': 20 if completed else None}
    return {'schema_version': 5, 'arrival_profile': 'fixed_500_per_worker_physical_pilot_v2',
            'timing': {'window_ns': 10_000_000_000, 'start': {'unix_ns': 10_001_000_000, 'read_bracket_ns': 100},
                       'end_marker': {'unix_ns': 20_001_000_000, 'read_bracket_ns': 100}, 'end_marker_elapsed_ns': 10_000_000_000},
            'global_workers': workers, 'seconds': 10, 'workers': [{
                'role': role, 'latency': dict(latency), 'capped': False, 'tail_requests': 0,
                'scheduled_arrivals': 5000, 'skipped_arrivals': 5000 - completed,
                'scheduled_latency': dict(latency), 'scheduling_delay': dict(latency),
                'activity': [{'client_index': index, 'timed_requests': count,
                              'first_request_offset_ns': 1_000_000 if count else None,
                              'last_timed_completion_offset_ns': 9_900_000_000 if count else None}
                             for index, count in enumerate((completed // 2, completed - completed // 2))]}]}


class Contracts(unittest.TestCase):
    def test_actual_process_cpu_memory_and_identity(self):
        process = measurement.Process(os.getpid())
        before = process.sample()
        sum(i * i for i in range(10000))
        after = process.sample()
        result = measurement.resource_delta(before, after, before['rss_kib'], before['swap_kib'])
        self.assertGreater(result['cpu_nanoseconds'], 0)
        self.assertGreater(result['cpu_bracket_ns'], 0)
        self.assertGreater(result['rss_peak_kib'], 0)
        self.assertEqual(process.initial['parent'], os.getppid())

    def test_pid_reuse_or_changed_executable_is_rejected(self):
        process = measurement.Process(os.getpid())
        process.initial['start_ticks'] -= 1
        with self.assertRaises(ValueError):
            process.sample()
        with self.assertRaises(ValueError):
            measurement.Process(os.getpid(), '/definitely/not/the/executable')
        with self.assertRaises(ValueError):
            measurement.Process(os.getpid(), parent=-1)

    def test_unavailable_cpu_clock_is_not_zero(self):
        process = measurement.Process(os.getpid())
        with patch.object(measurement, 'clock_function', return_value=lambda *_: 3):
            with self.assertRaises(OSError):
                process.sample()

    def test_watchdog_child_is_actual_process_not_wrapper(self):
        watchdog = subprocess.Popen(['timeout', '--preserve-status', '--kill-after=1s', '5s',
                                     sys.executable, '-c', 'import time; time.sleep(4)'], start_new_session=True)
        try:
            deadline = time.monotonic() + 2
            while True:
                try:
                    process = measurement.watchdog_child(watchdog, Path(sys.executable).resolve())
                    break
                except ValueError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.01)
            self.assertNotEqual(process.pid, watchdog.pid)
            self.assertEqual(process.initial['parent'], watchdog.pid)
            self.assertGreater(process.sample()['rss_kib'], 0)
        finally:
            watchdog.terminate()
            watchdog.wait(timeout=2)
        with self.assertRaises(ValueError):
            measurement.watchdog_child(watchdog, Path(sys.executable).resolve())

    def test_three_to_five_global_members_and_rate_failures(self):
        for workers in (3, 4, 5):
            self.assertEqual(measurement.load_issues(report(workers, workers - 1), workers, workers - 1), [])
        self.assertEqual(measurement.load_issues(report(completed=4949), 4, 3), ['offered_rate_not_sustained'])
        self.assertEqual(measurement.load_issues(report(completed=0), 4, 3), ['offered_rate_not_sustained'])

    def test_wrong_role_profile_and_arrival_accounting_rejected(self):
        for key, value in (('global_workers', 3), ('schema_version', 3), ('seconds', 11),
                           ('seconds', True), ('arrival_profile', 'saturation_v2'), ('extra_secret', 'no')):
            row = report()
            row[key] = value
            with self.assertRaises(ValueError):
                measurement.load_issues(row, 4, 3)
        for key, value in (('role', 2), ('capped', 'false'), ('tail_requests', 1), ('scheduled_arrivals', True)):
            row = report()
            row['workers'][0][key] = value
            with self.assertRaises(ValueError):
                measurement.load_issues(row, 4, 3)

    def test_old_physical_profile_and_missing_probe_clock_are_rejected(self):
        for change in (lambda value: value.update(schema_version=4),
                       lambda value: value.update(arrival_profile='fixed_500_per_worker_physical_pilot_v1'),
                       lambda value: value.pop('timing')):
            value = report()
            change(value)
            with self.assertRaises(ValueError):
                measurement.load_issues(value, 4, 3)

    def test_probe_timing_and_activity_are_strict_and_count_consistent(self):
        for field, invalid in (('client_index', True), ('client_index', 1), ('timed_requests', 2499),
                               ('first_request_offset_ns', None), ('first_request_offset_ns', 10_000_000_001),
                               ('last_timed_completion_offset_ns', None), ('extra', 'secret')):
            value = report()
            value['workers'][0]['activity'][0][field] = invalid
            with self.assertRaises(ValueError):
                measurement.load_issues(value, 4, 3)
        for invalid in ({}, {'unix_ns': True, 'read_bracket_ns': 1}, {'unix_ns': 2**63 - 1, 'read_bracket_ns': 1}):
            value = report()
            value['timing']['start'] = invalid
            with self.assertRaises(ValueError):
                measurement.load_issues(value, 4, 3)

    def test_probe_wall_step_is_retained_as_interval_not_zeroed(self):
        value = report()['timing']
        bounds = measurement.probe_clock_bounds(value)
        self.assertEqual(bounds['wall_change_interval_ns'], [-100, 100])
        value['end_marker']['unix_ns'] -= 5_000_000
        self.assertEqual(measurement.probe_clock_bounds(value)['wall_change_interval_ns'], [-5_000_100, -4_999_900])

    def test_nonfinite_negative_and_missing_latency_values_rejected(self):
        for number in (float('nan'), float('inf'), -1, None, True):
            row = report()
            row['workers'][0]['latency']['p95_us'] = number
            with self.assertRaises(ValueError):
                measurement.load_issues(row, 4, 3)
        row = report()
        row['workers'][0]['scheduled_latency']['requests'] = 4999
        with self.assertRaises(ValueError):
            measurement.load_issues(row, 4, 3)

    def test_start_must_be_future_bounded_and_cannot_replay(self):
        instance = object.__new__(measurement.Measurement)
        instance.ran = False
        for start in (True, time.time_ns(), time.time_ns() + 30_000_000_000):
            with self.assertRaises(ValueError):
                instance.run(start)
        instance.ran = True
        with self.assertRaises(ValueError):
            instance.run(time.time_ns() + 3_000_000_000)

    def test_report_decoder_rejects_duplicate_oversized_nonfinite_json(self):
        for raw in (b'{"workers":3,"workers":4}', b'{"value":NaN}', b' ' * 16385):
            with self.assertRaises(ValueError):
                measurement.decode_report(raw)

    def test_local_window_records_per_process_cpu_and_explicit_uncertainty(self):
        # Exercise the production window loop with deterministic caller clocks;
        # the separate process tests exercise actual Linux accounting/identity.
        for cost_ns in (100_000, 30_000_000):
            class Clock:
                ns = 1_000_000_000
                def mono(self):
                    return self.ns
                def wall(self):
                    return self.ns + 1_700_000_000_000_000_000
                def seconds(self):
                    return self.ns / 1e9
                def sleep(self, seconds):
                    self.ns += max(1, int(seconds * 1e9))
            clock = Clock()
            class Process:
                cpu = 0
                def sample(self):
                    self.cpu += 1000
                    clock.ns += cost_ns
                    return {'cpu_ns': self.cpu, 'observed_monotonic_ns': clock.mono(),
                            'rss_kib': 100, 'hwm_kib': 110, 'swap_kib': 0}
            class Input:
                closed = False
                data = b''
                def write(self, data):
                    self.data += data
                def close(self):
                    self.closed = True
            class Child:
                stdin, stdout = Input(), object()
                def poll(self):
                    return None
                def wait(self, **_kwargs):
                    return 0
            with tempfile.TemporaryDirectory() as root:
                value = object.__new__(measurement.Measurement)
                value.base, value.deadline, value.ran = Path(root), 180, False
                value.interface = 'eth0'
                value.config = {'schema_version': 2, 'workers': 4, 'role': 3, 'target': {'socket': str(Path(root) / 'api.sock'),
                                                                   'formation': 'f', 'node': 'n'}}
                value.progress = {'stage': 'prepare'}
                value.worker, value.probe, value.supervisor = Process(), Process(), Process()
                value.child = Child()
                lines = iter((b'END', json.dumps(report()).encode()))
                value.read_line = lambda *_: next(lines)
                def kernel(run_id, interface, *, include_firmware):
                    from test_pi_lab_environment import receipt
                    self.assertIs(include_firmware, False)
                    sample = receipt(run_id, interface)
                    clock.ns += 10_000
                    sample.update(monotonic_after_ns=clock.ns, firmware_throttled=None,
                                  unavailable=['firmware_throttled'])
                    return sample
                with patch.object(measurement.time, 'monotonic_ns', clock.mono), \
                     patch.object(measurement.time, 'time_ns', clock.wall), \
                     patch.object(measurement.time, 'monotonic', clock.seconds), \
                     patch.object(measurement.time, 'sleep', clock.sleep), \
                     patch.object(environment, 'snapshot', kernel):
                    result = value.run(clock.wall() + 3_000_000_000)
                self.assertEqual(value.child.stdin.data, b'GA')
                self.assertFalse(result['acceptance_run'])
                self.assertFalse(result['clock_uncertainty_qualified'])
                self.assertEqual(set(result['resources']), {'worker', 'probe', 'supervisor'})
                self.assertGreater(result['resources']['worker']['cpu_bracket_ns'], 0)
                self.assertEqual(result['window_wall_change_ns'], 0)
                self.assertTrue((Path(root) / 'measurement.json').exists())
                from pi_lab_results import review
                reviewed = review(result, value.config)
                self.assertEqual(reviewed['status'], 'valid_unqualified')
                self.assertEqual(reviewed['measurement_issues'], result['issues'])
                self.assertEqual(len(result['environment_series']['samples']), 10)
                self.assertFalse(reviewed['environment']['firmware_sampled'])
                if cost_ns > measurement.PERIOD_NS // 3:
                    self.assertIn('resource_sampling_missed', result['issues'])
                else:
                    self.assertEqual(result['issues'], [])

    def test_incomplete_window_keeps_partial_environment_receipts(self):
        with tempfile.TemporaryDirectory() as root:
            value = object.__new__(measurement.Measurement)
            value.base, value.ran = Path(root), True
            value.progress = {'stage': 'sampling'}
            value.environment_series = environment.KernelSeries('p-run', 'eth0', 1)
            with patch.object(environment.time, 'monotonic_ns', side_effect=(1, 10)), \
                 patch.object(environment, 'snapshot', side_effect=OSError('secret')):
                value.environment_series.sample_if_due()
            value.child = SimpleNamespace(stdin=io.BytesIO(), stdout=io.BytesIO(), poll=lambda: 1, returncode=1)
            value.stderr = SimpleNamespace(finish=lambda: {'bytes': 0})
            closed = value.close()
            saved = json.loads((Path(root) / 'measurement-incomplete.json').read_text())
            self.assertEqual(saved['environment_series']['missed_samples'], 9)
            self.assertEqual(saved['environment_series']['samples'][0]['error_type'], 'OSError')
            self.assertNotIn('secret', str(saved))
            self.assertFalse(closed['forced'])


if __name__ == '__main__':
    unittest.main()
