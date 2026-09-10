#!/usr/bin/env python3
"""Diagnostic observer contracts, not performance assertions."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "baseline", Path(__file__).with_name("diagnose-formation-baseline.py"))
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class Contracts(unittest.TestCase):
    def test_stat_handles_spaces_and_closing_parentheses(self):
        fields = ["S"] + [str(index) for index in range(1, 50)]
        row = baseline.thread_stat("123 (a thread) name) " + " ".join(fields))
        self.assertEqual(row["last_cpu"], 36)
        self.assertEqual(row["user_ticks"], 11)
        self.assertEqual(row["start_ticks"], 19)
        self.assertEqual(row["minor_faults"], 7)
        with self.assertRaises(ValueError):
            baseline.thread_stat("123 (a) S 1")

    def test_observer_reads_real_process_and_bounds_threads(self):
        row = baseline.process_snapshot(os.getpid())
        self.assertIn(str(os.getpid()), row["threads"])
        self.assertGreater(row["memory"]["rss_kib"], 0)
        with patch.dict(baseline.PROFILE, threads_per_process_cap=0):
            with self.assertRaises(ValueError):
                baseline.process_snapshot(os.getpid())

    def test_unavailable_is_not_zero_and_oversized_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "sensor"
            self.assertIsNone(baseline.bounded_text(path))
            path.write_text("12345")
            with self.assertRaises(ValueError):
                baseline.bounded_text(path, cap=4)

    def test_failed_real_measure_boundary_stops_observer(self):
        cell = baseline.ObservedCell(None, 3, "compiled_off", {}, {})
        with tempfile.TemporaryDirectory() as temporary:
            cell.observation_dir = Path(temporary)
            with patch.object(baseline, "sensors", return_value=[]), patch.object(
                    baseline.curve.Cell, "measure", side_effect=ValueError("fixture failure")):
                with self.assertRaises(ValueError):
                    cell.measure()
            self.assertTrue(cell.row["observation_thread_stopped"])


if __name__ == "__main__":
    unittest.main()
