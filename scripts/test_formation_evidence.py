"""Test failure evidence with hostile output, never production credentials."""

import base64
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from formation_evidence import CLI_LIMIT, LOG_LIMIT, Evidence, Tail, credential_secrets, redact


class EvidenceTests(unittest.TestCase):
    def test_log_reader_start_failure_reaps_spawned_child(self):
        evidence = Evidence("reader-failure")
        children = []
        launch = subprocess.Popen

        def capture(*args, **kwargs):
            process = launch(*args, **kwargs)
            children.append(process)
            return process

        with tempfile.TemporaryDirectory() as directory:
            with patch("formation_evidence.subprocess.Popen", side_effect=capture), \
                    patch("formation_evidence.threading.Thread.start", side_effect=RuntimeError("injected")):
                with self.assertRaises(RuntimeError):
                    evidence.start([sys.executable, "-c", "import time; time.sleep(30)"],
                                   os.environ.copy(), Path(directory))
        self.assertEqual(len(children), 1)
        self.assertIsNotNone(children[0].poll())
        self.assertTrue(children[0].stdout.closed)
        self.assertFalse(evidence.processes)

    def test_restart_reuses_bounded_tail_only_after_prior_process_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            evidence = Evidence("restart")
            process = evidence.start(
                [sys.executable, "-c", "import time; print('before restart', flush=True); time.sleep(30)"],
                os.environ.copy(), state)
            try:
                deadline = time.monotonic() + 3
                while "before restart" not in evidence.processes[0][2].snapshot():
                    self.assertLess(time.monotonic(), deadline)
                    time.sleep(0.01)
                with self.assertRaises(ValueError):
                    evidence.start([sys.executable, "-c", "pass"], os.environ.copy(), state, replace=0)
                process.kill()
                process.wait(timeout=3)
                with self.assertRaises(ValueError):
                    evidence.start([sys.executable, "-c", "pass"], os.environ.copy(), state / "different", replace=0)
                process = evidence.start([sys.executable, "-c", "print('after restart')"], os.environ.copy(), state, replace=0)
                self.assertEqual(process.wait(timeout=3), 0)
                evidence.processes[0][3].join(timeout=2)
                self.assertEqual(len(evidence.processes), 1)
                output = evidence.processes[0][2].snapshot()
                self.assertIn("before restart", output)
                self.assertIn("after restart", output)
                self.assertIn("explicit process restart", output)
                self.assertLessEqual(len(evidence.processes[0][2].data), LOG_LIMIT)
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=3)
                evidence.close()

    def test_tail_bounds_and_omits_partial_secret_line(self):
        tail = Tail()
        for _ in range(100):
            tail.append(b"secret-fragment" * 1000)
        self.assertLessEqual(len(tail.data), LOG_LIMIT)
        self.assertEqual(tail.snapshot(), "[overlong log line omitted]\n")
        tail.append(b"\ncompleted public line\n")
        self.assertEqual(tail.snapshot(), "completed public line\n")
        tail.clear()
        self.assertFalse(tail.data)

    def test_encoded_credentials_and_multiline_material_are_scrubbed(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            token = "abc12345" * 8
            raw = bytes(range(64))
            (state / "operator.token").write_text(token)
            identity = {"version": 1, "certificate": list(raw), "private_key": list(raw)}
            (state / "identity.json").write_text(json.dumps(identity))
            secrets = credential_secrets(state)
            forms = [token, raw.hex(), base64.b64encode(raw).decode(),
                     json.dumps(list(raw)), json.dumps(list(raw), separators=(",", ":"))]
            output = redact("\n".join(forms) + "\nprivate_key: do-not-keep\n42,\npublic diagnostic\n", secrets)
            for form in forms:
                self.assertNotIn(form, output)
            self.assertNotIn("do-not-keep", output)
            self.assertNotIn("42,", output)
            self.assertIn("public diagnostic", output)

    def test_real_subprocess_failure_bundle_is_bounded_private_and_redacted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / "bundle"
            bundle.mkdir()
            state = root / "state"
            state.mkdir()
            token = "b" * 64
            (state / "operator.token").write_text(token)
            evidence = Evidence("seeded-failure")
            process = evidence.start(
                [sys.executable, "-c", "import sys; print('x'*200000); print('b'*64); print('public end')"],
                os.environ.copy(), state)
            try:
                self.assertEqual(process.wait(timeout=5), 0)
                evidence.processes[0][3].join(timeout=2)
                for _ in range(50):
                    evidence.observe(0, "inspect", subprocess.CompletedProcess([], 1, b"x" * (CLI_LIMIT + 1), token.encode()))
                evidence.observe(0, "token", subprocess.CompletedProcess([], 0, b"must-not-retain", b""))
                self.assertEqual(len(evidence.recent), 16)
                with patch("formation_evidence.tempfile.mkdtemp", return_value=str(bundle)):
                    self.assertEqual(evidence.save_failure(AssertionError(token)), bundle)
                self.assertEqual(bundle.stat().st_mode & 0o777, 0o700)
                self.assertEqual({p.name for p in bundle.iterdir()}, {"worker-0.log", "report.json"})
                for path in bundle.iterdir():
                    self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                    self.assertNotIn(token, path.read_text())
                    self.assertNotIn("must-not-retain", path.read_text())
                self.assertLessEqual((bundle / "worker-0.log").stat().st_size, LOG_LIMIT)
                self.assertIn("public end", (bundle / "worker-0.log").read_text())
                report = json.loads((bundle / "report.json").read_text())
                self.assertEqual(report["errorType"], "AssertionError")
                self.assertEqual(len(report["recentCli"]), 16)
                self.assertTrue(report["credentialRedactionAvailable"])
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=3)
                evidence.close()

    def test_invalid_credentials_withhold_all_raw_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / "bundle"
            bundle.mkdir()
            (root / "identity.json").write_text("invalid identity")
            evidence = Evidence("invalid-identity")
            process = evidence.start([sys.executable, "-c", "print('unclassified-secret')"], os.environ.copy(), root)
            try:
                process.wait(timeout=5)
                evidence.processes[0][3].join(timeout=2)
                evidence.observe(0, "inspect", subprocess.CompletedProcess([], 1, b"unclassified-secret", b""))
                with patch("formation_evidence.tempfile.mkdtemp", return_value=str(bundle)):
                    evidence.save_failure(ValueError("unclassified-secret"))
                for path in bundle.iterdir():
                    self.assertNotIn("unclassified-secret", path.read_text())
                report = json.loads((bundle / "report.json").read_text())
                self.assertFalse(report["credentialRedactionAvailable"])
                self.assertEqual(report["recentCli"], [])
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=3)
                evidence.close()


if __name__ == "__main__":
    unittest.main()
