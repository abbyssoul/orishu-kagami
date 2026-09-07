#!/usr/bin/env python3
"""Validate and compare the historical single-worker v1 report, never accept M4."""
import argparse
import json
import math
from pathlib import Path
import re
import statistics

MODES = ("disabled", "metrics", "zero_sample", "sample_1000ppm", "sample_all")
FIELDS = {"schema_version", "round", "mode", "requests", "seconds", "requests_per_second",
          "median_us", "p95_us", "worker_cpu_ticks", "rss_kib", "peak_rss_kib", "scrapes",
          "median_scrape_us", "final_trace_accounting"}
COUNTERS = {
    "trace exporter stopped: ExportStats": ("accepted", "rejected", "failed", "encoding_dropped", "shutdown_dropped", "warnings"),
    "trace queue stopped: QueueStats": ("sampled_out", "active_full", "queue_full", "closed", "invalid_source", "enqueued"),
}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def unique_object(items):
    result = {}
    for key, value in items:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def integer(value, low=0, high=2**64 - 1):
    return type(value) is int and low <= value <= high


def number(value, low=0):
    return type(value) in (float, int) and math.isfinite(value) and value >= low


def read_report(path):
    with Path(path).open("rb") as source:
        data = source.read(65537)
    require(len(data) <= 65536, "report exceeds 64 KiB")
    lines = data.splitlines()
    require(len(lines) == 15, "v1 requires three complete five-mode rounds; partial data is not acceptance")
    records = {}
    for line in lines:
        require(len(line) <= 8192, "report line exceeds 8 KiB")
        row = json.loads(line, object_pairs_hook=unique_object)
        require(type(row) is dict and set(row) == FIELDS, "unsupported v1 fields")
        require(type(row["schema_version"]) is int and row["schema_version"] == 1, "unsupported schema")
        require(integer(row["round"], 0, 2) and row["mode"] in MODES, "unknown round or mode")
        key = (row["round"], row["mode"])
        require(key not in records, "duplicate round/mode")
        for field in ("requests", "worker_cpu_ticks", "rss_kib", "peak_rss_kib", "scrapes"):
            require(integer(row[field]), "invalid integer measurement")
        require(0 < row["requests"] <= 100000 and row["scrapes"] <= 100, "v1 work bound")
        for field in ("seconds", "requests_per_second", "median_us", "p95_us"):
            require(number(row[field]) and row[field] > 0, "invalid finite measurement")
        require(row["seconds"] <= 180 and row["p95_us"] >= row["median_us"], "invalid duration or quantile ordering")
        require(row["peak_rss_kib"] >= row["rss_kib"], "RSS exceeds high-water RSS")
        require(math.isclose(row["requests_per_second"], row["requests"] / row["seconds"], rel_tol=1e-9), "throughput disagrees with count/window")
        if row["mode"] == "disabled":
            require(row["scrapes"] == 0 and row["median_scrape_us"] is None, "disabled scrape inconsistency")
        else:
            require(row["scrapes"] > 0 and number(row["median_scrape_us"]) and row["median_scrape_us"] > 0, "missing enabled scrape evidence")
        reports = row["final_trace_accounting"]
        require(type(reports) is list and len(reports) == (0 if row["mode"] in MODES[:2] else 2), "trace accounting count")
        counters = {}
        for report, (prefix, fields) in zip(reports, COUNTERS.items()):
            pattern = re.escape(prefix) + r" \{ " + ", ".join(re.escape(field) + r": ([0-9]{1,20})" for field in fields) + r" \}"
            match = re.fullmatch(pattern, report) if type(report) is str else None
            require(match is not None, "unsupported trace accounting format")
            values = [int(value) for value in match.groups()]
            require(all(integer(value) for value in values), "trace counter overflow")
            counters[prefix] = dict(zip(fields, values))
        row["parsed_accounting"] = counters
        records[key] = row
    require(set(records) == {(round_, mode) for round_ in range(3) for mode in MODES}, "incomplete mode matrix")
    return records


def summarize(records, clock_ticks):
    require(integer(clock_ticks, 1, 1000000), "explicit positive CLK_TCK required")
    comparison = {}
    for mode in MODES[1:]:
        pairs = []
        for round_ in range(3):
            baseline, row = records[(round_, "disabled")], records[(round_, mode)]
            require(baseline["worker_cpu_ticks"] > 0, "CPU ratio unavailable with zero baseline ticks")
            baseline_cpu = baseline["worker_cpu_ticks"] / baseline["requests"]
            pairs.append({"round": round_,
                "median_latency_change_pct": 100 * (row["median_us"] / baseline["median_us"] - 1),
                "p95_latency_change_pct": 100 * (row["p95_us"] / baseline["p95_us"] - 1),
                "throughput_change_pct": 100 * (row["requests_per_second"] / baseline["requests_per_second"] - 1),
                "cpu_per_request_change_pct": 100 * ((row["worker_cpu_ticks"] / row["requests"]) / baseline_cpu - 1),
                "worker_cpu_us_per_request": row["worker_cpu_ticks"] * 1000000 / clock_ticks / row["requests"],
                "rss_change_kib": row["rss_kib"] - baseline["rss_kib"],
                "median_scrape_us": row["median_scrape_us"],
                # Process-lifetime accounting includes startup, warm-up and drain.
                # It is never divided by timed requests as an invented loss rate.
                "process_lifetime_trace_accounting": row["parsed_accounting"]})
        fields = [key for key in pairs[0] if key not in ("round", "process_lifetime_trace_accounting")]
        require(all(number(abs(pair[field])) for pair in pairs for field in fields), "non-finite derived comparison")
        comparison[mode] = {"pairs": pairs, "paired_summaries": {
            field: {"minimum": min(pair[field] for pair in pairs),
                    "median": statistics.median(pair[field] for pair in pairs),
                    "maximum": max(pair[field] for pair in pairs)} for field in fields}}
    return {"schema_version": 1, "scope": "historical single-worker closed-loop v1 comparison",
            "acceptance": "not assessed; no reviewed budget; not a three-worker/final-M4 result",
            "clock_ticks_per_second": clock_ticks, "comparisons": comparison}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--clock-ticks", type=int, required=True, help="CLK_TCK recorded on the measurement host, not this reader's host")
    args = parser.parse_args()
    try:
        result = summarize(read_report(args.report), args.clock_ticks)
    except (ValueError, TypeError, OverflowError, RecursionError, OSError) as error:
        parser.exit(2, "invalid or unreadable overhead report (contents withheld): " + type(error).__name__ + "\n")
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
