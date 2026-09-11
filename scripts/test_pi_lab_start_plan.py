#!/usr/bin/env python3
"""Offset-aware scheduling proposals with no IO or hardware execution."""
import copy
import unittest

import pi_lab_clock as clock

RUN = 'clock-plan-test'


def stamp(mono, wall):
    return {'monotonic_before_ns': mono, 'unix_ns': wall, 'monotonic_after_ns': mono + 2}


def fixture(count=4, offsets=None, durations=None):
    now = stamp(1_000_000_000, 10_000_000_000)
    rows = []
    for role in range(count):
        exchanges = []
        for index in range(3):
            begin = 999_000_000 + index * 10000
            duration = durations[index] if durations else 2000
            offset = offsets[index] if offsets else (role - 1) * 50_000_000
            nonce = f'{role * 3 + index:032x}'
            node = stamp(begin + 100_000 * role + duration // 2, begin + 9_000_000_000 + duration // 2 + offset)
            remote = {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': RUN, 'nonce': nonce, 'stamp': node}
            exchanges.append(clock.evidence(stamp(begin, begin + 9_000_000_000),
                                             stamp(begin + duration, begin + duration + 9_000_000_000),
                                             remote, RUN, nonce))
        rows.append({'role': role, 'exchanges': exchanges})
    policy = {'lead_ns': 3_000_000_000, 'max_age_ns': 2_000_000, 'max_round_trip_ns': 3000,
              'max_offset_width_ns': 3000, 'max_wall_change_ns': 100, 'max_relative_drift_ppm': 0,
              'max_start_uncertainty_ns': 1000, 'max_local_start_lateness_ns': 10,
              'max_pairwise_start_skew_ns': 2010}
    return rows, now, policy


class StartPlanContracts(unittest.TestCase):
    def test_three_to_five_roles_correct_positive_and_negative_offset_direction(self):
        for count in (3, 4, 5):
            rows, now, policy = fixture(count)
            value = clock.plan_start(rows, RUN, now, policy)
            self.assertEqual(value['reference_start_unix_ns'], 13_000_000_000)
            for role, node in enumerate(value['roles']):
                self.assertEqual(node['start_unix_ns'], 13_000_000_000 + (role - 1) * 50_000_000)
                self.assertEqual(node['conditional_start_error_ns'], [-1000, 1010])
            self.assertEqual(value['conditional_pairwise_skew_ns'], 2010)
            self.assertIs(value['dispatch_authorized'], False)
            self.assertIs(value['clock_uncertainty_qualified'], False)
            self.assertIs(value['acceptance_run'], False)

    def test_conservative_union_retains_slow_sample_not_fastest_intersection(self):
        rows, now, policy = fixture(durations=[1000, 2000, 2500])
        policy.update(max_start_uncertainty_ns=1250, max_pairwise_start_skew_ns=2510)
        value = clock.plan_start(rows, RUN, now, policy)
        self.assertTrue(all(row['start_uncertainty_ns'] == 1250 for row in value['roles']))
        self.assertEqual(value['conditional_pairwise_skew_ns'], 2510)

    def test_drift_margins_round_outward_using_entire_horizon(self):
        rows, now, policy = fixture()
        policy.update(max_relative_drift_ppm=1, max_start_uncertainty_ns=5000, max_pairwise_start_skew_ns=10010)
        value = clock.plan_start(rows, RUN, now, policy)
        for role, row in enumerate(value['roles']):
            for index, margin in enumerate(row['drift_margins_ns']):
                horizon = value['reference_start_unix_ns'] - rows[role]['exchanges'][index]['coordinator_before']['unix_ns']
                self.assertEqual(margin, (horizon + 999999) // 1_000_000)
            self.assertGreater(row['start_uncertainty_ns'], 1000)

    def test_stale_future_and_changed_coordinator_clock_are_rejected(self):
        rows, now, policy = fixture()
        with self.assertRaises(ValueError):
            clock.plan_start(rows, RUN, now, policy | {'max_age_ns': 1000})
        with self.assertRaises(ValueError):
            clock.plan_start(rows, RUN, stamp(998_000_000, 9_998_000_000), policy)
        with self.assertRaises(ValueError):
            clock.plan_start(rows, RUN, now | {'unix_ns': now['unix_ns'] + 1000}, policy)

    def test_wrong_run_duplicate_nonce_order_and_missing_roles_are_rejected(self):
        rows, now, policy = fixture()
        for wrong_rows, run_id in ((rows, 'other'), (list(reversed(rows)), RUN), (rows[:2], RUN)):
            with self.assertRaises(ValueError):
                clock.plan_start(wrong_rows, run_id, now, policy)
        for change in (lambda value: value[1]['exchanges'].__setitem__(0, copy.deepcopy(value[0]['exchanges'][0])),
                       lambda value: value[0]['exchanges'].reverse(),
                       lambda value: value[0]['exchanges'].pop(),
                       lambda value: value[0].update(role=True)):
            changed = copy.deepcopy(rows)
            change(changed)
            with self.assertRaises(ValueError):
                clock.plan_start(changed, RUN, now, policy)

    def test_inconsistent_intervals_do_not_get_hidden_by_wide_union(self):
        rows, now, policy = fixture(offsets=[0, 10000, 0])
        policy.update(max_start_uncertainty_ns=100000, max_pairwise_start_skew_ns=200010)
        with self.assertRaisesRegex(ValueError, 'inconsistent'):
            clock.plan_start(rows, RUN, now, policy)

    def test_rejects_oversized_exchange_uncertainty_and_skew(self):
        rows, now, policy = fixture()
        for key, limit in (('max_round_trip_ns', 1000), ('max_offset_width_ns', 1000),
                           ('max_start_uncertainty_ns', 999), ('max_pairwise_start_skew_ns', 2009)):
            with self.assertRaises(ValueError):
                clock.plan_start(rows, RUN, now, policy | {key: limit})
        # A local wakeup allowance contributes to predicted cross-node skew.
        with self.assertRaises(ValueError):
            clock.plan_start(rows, RUN, now, policy | {'max_local_start_lateness_ns': 11})

    def test_policy_is_explicit_typed_bounded_and_does_not_mutate_inputs(self):
        rows, now, policy = fixture()
        original = copy.deepcopy((rows, now, policy))
        for wrong in ({}, policy | {'lead_ns': True}, policy | {'lead_ns': 16_000_000_000},
                      policy | {'max_age_ns': 61_000_000_000}, policy | {'max_relative_drift_ppm': 1_000_001},
                      policy | {'max_wall_change_ns': float('nan')}, policy | {'secret': 'no'}):
            with self.assertRaises(ValueError):
                clock.plan_start(rows, RUN, now, wrong)
        clock.plan_start(rows, RUN, now, policy)
        self.assertEqual((rows, now, policy), original)


if __name__ == '__main__':
    unittest.main()
