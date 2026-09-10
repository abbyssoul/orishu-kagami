"""Negative controls for role-aware formation Collector/log receipt validation."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import sys
import unittest

from worker_formation_receipts import collector, correlate, read_logs

_spec = importlib.util.spec_from_file_location("formation_journey", Path(__file__).with_name("check-worker-formation-otelcol.py"))
journey = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(journey)


def fixture():
    spans, logs = [], [[], [], []]
    for source, target in ((1, 0), (2, 1)):
        trace = f"{source:032x}"
        for offset, name, kind, role in ((1, "orishu.client.request", 2, source),
                                         (2, "orishu.peer.exchange", 3, source),
                                         (3, "orishu.admission", 2, target)):
            span = {"name": name, "kind": kind, "traceId": trace,
                    "spanId": f"{offset:016x}", "flags": 1,
                    "startTimeUnixNano": "1", "endTimeUnixNano": "2",
                    "attributes": [{"key": "orishu.outcome", "value": {"stringValue": "completed"}}]}
            if offset > 1:
                span["parentSpanId"] = f"{offset - 1:016x}"
            spans.append(span)
            logs[role].append({"version": 1, "event": name, "outcome": "completed", "unix_nanos": 2,
                               "trace_id": trace, "span_id": span["spanId"]})
    return spans, logs


class FormationReceipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name) / "receipt.jsonl"
        self.spans, self.logs = fixture()

    def write_spans(self, spans):
        document = {"resourceSpans": [{"resource": {"attributes": [
            {"key": "service.name", "value": {"stringValue": "orishu-worker"}}]},
            "scopeSpans": [{"scope": {"name": "orishu.worker", "version": "0.1.0"}, "spans": spans}]}]}
        self.path.write_text(json.dumps(document) + "\n")

    def write_logs(self, rows):
        self.path.write_text("".join(json.dumps(row) + "\n" for row in rows))

    def test_valid_causal_receipts_are_explicitly_not_local_root_profile(self):
        self.write_spans(self.spans)
        self.assertEqual(collector.read_spans(self.path, admission=True, complete=True), self.spans)
        with self.assertRaises(AssertionError):
            collector.read_spans(self.path)
        self.assertEqual(correlate(self.spans, self.logs), 6)
        for rows in self.logs:
            self.write_logs(rows)
            self.assertEqual(read_logs(self.path, complete=True), rows)

    def test_formation_kinds_flags_parents_and_catalogue_are_checked(self):
        for field, value in (("kind", 2), ("flags", 0), ("name", "raw-path"),
                             ("parentSpanId", "0" * 16), ("parentSpanId", "F" * 16),
                             ("parentSpanId", self.spans[1]["spanId"])):
            with self.subTest(field=field, value=value):
                spans = copy.deepcopy(self.spans)
                spans[1][field] = value
                self.write_spans(spans)
                with self.assertRaises(AssertionError):
                    collector.read_spans(self.path, admission=True)

    def test_wrong_role_parent_missing_receipt_and_duplicates_fail(self):
        cases = []
        logs = copy.deepcopy(self.logs)
        logs[2].append(logs[0].pop())
        cases.append((self.spans, logs))
        spans = copy.deepcopy(self.spans)
        spans[2]["parentSpanId"] = "f" * 16
        cases.append((spans, self.logs))
        cases.append((self.spans[:-1], self.logs))
        logs = copy.deepcopy(self.logs)
        logs[1].pop(0)
        cases.append((self.spans, logs))
        logs = copy.deepcopy(self.logs)
        logs[0].append(logs[0][0])
        cases.append((self.spans, logs))
        for spans, logs in cases:
            with self.assertRaises((AssertionError, KeyError)):
                correlate(spans, logs)

    def test_log_timestamp_outcome_and_event_must_match_received_span(self):
        for field, value in (("unix_nanos", 3), ("outcome", "failed"), ("event", "orishu.client.request")):
            logs = copy.deepcopy(self.logs)
            logs[0][0][field] = value
            with self.assertRaises(AssertionError):
                correlate(self.spans, logs)

    def test_log_schema_bounds_redaction_and_partial_lines(self):
        original = self.logs[0][0]
        for field, value in (("version", True), ("unix_nanos", 2**64), ("trace_id", "0" * 32),
                             ("span_id", "F" * 16), ("payload", "private"), ("event", "private")):
            row = dict(original, **{field: value})
            self.write_logs([row])
            with self.assertRaises((AssertionError, KeyError)):
                read_logs(self.path)
        self.write_logs([original])
        with self.assertRaisesRegex(AssertionError, "secret"):
            read_logs(self.path, (original["trace_id"].encode(),))
        self.path.write_text(json.dumps(original))
        self.assertEqual(read_logs(self.path), [])
        with self.assertRaisesRegex(AssertionError, "incomplete"):
            read_logs(self.path, complete=True)
        self.path.write_bytes(b"x" * 321)
        with self.assertRaisesRegex(AssertionError, "record exceeded"):
            read_logs(self.path)
        self.path.write_bytes(b"x" * 131073)
        with self.assertRaisesRegex(AssertionError, "capture exceeded"):
            read_logs(self.path)
        self.path.write_bytes(b'{"version":1,"version":1}\n')
        with self.assertRaisesRegex(AssertionError, "duplicate"):
            read_logs(self.path)

    def test_final_counters_have_no_identity_or_arbitrary_fields(self):
        row = {"version": 1, "event": "orishu.trace.accounting", "outcome": "completed",
               "unix_nanos": 2, "counter": "accepted", "value": 2**64 - 1}
        self.write_logs([row])
        self.assertEqual(read_logs(self.path), [row])
        for field, value in (("value", True), ("counter", "private"), ("span_id", "a" * 16)):
            self.write_logs([dict(row, **{field: value})])
            with self.assertRaises(AssertionError):
                read_logs(self.path)

    def test_capture_does_not_persist_overflow_or_block_child_output(self):
        for count in (128, 131073):
            path = self.path.with_name(f"stdout-{count}.jsonl")
            command = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write(b'x' * {count})"]
            if count <= 131072:
                with journey.recorded_worker(command, dict(os.environ), path) as child:
                    self.assertEqual(child.wait(timeout=3), 0)
            else:
                with self.assertRaisesRegex(AssertionError, "overflowed"):
                    with journey.recorded_worker(command, dict(os.environ), path) as child:
                        self.assertEqual(child.wait(timeout=3), 0)
            self.assertEqual(path.stat().st_size, min(count, 131072))

    def test_formation_receipt_span_cap_is_distinct_and_finite(self):
        spans = [dict(self.spans[0], spanId=f"{index + 1:016x}") for index in range(256)]
        self.write_spans(spans)
        self.assertEqual(len(collector.read_spans(self.path, admission=True)), 256)
        self.write_spans(spans + [dict(self.spans[0], spanId="f" * 16)])
        with self.assertRaisesRegex(AssertionError, "span budget"):
            collector.read_spans(self.path, admission=True)


if __name__ == "__main__":
    unittest.main()
