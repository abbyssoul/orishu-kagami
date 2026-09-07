"""Negative controls for the release exclusion gate; no compiler subprocesses."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_guard", Path(__file__).with_name("check-formation-release.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class ReleaseGuardTests(unittest.TestCase):
    def run_gate(self, normal_code, fault_code, diagnostic):
        normal = subprocess.CompletedProcess([], normal_code, b"", b"")
        fault = subprocess.CompletedProcess([], fault_code, json.dumps({
            "reason": "compiler-message", "target": {"name": "orishu_worker"},
            "message": {"level": "error", "message": diagnostic},
        }).encode() + b"\n", b"")
        with patch("sys.argv", ["release-guard"]), \
                patch.object(guard.subprocess, "run", side_effect=[normal, fault]), \
                contextlib.redirect_stdout(io.StringIO()):
            guard.main()

    def test_exact_guard_is_required(self):
        self.run_gate(0, 101, "formation-fault-test is forbidden in release builds")

    def test_unrelated_failure_is_not_exclusion(self):
        with self.assertRaises(AssertionError):
            self.run_gate(0, 101, "unrelated type error")

    def test_successful_fault_build_is_not_exclusion(self):
        with self.assertRaises(AssertionError):
            self.run_gate(0, 0, "formation-fault-test is forbidden in release builds")

    def test_broken_normal_build_is_not_exclusion(self):
        with self.assertRaises(AssertionError):
            self.run_gate(101, 101, "formation-fault-test is forbidden in release builds")


if __name__ == "__main__":
    unittest.main()
