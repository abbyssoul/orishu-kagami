#!/usr/bin/env python3
"""Read-only environment evidence with bounded kernel/firmware fixtures."""
from pathlib import Path
import copy
import tempfile
import time
import unittest
from unittest.mock import patch

import pi_lab_environment as environment
import pi_lab_session as session


def receipt(run_id='p-run', interface='eth0'):
    now = time.monotonic_ns()
    return {'schema_version': 1, 'kind': 'pi-environment', 'run_id': run_id, 'interface': interface,
            'ifindex': 2, 'boot_id': '00000000-0000-0000-0000-000000000001',
            'monotonic_before_ns': now, 'monotonic_after_ns': now + 1,
            'network': {key: 3000 for key in environment.COUNTERS},
            'temperature_millicelsius': 45000, 'firmware_throttled': 0, 'unavailable': []}


def series(run_id='p-run', interface='eth0'):
    samples = []
    start = 1_000_000_000
    for slot in range(10):
        offset = slot * environment.SERIES_PERIOD_NS
        value = receipt(run_id, interface)
        value.update(monotonic_before_ns=start + offset + 1, monotonic_after_ns=start + offset + 2,
                     firmware_throttled=None, unavailable=['firmware_throttled'])
        for key in ('rx_bytes', 'tx_bytes'):
            value['network'][key] += slot * 1000
        if slot == 5:
            value['temperature_millicelsius'] = 55000
        samples.append({'slot': slot, 'begin_offset_ns': offset, 'end_offset_ns': offset + 3, 'snapshot': value})
    return {'schema_version': 1, 'kind': 'pi-kernel-environment-series', 'run_id': run_id,
            'interface': interface, 'monotonic_start_ns': start,
            'period_ns': environment.SERIES_PERIOD_NS, 'window_ns': environment.SERIES_WINDOW_NS,
            'samples': samples, 'missed_samples': 0, 'firmware_sampled': False, 'acceptance_run': False}


class EnvironmentContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sys, self.proc = self.root / 'sys', self.root / 'proc'
        files = {self.sys / 'class/net/eth0/ifindex': '2',
                 self.proc / 'sys/kernel/random/boot_id': receipt()['boot_id'],
                 self.sys / 'class/thermal/thermal_zone0/type': 'cpu-thermal',
                 self.sys / 'class/thermal/thermal_zone0/temp': '45277'}
        files.update({self.sys / 'class/net/eth0/statistics' / name: '3000' for name in environment.COUNTERS})
        for path, text in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)

    def snapshot(self, command=None):
        return environment.snapshot('p-run', 'eth0', sys_root=self.sys, proc_root=self.proc,
                                    command=command or (lambda *_args, **_kw: {'exit': 0, 'stdout': 'throttled=0x0\n'}))

    def test_snapshot_reads_bounded_counter_temperature_and_fixed_firmware_command(self):
        calls = []
        def command(args, **kw):
            calls.append((args, kw))
            return {'exit': 0, 'stdout': 'throttled=0x50000\n'}
        value = self.snapshot(command)
        self.assertEqual(value['temperature_millicelsius'], 45277)
        self.assertEqual(value['firmware_throttled'], 0x50000)
        self.assertEqual(value['network']['rx_dropped'], 3000)
        self.assertEqual(calls, [(['vcgencmd', 'get_throttled'], {'timeout': 3, 'cap': 128})])
        environment.validate(value, 'p-run', 'eth0')

    def test_historical_network_errors_are_not_new_window_errors(self):
        before, after = receipt(), receipt()
        self.assertEqual(environment.compare(before, after, 'p-run', 'eth0')['issues'], [])
        after['network']['rx_dropped'] += 1
        value = environment.compare(before, after, 'p-run', 'eth0')
        self.assertEqual(value['network_delta']['rx_dropped'], 1)
        self.assertIn('host_network_errors_or_drops_in_bracket', value['issues'])
        self.assertIs(value['continuous_temperature_peak_measured'], False)

    def test_counter_reset_reboot_or_interface_replacement_never_becomes_zero(self):
        before = receipt()
        for change in (lambda x: x['network'].update(rx_bytes=0),
                       lambda x: x.update(boot_id='00000000-0000-0000-0000-000000000002'),
                       lambda x: x.update(ifindex=3),
                       lambda x: x.update(monotonic_before_ns=0, monotonic_after_ns=1)):
            after = receipt()
            change(after)
            value = environment.compare(before, after, 'p-run', 'eth0')
            self.assertIsNone(value['network_delta'])
            self.assertTrue(value['issues'])

    def test_missing_temperature_and_firmware_are_explicit_unavailable(self):
        (self.sys / 'class/thermal/thermal_zone0/type').write_text('not-the-cpu')
        for command in (lambda *_a, **_k: {'exit': 1, 'stdout': 'secret'},
                        lambda *_a, **_k: {'exit': 0, 'stdout': 'secret'},
                        lambda *_a, **_k: (_ for _ in ()).throw(TimeoutError('secret'))):
            value = self.snapshot(command)
            self.assertEqual(value['unavailable'], ['temperature', 'firmware_throttled'])
            self.assertIsNone(value['firmware_throttled'])
            self.assertIsNone(value['temperature_millicelsius'])
            self.assertNotIn('secret', str(value))

    def test_identity_change_during_snapshot_fails(self):
        def command(*_a, **_k):
            (self.sys / 'class/net/eth0/ifindex').write_text('3')
            return {'exit': 0, 'stdout': 'throttled=0x0'}
        with self.assertRaises(ValueError):
            self.snapshot(command)

    def test_hostile_files_receipts_and_interface_arguments_rejected(self):
        path = self.sys / 'class/net/eth0/statistics/rx_bytes'
        for text in ('-1', str(2**64), '9' * 129, 'not a number'):
            path.write_text(text)
            with self.assertRaises(ValueError):
                self.snapshot()
        for interface in ('../secret', '-eth', True, 'x' * 16):
            with self.assertRaises(ValueError):
                environment.snapshot('p-run', interface)
        for field, bad in (('run_id', 'other'), ('schema_version', True), ('ifindex', True),
                           ('firmware_throttled', 2**32), ('temperature_millicelsius', float('nan')),
                           ('unavailable', ['temperature']), ('secret', 'no')):
            value = receipt()
            value[field] = bad
            with self.assertRaises(ValueError):
                environment.validate(value, 'p-run', 'eth0')

    def test_session_environment_cannot_override_paths_or_interface(self):
        node = object.__new__(session.Session)
        node.run_id, node.interface = 'p-run', 'eth0'
        with patch.object(environment, 'snapshot', side_effect=receipt) as snapshot:
            value = node.request({'operation': 'environment', 'arguments': {}})
            self.assertEqual(value['interface'], 'eth0')
            snapshot.assert_called_once_with('p-run', 'eth0')
            for args in ({'interface': 'eth1'}, {'sys_root': '/etc'}, {'command': 'shell'}):
                with self.assertRaises(ValueError):
                    node.request({'operation': 'environment', 'arguments': args})

    def test_kernel_only_snapshot_never_invokes_firmware_command(self):
        def forbidden(*_args, **_kwargs):
            self.fail('firmware subprocess during periodic sampling')
        value = environment.snapshot('p-run', 'eth0', sys_root=self.sys, proc_root=self.proc,
                                     command=forbidden, include_firmware=False)
        self.assertEqual(value['temperature_millicelsius'], 45277)
        self.assertIsNone(value['firmware_throttled'])
        self.assertEqual(value['unavailable'], ['firmware_throttled'])

    def test_series_samples_once_per_slot_without_catchup_or_postwindow_io(self):
        now = [1_000_000_000]
        sampler = environment.KernelSeries('p-run', 'eth0', now[0])
        original = environment.snapshot
        calls = []
        def kernel(run_id, interface, *, include_firmware):
            calls.append(include_firmware)
            return original(run_id, interface, sys_root=self.sys, proc_root=self.proc,
                            include_firmware=include_firmware)
        with patch.object(environment.time, 'monotonic_ns', lambda: now[0]), \
             patch.object(environment, 'snapshot', kernel):
            for offset in (0, 1, 4_000_000_000, 4_500_000_000, 9_000_000_000, 10_000_000_000):
                now[0] = 1_000_000_000 + offset
                sampler.sample_if_due()
        value = sampler.finish()
        self.assertEqual([row['slot'] for row in value['samples']], [0, 4, 9])
        self.assertEqual(calls, [False] * 3)
        reviewed = environment.review_series(value, 'p-run', 'eth0', 10_000_000_000)
        self.assertEqual(reviewed['missed_samples'], 7)
        self.assertIn('environment_samples_missed', reviewed['issues'])
        self.assertIsNone(reviewed['network_delta'])
        self.assertEqual(reviewed['sampled_temperature_peak_millicelsius'], 45277)

    def test_series_retains_midwindow_peak_and_host_counter_read_brackets(self):
        value = series()
        reviewed = environment.review_series(value, 'p-run', 'eth0', 10_000_000_000)
        self.assertEqual(reviewed['issues'], [])
        self.assertEqual(reviewed['sampled_temperature_peak_millicelsius'], 55000)
        self.assertTrue(reviewed['temperature_samples_complete'])
        self.assertEqual(reviewed['network_delta']['rx_bytes'], 9000)
        self.assertEqual(reviewed['network_delta']['rx_dropped'], 0)
        self.assertEqual(reviewed['network_bracket_ns'], [8_999_999_999, 9_000_000_001])
        self.assertIs(reviewed['continuous_temperature_peak_measured'], False)
        self.assertIs(reviewed['firmware_sampled'], False)
        self.assertEqual(reviewed['sampler_cpu_owner'], 'supervisor')

    def test_failed_read_is_retained_sanitized_and_never_zero_network(self):
        sampler = environment.KernelSeries('p-run', 'eth0', 1)
        with patch.object(environment.time, 'monotonic_ns', side_effect=(1, 10)), \
             patch.object(environment, 'snapshot', side_effect=PermissionError('DO_NOT_EXPORT_SECRET')):
            sampler.sample_if_due()
        value = sampler.finish()
        reviewed = environment.review_series(value, 'p-run', 'eth0', 10_000_000_000)
        self.assertEqual(reviewed['failed_samples'], 1)
        self.assertIn('environment_sample_failed', reviewed['issues'])
        self.assertIsNone(reviewed['network_delta'])
        self.assertIsNone(reviewed['sampled_temperature_peak_millicelsius'])
        self.assertNotIn('DO_NOT_EXPORT_SECRET', str(value))

    def test_midwindow_reset_or_identity_change_invalidates_entire_network_delta(self):
        for change, issue in (
            (lambda row: row['network'].update(rx_bytes=0), 'network_counter_regressed'),
            (lambda row: row.update(ifindex=3), 'network_interface_replaced'),
            (lambda row: row.update(boot_id='00000000-0000-0000-0000-000000000002'), 'boot_identity_changed')):
            value = series()
            change(value['samples'][5]['snapshot'])
            reviewed = environment.review_series(value, 'p-run', 'eth0', 10_000_000_000)
            self.assertIsNone(reviewed['network_delta'])
            self.assertIn(issue, reviewed['issues'])

    def test_unavailable_temperature_and_late_sample_are_explicit(self):
        value = series()
        value['samples'][5]['snapshot'].update(temperature_millicelsius=None,
                                             unavailable=['temperature', 'firmware_throttled'])
        value['samples'][-1]['end_offset_ns'] = 10_100_000_000
        reviewed = environment.review_series(value, 'p-run', 'eth0', 10_200_000_000)
        self.assertIn('temperature_sample_unavailable', reviewed['issues'])
        self.assertIn('environment_sample_crossed_window_end', reviewed['issues'])
        self.assertFalse(reviewed['temperature_samples_complete'])
        self.assertEqual(reviewed['sampled_temperature_peak_millicelsius'], 45000)

    def test_series_rejects_hostile_identity_slots_firmware_and_coverage(self):
        original = series()
        changes = [lambda value: value.update(run_id='other'),
                   lambda value: value.update(interface='other'),
                   lambda value: value.update(schema_version=True),
                   lambda value: value.update(period_ns=0),
                   lambda value: value.update(missed_samples=1),
                   lambda value: value.update(acceptance_run=True),
                   lambda value: value['samples'].append(copy.deepcopy(value['samples'][0])),
                   lambda value: value['samples'][1].update(slot=0),
                   lambda value: value['samples'][0].update(begin_offset_ns=1_000_000_000),
                   lambda value: value['samples'][0].update(end_offset_ns=10_000_000_001),
                   lambda value: value['samples'][0]['snapshot'].update(monotonic_before_ns=1),
                   lambda value: value['samples'][0]['snapshot'].update(firmware_throttled=0, unavailable=[])]
        for change in changes:
            value = copy.deepcopy(original)
            change(value)
            with self.assertRaises(ValueError):
                environment.review_series(value, 'p-run', 'eth0', 10_000_000_000)


if __name__ == '__main__':
    unittest.main()
