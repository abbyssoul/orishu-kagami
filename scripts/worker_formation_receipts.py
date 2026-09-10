"""Bounded offline validation for the local formation Collector/log recipe."""
import importlib.util
import json
from pathlib import Path
import re

_spec = importlib.util.spec_from_file_location("formation_collector", Path(__file__).with_name("check-worker-otelcol.py"))
collector = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(collector)

MAX_LOG_BYTES = 131072
OPERATIONS = {"orishu.client.request", "orishu.peer.exchange", "orishu.admission"}
LIFECYCLE = {"orishu.worker.ready", "orishu.worker.stopping", "orishu.worker.stopped",
             "orishu.worker.failed", "orishu.diagnostics.failed", "orishu.trace_exporter.failed"}
OUTCOMES = {"completed", "rejected", "failed", "cancelled"}
COUNTERS = {"sampled_out", "active_full", "queue_full", "closed", "invalid_source", "enqueued",
            "accepted", "rejected", "failed", "encoding_dropped", "shutdown_dropped", "warnings"}


def join_phase_complete(phase):
    """Observe an existing operation; catch-up failure may precede worker retry."""
    if phase not in ("connecting", "admitting", "catchingUp", "catchUpFailed", "joined"):
        raise ValueError("join failed")
    return phase == "joined"


def uint64(value):
    return type(value) is int and 0 <= value < 2**64


def read_logs(path, forbidden=(), complete=False):
    if not path.exists():
        return []
    with path.open("rb") as stream:
        raw = stream.read(MAX_LOG_BYTES + 1)
    assert len(raw) <= MAX_LOG_BYTES, "worker log capture exceeded byte budget"
    assert all(secret not in raw for secret in forbidden if secret), "secret in worker log"
    if complete:
        assert not raw or raw.endswith(b"\n"), "incomplete final worker log"
    records = []
    for line in raw.splitlines(keepends=True):
        assert len(line) <= 320, "log record exceeded byte budget"
        if not line.endswith(b"\n"):
            break
        record = json.loads(line, object_pairs_hook=collector.unique_object)
        keys = {"version", "event", "outcome", "unix_nanos"}
        assert type(record["version"]) is int and record["version"] == 1
        assert uint64(record["unix_nanos"]) and record["unix_nanos"] > 0
        assert record["outcome"] in OUTCOMES
        if record["event"] in OPERATIONS:
            keys |= {"trace_id", "span_id"}
            for field, size in (("trace_id", 32), ("span_id", 16)):
                value = record[field]
                assert re.fullmatch(f"[0-9a-f]{{{size}}}", value) and int(value, 16)
        elif record["event"] == "orishu.trace.accounting":
            keys |= {"counter", "value"}
            assert record["counter"] in COUNTERS and uint64(record["value"])
            assert record["outcome"] == "completed"
        else:
            assert record["event"] in LIFECYCLE, "unknown operational event"
        assert set(record) == keys, "unexpected operational log fields"
        records.append(record)
        assert len(records) <= 512, "worker log record count exceeded"
    return records


def correlate(spans, logs):
    """Require exact received client→peer→admission chains for A→B and B→C.

    Worker ownership comes from separately captured stdout handles, never an
    untrusted log attribute or an invented OTLP resource field.
    """
    assert len(logs) == 3
    by_id = {}
    for role, records in enumerate(logs):
        for record in records:
            if record["event"] in OPERATIONS:
                key = record["trace_id"], record["span_id"]
                assert key not in by_id, "duplicate logged span identity"
                by_id[key] = role, record
    received = {}
    for span in spans:
        key = span["traceId"], span["spanId"]
        assert key not in received, "duplicate received span"
        assert key in by_id, "received span has no matching worker log"
        role, record = by_id[key]
        assert record["event"] == span["name"], "span/log event mismatch"
        assert record["unix_nanos"] == int(span["endTimeUnixNano"]), "span/log timestamp mismatch"
        assert record["outcome"] == span["attributes"][0]["value"]["stringValue"], "span/log outcome mismatch"
        received[key] = role, span
    peers = [(role, span) for role, span in received.values() if span["name"] == "orishu.peer.exchange"]
    admissions = [(role, span) for role, span in received.values() if span["name"] == "orishu.admission"]
    assert len(peers) == len(admissions) == 2, "expected exactly two happy-path admissions"
    for source, target in ((1, 0), (2, 1)):
        matches = [span for role, span in peers if role == source]
        assert len(matches) == 1, "missing or duplicate source peer exchange"
        peer = matches[0]
        root_role, root = received[(peer["traceId"], peer["parentSpanId"])]
        assert root_role == source and root["name"] == "orishu.client.request"
        assert not root.get("parentSpanId"), "CLI join must start a local root"
        children = [(role, span) for role, span in admissions
                    if span["traceId"] == peer["traceId"] and span.get("parentSpanId") == peer["spanId"]]
        assert len(children) == 1 and children[0][0] == target, "wrong admission parent or worker"
        for span in (root, peer, children[0][1]):
            assert span["attributes"][0]["value"]["stringValue"] == "completed"
    assert set(received) == set(by_id), "logged operation has no received span"
    return len(received)
