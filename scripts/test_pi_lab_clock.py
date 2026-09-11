#!/usr/bin/env python3
"""Clock evidence contracts; no Pi connection or timed load."""
import copy
import json
import os
import subprocess
import sys
import unittest

import pi_lab_clock as clock
import pi_lab_experiment as experiment
import pi_lab_session as session

NONCE = 'a' * 32
RUN = 'p-test'


def stamp(mono, wall, span=2):
    return {'monotonic_before_ns': mono, 'unix_ns': wall, 'monotonic_after_ns': mono + span}


def receipt(value):
    return {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': RUN,
            'nonce': NONCE, 'stamp': value}


class ClockContracts(unittest.TestCase):
    def sample(self):
        return clock.evidence(stamp(1000, 10000), stamp(1100, 10100),
                              receipt(stamp(5000, 12050)), RUN, NONCE)

    def test_causal_offset_and_monotonic_read_brackets(self):
        value = self.sample()
        self.assertEqual(value['offset_bracket_ns'], [1950, 2050])
        self.assertEqual(value['round_trip_bracket_ns'], [98, 102])
        self.assertEqual(value['coordinator_wall_change_bracket_ns'], [-2, 2])
        self.assertIs(value['clock_uncertainty_qualified'], False)

    def test_asymmetric_delivery_retains_full_envelope_not_midpoint(self):
        for remote_wall in (12001, 12099):
            value = clock.evidence(stamp(1000, 10000), stamp(1100, 10100),
                                   receipt(stamp(5000, remote_wall)), RUN, NONCE)
            lower, upper = value['offset_bracket_ns']
            self.assertLessEqual(lower, 2000)
            self.assertGreaterEqual(upper, 2000)
            self.assertEqual(upper - lower, 100)

    def test_explicit_limits_and_exact_boundaries(self):
        value = self.sample()
        policy = dict(max_round_trip_ns=102, max_offset_width_ns=100, max_wall_change_ns=2)
        self.assertEqual(clock.assess(value, **policy), [])
        for field, issue in [('max_round_trip_ns', 'clock_round_trip_limit'),
                             ('max_offset_width_ns', 'clock_offset_envelope_limit'),
                             ('max_wall_change_ns', 'coordinator_wall_clock_change')]:
            self.assertIn(issue, clock.assess(value, **(policy | {field: policy[field] - 1})))
        for bad in (True, -1, 1.0, 2**63):
            with self.assertRaises(ValueError):
                clock.assess(value, **(policy | {'max_round_trip_ns': bad}))

    def test_wall_step_is_retained_and_rejected_not_clamped(self):
        for wall in (9900, 20100):
            value = clock.evidence(stamp(1000, 10000), stamp(1100, wall),
                                   receipt(stamp(5000, 12050)), RUN, NONCE)
            self.assertIn('coordinator_wall_clock_change', clock.assess(
                value, max_round_trip_ns=200, max_offset_width_ns=20000, max_wall_change_ns=10))
            if wall == 9900:
                self.assertGreater(*value['offset_bracket_ns'])

    def test_hostile_shape_types_and_mixed_identity(self):
        good = receipt(stamp(5000, 12050))
        for bad in (good | {'schema_version': True}, good | {'run_id': 'other'},
                    good | {'nonce': 'b' * 32}, good | {'token': 'secret'},
                    good | {'stamp': stamp(True, 123)}, good | {'stamp': stamp(1, 2**63)},
                    good | {'stamp': stamp(1, 2, -1)}, good | {'stamp': None}):
            with self.assertRaises(ValueError):
                clock.evidence(stamp(1000, 10000), stamp(1100, 10100), bad, RUN, NONCE)
        with self.assertRaises(ValueError):
            clock.evidence(stamp(1100, 10000), stamp(1000, 10100), good, RUN, NONCE)

    def test_saved_intervals_cannot_be_forged(self):
        for field, forged in [('offset_bracket_ns', [2000, 2000]), ('schema_version', True),
                              ('clock_uncertainty_qualified', True), ('acceptance_run', 0),
                              ('round_trip_bracket_ns', [98.0, 102.0])]:
            value = copy.deepcopy(self.sample())
            value[field] = forged
            with self.assertRaises(ValueError):
                clock.assess(value, max_round_trip_ns=1000, max_offset_width_ns=1000, max_wall_change_ns=1000)

    def test_session_clock_operation_requires_no_worker_or_child(self):
        node = object.__new__(session.Session)
        node.run_id = RUN
        value = node.request({'operation': 'clock', 'arguments': {'nonce': NONCE}})
        self.assertEqual(set(value), {'schema_version', 'kind', 'run_id', 'nonce', 'stamp'})
        clock.validate_stamp(value['stamp'])
        for args in ({}, {'nonce': True}, {'nonce': NONCE, 'path': '/etc/passwd'}):
            with self.assertRaises(ValueError):
                node.request({'operation': 'clock', 'arguments': args})

    def test_exchange_uses_unique_nonce_and_measured_send_receive_stamps(self):
        node = object.__new__(session.Session)
        node.run_id = RUN
        def call(operation, **args):
            return node.request({'operation': operation, 'arguments': args})
        a, b = clock.exchange(call, RUN), clock.exchange(call, RUN)
        self.assertNotEqual(a['nonce'], b['nonce'])
        self.assertLessEqual(a['offset_bracket_ns'][0], 0)
        self.assertGreaterEqual(a['offset_bracket_ns'][1], 0)

    def test_remote_call_over_serialized_pipe_path(self):
        # Real Remote.call JSON/deadline path and node Session.request, no SSH
        # or sockets needed. The subprocess owns no worker and exits on EOF.
        code = ('import pi_lab_session as s\n'
                'node=object.__new__(s.Session); node.run_id="p-test"\n'
                'import sys,json\n'
                'for raw in sys.stdin.buffer:\n'
                ' s.emit({"ok":True,"result":node.request(s.decode(raw))})\n')
        child = subprocess.Popen([sys.executable, '-u', '-c', code],
                                 cwd=os.path.dirname(__file__), stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
        try:
            remote = object.__new__(experiment.Remote)
            remote.child = child
            remote.deadline = clock.time.monotonic() + 5
            value = clock.exchange(remote.call, RUN)
            self.assertEqual(value['run_id'], RUN)
            self.assertEqual(json.loads(json.dumps(value)), value)
        finally:
            child.stdin.close()
            try:
                child.wait(timeout=2)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=2)
                child.stdout.close()
                child.stderr.close()
        self.assertEqual(child.returncode, 0)


if __name__ == '__main__':
    unittest.main()
