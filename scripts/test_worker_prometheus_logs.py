"""Negative controls for optional Prometheus logging-counter ingestion."""
import importlib.util
from pathlib import Path
import sys
import unittest

_spec = importlib.util.spec_from_file_location("worker_prometheus", Path(__file__).with_name("check-worker-prometheus.py"))
check = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check)


def counters(**values):
    result = dict.fromkeys(check.LOG_NAMES, 0.0)
    result.update({f"orishu_worker_log_{name}_total": float(value) for name, value in values.items()})
    return result


class LogIngestion(unittest.TestCase):
    def test_independent_catalogue_sizes(self):
        self.assertEqual(len(check.SAMPLE_KEYS), 151)
        self.assertEqual(len(check.SAMPLE_KEYS | check.LOG_NAMES), 160)
        self.assertEqual(len(check.SAMPLE_KEYS | check.TRACE_NAMES), 163)
        self.assertEqual(len(check.SAMPLE_KEYS | check.TRACE_NAMES | check.LOG_NAMES), 172)

    def test_waits_for_real_delivery_or_terminal_failure(self):
        self.assertFalse(check.logging_ingested(counters()))
        self.assertFalse(check.logging_ingested(counters(accepted=1)))
        self.assertTrue(check.logging_ingested(counters(accepted=1, written=1)))
        self.assertFalse(check.logging_ingested(counters(accepted=1), True))
        self.assertTrue(check.logging_ingested(counters(accepted=1, output_failed=1, closed=3), True))

    def test_missing_invalid_and_unexpected_loss_fail(self):
        with self.assertRaises(KeyError):
            check.logging_ingested({})
        for name, value in (("written", -1), ("accepted", float("nan")),
                            ("accepted", float("inf")), ("accepted", 1.5),
                            ("queue_full", 1), ("contended", 1), ("encoding_failed", 1),
                            ("invalid_source", 1), ("shutdown_dropped", 1),
                            ("closed", 1), ("output_failed", 1)):
            with self.subTest(name=name, value=value), self.assertRaises(AssertionError):
                check.logging_ingested(counters(**({"accepted": 1, "written": 1} | {name: value})))

    def test_closed_pipe_never_acknowledges_or_retries(self):
        for row in (counters(accepted=1, written=1, output_failed=1),
                    counters(accepted=2, output_failed=2)):
            with self.assertRaises(AssertionError):
                check.logging_ingested(row, True)

    def test_retained_tsdb_values_cannot_pass_new_ingestion(self):
        for closed, name in ((False, "written"), (True, "closed")):
            row = counters(accepted=10, **({"output_failed": 1, "closed": 3} if closed else {"written": 3}))
            key = f"orishu_worker_log_{name}_total"
            self.assertFalse(check.logging_ingested(row, closed, (key, 4)))
            row[key] = 4
            self.assertTrue(check.logging_ingested(row, closed, (key, 4)))

    def test_closed_stdout_really_has_no_reader(self):
        command = [sys.executable, "-c",
                   "import os\ntry: os.write(1, b'test')\nexcept BrokenPipeError: pass\nelse: raise AssertionError('pipe accepted output')"]
        with check.process(command, closed_stdout=True) as child:
            self.assertEqual(child.wait(timeout=3), 0)


if __name__ == "__main__":
    unittest.main()
