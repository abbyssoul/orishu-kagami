"""Finite hostile/partial-data and paired-calculation checks; no benchmark runs."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("overhead_report", ROOT / "scripts/summarize-worker-overhead.py")
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)
BASELINE = ROOT / "docs/measurements/worker-telemetry-2026-09-07.jsonl"


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.rows = [json.loads(line) for line in BASELINE.read_text().splitlines()]

    def read(self, rows=None, raw=None):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "report.jsonl"
            path.write_text(raw if raw is not None else "\n".join(json.dumps(row) for row in rows))
            return report.read_report(path)

    def test_retained_baseline_and_pairing(self):
        result = report.summarize(report.read_report(BASELINE), 100)
        self.assertIn("not assessed", result["acceptance"])
        default = result["comparisons"]["sample_1000ppm"]
        self.assertEqual(len(default["pairs"]), 3)
        self.assertAlmostEqual(default["pairs"][0]["median_latency_change_pct"], 100 * (22.262 / 20.437 - 1))
        self.assertAlmostEqual(default["pairs"][0]["cpu_per_request_change_pct"], 100 * ((106 / 80064) / (97 / 87915) - 1))

    def test_input_order_does_not_change_round_pairing(self):
        self.assertEqual(report.summarize(self.read(self.rows), 100), report.summarize(self.read(list(reversed(self.rows))), 100))

    def test_partial_and_duplicate_rounds_refused(self):
        for rows in (self.rows[:-1], self.rows + [self.rows[0]], self.rows[:-1] + [self.rows[0]]):
            with self.subTest(size=len(rows)), self.assertRaises(ValueError):
                self.read(rows)

    def test_invalid_measurements(self):
        for field, value in (("schema_version", True), ("schema_version", 2), ("round", 3), ("mode", "unknown"),
                ("requests", 0), ("requests", True), ("requests", 100001), ("seconds", 0),
                ("median_us", float("nan")), ("p95_us", float("inf")), ("p95_us", 1),
                ("requests_per_second", 1), ("worker_cpu_ticks", -1), ("peak_rss_kib", 0),
                ("scrapes", 1), ("median_scrape_us", 0), ("final_trace_accounting", ["secret"])):
            with self.subTest(field=field, value=value):
                rows = [dict(row) for row in self.rows]
                rows[0][field] = value
                with self.assertRaises(ValueError):
                    self.read(rows)

    def test_unknown_fields_and_duplicate_json_keys(self):
        rows = [dict(row) for row in self.rows]
        rows[0]["unreviewed_field"] = "value"
        with self.assertRaises(ValueError):
            self.read(rows)
        raw = BASELINE.read_text().replace('"schema_version":1', '"schema_version":1,"schema_version":1', 1)
        with self.assertRaises(ValueError):
            self.read(raw=raw)

    def test_byte_and_line_bounds(self):
        with self.assertRaises(ValueError):
            self.read(raw=" " * 65537)
        raw = BASELINE.read_text().replace("\n", " " * 8193 + "\n", 1)
        with self.assertRaises(ValueError):
            self.read(raw=raw)

    def test_trace_format_and_overflow(self):
        index = next(index for index, row in enumerate(self.rows) if row["mode"] == "sample_all")
        for value in ("unexpected", self.rows[index]["final_trace_accounting"][0].replace("accepted: ", "accepted: 99999999999999999999", 1)):
            rows = [dict(row) for row in self.rows]
            rows[index]["final_trace_accounting"] = [value, self.rows[index]["final_trace_accounting"][1]]
            with self.assertRaises(ValueError):
                self.read(rows)

    def test_losses_are_preserved_not_mistaken_for_a_pass(self):
        index = next(index for index, row in enumerate(self.rows) if row["mode"] == "sample_all" and row["round"] == 0)
        self.rows[index]["final_trace_accounting"][0] = self.rows[index]["final_trace_accounting"][0].replace("shutdown_dropped: 0", "shutdown_dropped: 128")
        result = report.summarize(self.read(self.rows), 100)
        accounting = result["comparisons"]["sample_all"]["pairs"][0]["process_lifetime_trace_accounting"]
        self.assertEqual(accounting["trace exporter stopped: ExportStats"]["shutdown_dropped"], 128)
        self.assertIn("not assessed", result["acceptance"])

    def test_clock_frequency_and_zero_baseline(self):
        records = self.read(self.rows)
        for ticks in (0, -1, True, 1000001):
            with self.assertRaises(ValueError):
                report.summarize(records, ticks)
        records[(0, "disabled")]["worker_cpu_ticks"] = 0
        with self.assertRaises(ValueError):
            report.summarize(records, 100)

    def test_nonfinite_derived_ratio_is_refused(self):
        records = self.read(self.rows)
        records[(0, "disabled")]["median_us"] = 1e-300
        records[(0, "metrics")]["median_us"] = 1e300
        with self.assertRaises(ValueError):
            report.summarize(records, 100)


if __name__ == "__main__":
    unittest.main()
