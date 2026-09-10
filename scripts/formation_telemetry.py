"""Bounded IO and pure report contracts for the formation telemetry experiment."""
import http.client as http_client
import io
import json
import math
import os
from pathlib import Path
import selectors
import statistics
import threading
import time

from worker_formation_receipts import COUNTERS, LIFECYCLE, OPERATIONS, OUTCOMES

MODES = ("omitted", "compiled_off", "metrics", "zero_sample", "default_sample", "full_sample")
SIZES = (3, 10, 30)
CLK_TCK = os.sysconf("SC_CLK_TCK")


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def unique(items):
    result = {}
    for key, value in items:
        require(key not in result, "duplicate JSON field")
        result[key] = value
    return result


def read_json(path, cap=1048576):
    with Path(path).open("rb") as source:
        raw = source.read(cap + 1)
    require(len(raw) <= cap, "JSON artifact byte cap")
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _value: require(False, "non-finite JSON number"))


def write_json(path, value):
    raw = json.dumps(value, allow_nan=False, sort_keys=True).encode() + b"\n"
    require(len(raw) <= 1048576, "JSON artifact byte cap")
    with Path(path).open("xb") as output:
        output.write(raw)


def bounded_line(stream, timeout, cap=65536):
    """Unbuffered pipe reader: a stalled/oversized tool cannot hang the runner."""
    deadline = time.monotonic() + timeout
    result = bytearray()
    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_READ)
        while len(result) <= cap:
            require(selector.select(max(0, deadline - time.monotonic())), "tool output deadline")
            byte = os.read(stream.fileno(), 1)
            require(byte, "unexpected tool output EOF")
            if byte == b"\n":
                return bytes(result)
            result.extend(byte)
    raise ValueError("tool output line cap")


def proc(pid):
    """Bound /proc reads; missing instruments are errors, never zeroes."""
    with open(f"/proc/{pid}/stat", "rb") as source:
        raw = source.read(4097)
    require(len(raw) <= 4096, "proc stat cap")
    fields = raw.rsplit(b")", 1)[1].split()
    with open(f"/proc/{pid}/status", "rb") as source:
        raw = source.read(65537)
    require(len(raw) <= 65536, "proc status cap")
    values = {line.split(b":", 1)[0]: line.split()[1] for line in raw.splitlines()
              if line.startswith((b"VmRSS:", b"VmHWM:", b"VmSwap:"))}
    return {"ticks": int(fields[11]) + int(fields[12]),
            "rss_kib": int(values[b"VmRSS"]), "hwm_kib": int(values[b"VmHWM"]),
            "swap_kib": int(values[b"VmSwap"])}


def http(port):
    connection = http_client.HTTPConnection("127.0.0.1", port, timeout=1)
    try:
        connection.request("GET", "/metrics")
        response = connection.getresponse()
        body = response.read(32769)
        require(response.status == 200 and len(body) <= 32768, "scrape status/byte cap")
        return body
    finally:
        connection.close()


def metrics(port):
    return {key: float(value) for line in http(port).decode("ascii").splitlines()
            if line and not line.startswith("#") for key, value in [line.split()]}


class LogDrain:
    """Drain bounded records; retain only counters, never unbounded span history.

    Python validation/sink CPU belongs to the runner, not the worker. Exact
    span-ID correlation is covered by the separately retained causal preflight.
    """
    def __init__(self, stream):
        self.stream = io.BufferedReader(stream, buffer_size=65536)
        self.counts = {}
        self.accounting = {}
        self.bytes = 0
        self.errors = 0
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def record(self, raw):
        require(len(raw) <= 320 and raw.endswith(b"\n"), "bounded complete log record")
        row = json.loads(raw, object_pairs_hook=unique)
        keys = {"version", "event", "outcome", "unix_nanos"}
        require(type(row["version"]) is int and row["version"] == 1, "log schema")
        require(type(row["unix_nanos"]) is int and 0 < row["unix_nanos"] < 2**64, "log time")
        event = row["event"]
        require(row["outcome"] in OUTCOMES, "log outcome")
        if event in OPERATIONS:
            keys |= {"trace_id", "span_id"}
            for name, length in (("trace_id", 32), ("span_id", 16)):
                value = row[name]
                require(type(value) is str and len(value) == length
                        and all(c in "0123456789abcdef" for c in value) and int(value, 16), "log ID")
        elif event == "orishu.trace.accounting":
            keys |= {"counter", "value"}
            require(row["counter"] in COUNTERS and row["counter"] not in self.accounting, "log accounting key")
            require(type(row["value"]) is int and 0 <= row["value"] < 2**64, "log counter")
            self.accounting[row["counter"]] = row["value"]
        else:
            require(event in LIFECYCLE, "log event")
        require(set(row) == keys, "unexpected log fields")
        self.counts[event] = self.counts.get(event, 0) + 1

    def run(self):
        try:
            while raw := self.stream.readline(321):
                self.bytes += len(raw)
                try:
                    self.record(raw)
                except (ValueError, KeyError, TypeError):
                    self.errors += 1
        except OSError:
            self.errors += 1
        finally:
            self.stream.close()

    def finish(self):
        self.thread.join(timeout=1)
        require(not self.thread.is_alive(), "log reader exit deadline")
        return {"counts": self.counts, "accounting": self.accounting,
                "bytes": self.bytes, "errors": self.errors}


class Scraper:
    """One in-flight request, no catch-up bursts, fixed forty-slot schedule."""
    def __init__(self, port, start):
        self.port, self.start = port, start
        self.rows = []
        self.skipped = 0
        self.failed = 0
        self.thread = threading.Thread(target=self.run, daemon=True)

    def run(self):
        for slot in range(40):
            due = self.start + slot * 0.25
            if time.monotonic() >= due + 0.25:
                self.skipped += 1
                continue
            time.sleep(max(0, due - time.monotonic()))
            began = time.monotonic()
            try:
                body = http(self.port)
                self.rows.append((time.monotonic() - began, len(body)))
            except (OSError, http_client.HTTPException, ValueError):
                self.failed += 1

    def finish(self):
        self.thread.join(timeout=2)
        require(not self.thread.is_alive(), "scraper exit deadline")
        samples = sorted(row[0] * 1e6 for row in self.rows)
        return {"scheduled": 40, "completed": len(samples), "skipped": self.skipped,
                "failed": self.failed, "bytes": sum(row[1] for row in self.rows),
                "median_us": statistics.median(samples) if samples else None,
                "p95_us": samples[(len(samples) - 1) * 95 // 100] if samples else None}


def band(change):
    require(type(change) in (int, float) and math.isfinite(change), "unavailable cost ratio")
    if change < 10:
        return "normal_goal"
    if change <= 20:
        return "temporary_only_requires_disposition"
    if change <= 25:
        return "outside_temporary_tolerance"
    return "troubleshooting"


def ratio(value, baseline):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0, "invalid numerator")
    require(type(baseline) in (int, float) and math.isfinite(baseline) and baseline > 0, "unavailable baseline denominator")
    return 100 * (value / baseline - 1)
