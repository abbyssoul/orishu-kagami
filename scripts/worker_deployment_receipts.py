"""Strict journal/stdout and OTLP receipt checks for deployment examples."""
import json
import re

from worker_formation_receipts import COUNTERS, MAX_LOG_BYTES, OPERATIONS, collector


def journal_messages(raw, invocation, pid, forbidden=(), record_forbidden=()):
    """Accept only the selected worker invocation's journal stream messages."""
    assert re.fullmatch("[0-9a-f]{32}", invocation)
    assert str(pid).isdigit() and int(pid) > 0
    assert len(raw) <= 512 * 1024, "journal query byte budget"
    assert all(secret not in raw for secret in forbidden if secret), "secret in journal"
    assert not raw or raw.endswith(b"\n"), "partial journal query"
    lines = raw.splitlines()
    assert len(lines) <= 512, "journal record budget"
    messages = []
    total = 0
    for line in lines:
        record = json.loads(line, object_pairs_hook=collector.unique_object)
        assert record["_SYSTEMD_INVOCATION_ID"] == invocation, "wrong journal invocation"
        assert record["_PID"] == str(pid), "wrong worker PID"
        assert record["_TRANSPORT"] == "stdout", "not a collected standard stream"
        message = record["MESSAGE"]
        assert isinstance(message, str) and "\n" not in message and "\r" not in message
        encoded = message.encode() + b"\n"
        assert all(marker not in encoded for marker in record_forbidden if marker), "forbidden worker record field"
        assert len(encoded) <= 320, "journal message exceeds worker record bound"
        total += len(encoded)
        assert total <= MAX_LOG_BYTES, "journal message byte budget"
        messages.append(encoded)
    return b"".join(messages)


def match_invocations(spans, invocations, expected_operations=8):
    """Two clean starts: exact receipts and fresh IDs, never old logs as proof."""
    assert len(invocations) == 2
    received = {(span["traceId"], span["spanId"]): span for span in spans}
    assert len(received) == len(spans), "duplicate received identity"
    logged = set()
    for records in invocations:
        operations = [record for record in records if record["event"] in OPERATIONS]
        assert len(operations) == expected_operations, "missing or extra operation logs"
        assert sum(record["outcome"] == "rejected" for record in operations) == 1
        for event in ("ready", "stopping", "stopped"):
            rows = [record for record in records if record["event"] == "orishu.worker." + event]
            assert len(rows) == 1 and rows[0]["outcome"] == "completed", "incomplete clean lifecycle"
        assert len(records) == expected_operations + 3 + 12, "unexpected lifecycle/failure record"
        accounting = [record for record in records if record["event"] == "orishu.trace.accounting"]
        assert len(accounting) == 12 and {row["counter"] for row in accounting} == COUNTERS
        counts = {row["counter"]: row["value"] for row in accounting}
        assert counts["accepted"] == counts["enqueued"] == expected_operations
        assert all(value == 0 for name, value in counts.items() if name not in ("accepted", "enqueued"))
        for record in operations:
            key = record["trace_id"], record["span_id"]
            assert key not in logged, "reused operation identity across invocations"
            logged.add(key)
            assert key in received, "operation log lacks actual Collector receipt"
            span = received[key]
            assert span["name"] == record["event"] == "orishu.client.request"
            assert not span.get("parentSpanId")
            assert int(span["endTimeUnixNano"]) == record["unix_nanos"]
            assert span["attributes"][0]["value"]["stringValue"] == record["outcome"]
    assert set(received) == logged, "received span lacks collected log"
    return len(logged)
