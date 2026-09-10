"""Negative controls for supervisor-collected log/Collector receipt matching."""
import json
import unittest

from worker_deployment_receipts import journal_messages, match_invocations
from worker_formation_receipts import COUNTERS


def fixture():
    spans, invocations = [], []
    for phase in (1, 2):
        rows = []
        for number in range(8):
            trace, span_id = f"{phase:032x}", f"{number + 1:016x}"
            outcome = "rejected" if number == 2 else "completed"
            rows.append({"version": 1, "event": "orishu.client.request", "outcome": outcome,
                         "unix_nanos": 2, "trace_id": trace, "span_id": span_id})
            spans.append({"traceId": trace, "spanId": span_id, "name": "orishu.client.request",
                          "endTimeUnixNano": "2", "attributes": [{"key": "orishu.outcome",
                          "value": {"stringValue": outcome}}]})
        for event in ("ready", "stopping", "stopped"):
            rows.append({"version": 1, "event": "orishu.worker." + event, "outcome": "completed", "unix_nanos": 2})
        for name in COUNTERS:
            rows.append({"version": 1, "event": "orishu.trace.accounting", "outcome": "completed",
                         "unix_nanos": 2, "counter": name, "value": 8 if name in ("accepted", "enqueued") else 0})
        invocations.append(rows)
    return spans, invocations


class DeploymentReceipts(unittest.TestCase):
    def test_exact_two_clean_invocations(self):
        self.assertEqual(match_invocations(*fixture()), 16)

    def test_missing_receipts_logs_and_previous_invocation_reuse(self):
        spans, invocations = fixture()
        for received, logs in ((spans[:-1], invocations), (spans + [spans[0]], invocations),
                               (spans, [invocations[0], invocations[0]]),
                               (spans, [invocations[0][1:], invocations[1]])):
            with self.assertRaises(AssertionError):
                match_invocations(received, logs)

    def test_wrong_name_timestamp_outcome_and_parent(self):
        for key, value in (("name", "orishu.peer.exchange"), ("endTimeUnixNano", "3"),
                           ("attributes", [{"value": {"stringValue": "failed"}}]),
                           ("parentSpanId", "f" * 16)):
            spans, invocations = fixture()
            spans[0][key] = value
            with self.assertRaises(AssertionError):
                match_invocations(spans, invocations)

    def test_lifecycle_and_final_accounting_required(self):
        for category in ("missing", "failure", "counter"):
            spans, invocations = fixture()
            if category == "missing":
                invocations[0].pop(8)
            elif category == "failure":
                invocations[0][8]["outcome"] = "failed"
            else:
                next(row for row in invocations[0] if row.get("counter") == "failed")["value"] = 1
            with self.assertRaises(AssertionError):
                match_invocations(spans, invocations)

    def test_journal_matches_only_worker_invocation(self):
        record = {"_SYSTEMD_INVOCATION_ID": "a" * 32, "_PID": "123", "_TRANSPORT": "stdout", "MESSAGE": '{"version":1}'}
        encode = lambda row: (json.dumps(row) + "\n").encode()
        self.assertEqual(journal_messages(encode(record), "a" * 32, "123"), b'{"version":1}\n')
        for key, value in (("_SYSTEMD_INVOCATION_ID", "b" * 32), ("_PID", "124"),
                           ("_TRANSPORT", "journal"), ("MESSAGE", [1, 2]),
                           ("MESSAGE", "first\nsecond"), ("MESSAGE", "x" * 320)):
            row = dict(record, **{key: value})
            with self.assertRaises(AssertionError):
                journal_messages(encode(row), "a" * 32, "123")
        with self.assertRaisesRegex(AssertionError, "secret"):
            journal_messages(encode(record), "a" * 32, "123", (b"version",))

    def test_journal_truncation_duplicates_and_limits_fail(self):
        for raw in (b"x", b"x" * (512 * 1024 + 1), b"{}\n" * 513,
                    b'{"MESSAGE":"x","MESSAGE":"y"}\n'):
            with self.assertRaises(AssertionError):
                journal_messages(raw, "a" * 32, "123")

    def test_public_collection_metadata_is_not_a_worker_record_field(self):
        row = {"_SYSTEMD_INVOCATION_ID": "a" * 32, "_PID": "123", "_TRANSPORT": "stdout",
               "_CMDLINE": "worker --name public-marker", "MESSAGE": '{"version":1}'}
        encode = lambda: (json.dumps(row) + "\n").encode()
        self.assertEqual(journal_messages(encode(), "a" * 32, "123", (b"secret-token",),
                                         (b"public-marker",)), b'{"version":1}\n')
        row["MESSAGE"] = "public-marker"
        with self.assertRaisesRegex(AssertionError, "record field"):
            journal_messages(encode(), "a" * 32, "123", (b"secret-token",), (b"public-marker",))
        row["MESSAGE"] = '{}'
        row["_CMDLINE"] = "worker secret-token"
        with self.assertRaisesRegex(AssertionError, "secret"):
            journal_messages(encode(), "a" * 32, "123", (b"secret-token",), (b"public-marker",))


if __name__ == "__main__":
    unittest.main()
