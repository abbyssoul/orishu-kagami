#!/usr/bin/env python3
"""Actual-window overlap bounds using synthetic clocks, never hardware load."""
import copy
import unittest

import pi_lab_clock as clock
import pi_lab_results as results
from test_pi_lab_results import config, measurement

RUN = 'p-run'


def stamp(mono, wall):
    return {'monotonic_before_ns': mono, 'monotonic_after_ns': mono + 100, 'unix_ns': wall}


def exchange(role, index, offset=0, duration=2_000_000):
    wall = (9 if index == 0 else 22) * 1_000_000_000
    nonce = f'{role * 2 + index:032x}'
    remote = {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': RUN, 'nonce': nonce,
              'stamp': stamp(wall + (role + 1) * 100_000_000_000 + duration // 2, wall + offset + duration // 2)}
    return clock.evidence(stamp(wall, wall), stamp(wall + duration, wall + duration), remote, RUN, nonce)


def shift(value, offset, *, scheduled=True):
    for key in ('start_unix_ns', 'end_unix_ns'):
        value[key] += offset
    if scheduled:
        value['scheduled_start_unix_ns'] += offset
    else:
        value['local_start_lateness_ns'] += offset
    for key in ('start', 'end_marker'):
        value['load']['timing'][key]['unix_ns'] += offset


def fixture(count=4):
    rows, expected = [], []
    for role in range(count):
        offset = (role - 1) * 50_000_000
        wanted, value = config(count, role), measurement(count, role)
        # Supervisor observes END after the probe's clock-read bracket, not
        # at exactly the same nanosecond as the probe's earlier wall sample.
        value['end_unix_ns'] += 10_000
        value['load_control_bracket_ns'] += 10_000
        wanted['target']['node'] = f'n{role}'
        value['target'] = dict(wanted['target'])
        shift(value, offset)
        expected.append(wanted)
        rows.append({'role': role, 'measurement': value,
                     'clock_before': exchange(role, 0, offset), 'clock_after': exchange(role, 1, offset)})
    policy = {'max_round_trip_ns': 3_000_000, 'max_offset_width_ns': 3_000_000,
              'max_wall_change_ns': 1000, 'max_offset_change_ns': 1000,
              'max_collection_span_ns': 15_000_000_000, 'max_pairwise_start_skew_ns': 3_000_000,
              'min_common_window_ns': 9_990_000_000, 'max_first_request_offset_ns': 2_000_000,
              'min_last_completion_offset_ns': 9_800_000_000}
    return rows, expected, policy


class OverlapContracts(unittest.TestCase):
    def check(self, rows, expected, policy):
        return results.review_overlap(rows, expected, RUN, policy)

    def test_three_to_five_mixed_clock_epochs_and_signed_offsets(self):
        for count in (3, 4, 5):
            rows, expected, policy = fixture(count)
            original = copy.deepcopy((rows, expected, policy))
            result = self.check(rows, expected, policy)
            self.assertEqual(result['status'], 'within_conditional_bounds')
            self.assertEqual(result['conditional_pairwise_start_skew_upper_ns'], 2_003_100)
            self.assertEqual(result['conditional_common_window_lower_ns'], 9_997_995_900)
            self.assertEqual(len({tuple(row['probe_start_reference_interval_ns']) for row in result['nodes']}), 1)
            self.assertIs(result['acceptance_run'], False)
            self.assertIs(result['clock_uncertainty_qualified'], False)
            self.assertEqual((rows, expected, policy), original)

    def test_end_marker_delay_is_not_extra_load_window(self):
        rows, expected, policy = fixture()
        baseline = self.check(rows, expected, policy)
        value = rows[0]['measurement']
        value['end_unix_ns'] += 1_000_000_000
        value['load_control_bracket_ns'] += 1_000_000_000
        value['load']['timing']['end_marker']['unix_ns'] += 1_000_000_000
        value['load']['timing']['end_marker_elapsed_ns'] += 1_000_000_000
        result = self.check(rows, expected, policy)
        self.assertEqual(result['status'], 'within_conditional_bounds')
        self.assertEqual(result['conditional_common_window_lower_ns'], baseline['conditional_common_window_lower_ns'])
        self.assertEqual(result['nodes'][0]['end_marker_lateness_ns'], 1_000_000_000)

    def test_actual_start_not_scheduled_start_controls_skew_and_overlap(self):
        rows, expected, policy = fixture()
        shift(rows[3]['measurement'], 50_000_000, scheduled=False)
        result = self.check(rows, expected, policy)
        self.assertEqual(result['status'], 'outside_conditional_bounds')
        self.assertIn('pairwise_probe_start_skew_limit', result['cohort_issues'])
        self.assertIn('common_probe_window_too_short', result['cohort_issues'])
        self.assertEqual(result['conditional_pairwise_start_skew_upper_ns'], 52_003_100)
        self.assertEqual(result['conditional_common_window_lower_ns'], 9_947_995_900)

    def test_slow_exchange_and_larger_assumed_change_only_widen_bounds(self):
        rows, expected, policy = fixture()
        baseline = self.check(rows, expected, policy)
        rows[0]['clock_after'] = exchange(0, 1, -50_000_000, duration=4_000_000)
        result = self.check(rows, expected, policy)
        self.assertIn('clock_round_trip_limit', result['node_issues'][0]['issues'])
        self.assertGreater(result['conditional_pairwise_start_skew_upper_ns'], baseline['conditional_pairwise_start_skew_upper_ns'])
        self.assertLess(result['conditional_common_window_lower_ns'], baseline['conditional_common_window_lower_ns'])
        rows, expected, policy = fixture()
        result = self.check(rows, expected, policy | {'max_offset_change_ns': 10_000})
        self.assertLess(result['conditional_common_window_lower_ns'], baseline['conditional_common_window_lower_ns'])

    def test_offset_change_node_mapping_probe_mapping_and_brackets_fail(self):
        rows, expected, policy = fixture()
        rows[0]['clock_after'] = exchange(0, 1, -40_000_000)
        result = self.check(rows, expected, policy)
        self.assertIn('node_wall_clock_change', result['node_issues'][0]['issues'])
        self.assertIn('offset_change_limit', result['node_issues'][0]['issues'])
        for edit, issue in (
            (lambda value: value['load']['timing']['start'].update(unix_ns=1), 'probe_outside_clock_bracket'),
            (lambda value: value['load']['timing']['end_marker'].update(unix_ns=20_000_000_000), 'probe_wall_clock_change'),
            (lambda value: value['load']['timing']['start'].update(unix_ns=9_900_000_000), 'probe_outside_supervisor_bracket')):
            rows, expected, policy = fixture()
            edit(rows[0]['measurement'])
            self.assertIn(issue, self.check(rows, expected, policy)['node_issues'][0]['issues'])

    def test_no_activity_late_client_and_early_finish_are_not_alignment_success(self):
        for change, issue in (
            (lambda client: client.update(first_request_offset_ns=3_000_000), 'client_first_request_late'),
            (lambda client: client.update(last_timed_completion_offset_ns=9_000_000_000), 'client_last_completion_early')):
            rows, expected, policy = fixture()
            change(rows[2]['measurement']['load']['workers'][0]['activity'][1])
            result = self.check(rows, expected, policy)
            self.assertIn(issue, result['node_issues'][0]['issues'])
        rows, expected, policy = fixture()
        value = measurement(4, 0, completed=0)
        value['end_unix_ns'] += 10_000
        value['load_control_bracket_ns'] += 10_000
        value['target'] = dict(expected[0]['target'])
        shift(value, -50_000_000)
        rows[0]['measurement'] = value
        result = self.check(rows, expected, policy)
        self.assertIn('client_has_no_timed_activity', result['node_issues'][0]['issues'])
        self.assertIn('offered_rate_not_sustained', result['nodes'][0]['measurement_issues'])

    def test_whole_coordinator_mapping_and_collection_span_checked(self):
        rows, expected, policy = fixture()
        result = self.check(rows, expected, policy | {'max_collection_span_ns': 12_000_000_000})
        self.assertIn('collection_span_limit', result['cohort_issues'])
        # Preserve each exchange's local mapping but change it between exchanges.
        raw = rows[0]['clock_after']
        for key in ('coordinator_before', 'coordinator_after'):
            raw[key]['unix_ns'] += 100_000
        remote = {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': RUN, 'nonce': raw['nonce'], 'stamp': raw['node']}
        rows[0]['clock_after'] = clock.evidence(raw['coordinator_before'], raw['coordinator_after'], remote, RUN, raw['nonce'])
        self.assertIn('coordinator_wall_clock_change', self.check(rows, expected, policy)['cohort_issues'])

    def test_missing_roles_wrong_targets_replay_and_forged_intervals_rejected(self):
        changes = [lambda rows, expected: rows.pop(),
                   lambda rows, expected: rows.reverse(),
                   lambda rows, expected: rows[0].update(role=True),
                   lambda rows, expected: rows[0]['measurement']['target'].update(node='wrong'),
                   lambda rows, expected: expected[0]['target'].update(formation='other'),
                   lambda rows, expected: rows[0].update(clock_after=copy.deepcopy(rows[0]['clock_before'])),
                   lambda rows, expected: rows[0]['clock_before'].update(offset_bracket_ns=[0, 0])]
        for change in changes:
            rows, expected, policy = fixture()
            change(rows, expected)
            with self.assertRaises(ValueError):
                self.check(rows, expected, policy)
        rows, expected, policy = fixture()
        with self.assertRaises(ValueError):
            results.review_overlap(rows, expected, 'other-run', policy)

    def test_explicit_policy_types_bounds_and_unknown_fields(self):
        rows, expected, policy = fixture()
        for wrong in ({}, policy | {'max_offset_change_ns': True}, policy | {'max_wall_change_ns': -1},
                      policy | {'max_collection_span_ns': 45_000_000_001}, policy | {'min_common_window_ns': 0},
                      policy | {'min_common_window_ns': 10_000_000_001}, policy | {'secret': 'no'}):
            with self.assertRaises(ValueError):
                self.check(rows, expected, wrong)

    def test_bounds_enclose_known_windows_and_thresholds_are_inclusive(self):
        for delay in (0, 100, 1000, 100_000, 50_000_000):
            rows, expected, policy = fixture()
            shift(rows[-1]['measurement'], delay, scheduled=False)
            result = self.check(rows, expected, policy)
            self.assertGreaterEqual(result['conditional_pairwise_start_skew_upper_ns'], delay)
            self.assertLessEqual(result['conditional_common_window_lower_ns'], 10_000_000_000 - delay)
            for role, node in enumerate(result['nodes']):
                true_start = 10_001_000_000 + (delay if role == len(rows) - 1 else 0)
                for endpoint, instant in (('probe_start_reference_interval_ns', true_start),
                                          ('probe_end_reference_interval_ns', true_start + 10_000_000_000)):
                    self.assertLessEqual(node[endpoint][0], instant)
                    self.assertGreaterEqual(node[endpoint][1], instant)
        rows, expected, policy = fixture()
        baseline = self.check(rows, expected, policy)
        policy.update(max_pairwise_start_skew_ns=baseline['conditional_pairwise_start_skew_upper_ns'],
                      min_common_window_ns=baseline['conditional_common_window_lower_ns'])
        self.assertEqual(self.check(rows, expected, policy)['status'], 'within_conditional_bounds')
        for key, delta in (('max_pairwise_start_skew_ns', -1), ('min_common_window_ns', 1)):
            self.assertEqual(self.check(rows, expected, policy | {key: policy[key] + delta})['status'], 'outside_conditional_bounds')


if __name__ == '__main__':
    unittest.main()
