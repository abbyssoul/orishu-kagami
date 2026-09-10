"""Observer diagnostic contracts; never acceptance/performance assertions."""
import importlib.util
import json
import os
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location(
    "convergence", Path(__file__).with_name("diagnose-formation-convergence.py"))
diagnostic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostic)


class Contracts(unittest.TestCase):
    def test_persistent_typed_summary_preserves_cli_predicate_fields(self):
        row = {"formation_stage": "unlock", "observer": "persistent"}
        fixture = diagnostic.ObservedCell(None, 30, "omitted", {}, row)
        fixture.observer = Mock()
        typed = {"schemaVersion": 1, "formationId": "formation", "sourceNodeId": "node",
                 "memberCount": 30, "aliveCount": 30, "membershipLocked": False,
                 "participation": "joined", "introducerReady": True}
        with patch.object(diagnostic, "observer_reply", return_value=json.dumps(typed).encode()):
            value = fixture.cli(29, ["cluster", "info"])
        self.assertEqual((value["nodes"], value["alive"], value["locked"]), (30, 30, False))
        self.assertEqual((value["formationId"], value["sourceNodeId"]), ("formation", "node"))
        self.assertFalse(row["read_observations"][0]["locked"])

    def reply(self, data, cap=64):
        receive, send = os.pipe()
        with os.fdopen(receive, "rb", buffering=0) as stream:
            with os.fdopen(send, "wb", buffering=0) as output:
                output.write(data)
            return diagnostic.observer_reply(stream, cap=cap)

    def test_observer_reply_is_bounded_and_single_frame(self):
        self.assertEqual(self.reply(b'{"members":30}\n'), b'{"members":30}')
        self.assertEqual(self.reply(b'1234\n', cap=4), b'1234')
        for data in (b'', b'1234', b'12345\n', b'1\n2\n', b'1\n2'):
            with self.assertRaises(ValueError):
                self.reply(data, cap=4)

    def test_observer_timeout_is_not_empty_success(self):
        receive, send = os.pipe()
        with os.fdopen(receive, "rb", buffering=0) as stream, os.fdopen(send, "wb", buffering=0):
            with self.assertRaisesRegex(ValueError, "observer output deadline"):
                diagnostic.observer_reply(stream, timeout=0.001)

    def test_pacing_cannot_accept_policy_after_original_deadline(self):
        row = {"formation_stage": "unlock"}
        fixture = diagnostic.ObservedCell(None, 30, "omitted", {}, row)
        fixture.started = 0
        clock = [59.5]
        calls = []

        def check():
            calls.append(clock[0])
            return len(calls) > 1

        with patch.object(diagnostic.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(diagnostic.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
            with self.assertRaises(TimeoutError):
                fixture.poll(check, convergence=True)
        self.assertEqual(calls, [59.5, 60.5])
        self.assertFalse(row["formation_observations"][0]["accepted"])
        self.assertEqual(row["formation_observations"][0]["deadline_seconds"], 60)


if __name__ == "__main__":
    unittest.main()
