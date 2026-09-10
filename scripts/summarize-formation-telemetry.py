#!/usr/bin/env python3
"""Report every v2 cell and paired scaling result; never turn missing data into a pass."""
import argparse
import hashlib
import math
import statistics
from pathlib import Path

from formation_telemetry import (COUNTERS, MODES, SIZES, band, ratio, read_json, require)


def issues(row):
    """Correctness and instrumentation validity gates, separate from cost bands."""
    if row["status"] != "complete":
        return ["failed_or_incomplete_cell"]
    result = []
    mode = row["mode"]
    count = row["worker_count"]
    require(row["load"]["schema_version"] == 2 and row["load"]["seconds"] == 10, "window/schema mismatch")
    require(len(row["load"]["workers"]) == len(row["resources"]) == len(row["logs"]) == count, "missing worker instrument")
    require([value["role"] for value in row["load"]["workers"]] == list(range(count)), "worker role mismatch")
    require(set(row["tooling"]) == {"collector", "load_generator", "runner_and_log_sink"}, "missing tooling instrument")
    for resource in row["resources"]:
        require(all(type(resource[key]) in (int, float) and math.isfinite(resource[key]) and resource[key] >= 0
                    for key in ("cpu_seconds", "rss_peak_kib", "swap_peak_kib")), "invalid resource measurement")
    require(row["collector"]["schema_version"] == 2 and len(row["collector"]["workers"]) == count, "collector schema/count")
    if abs(row["cpu_bracket_seconds"] - 10) > 0.1 or row["proc_missed"]:
        result.append("measurement_schedule_skew")
    if any(value["swap_peak_kib"] for value in row["resources"] + list(row["tooling"].values())):
        result.append("process_swap")
    for role in range(count):
        value = row["load"]["workers"][role]
        require(type(value["latency"]["requests"]) is int and 0 <= value["latency"]["requests"] <= 1000000, "invalid request count")
        if value["latency"]["requests"]:
            require(type(value["latency"]["p95_us"]) in (int, float) and math.isfinite(value["latency"]["p95_us"])
                    and value["latency"]["p95_us"] > 0, "invalid latency")
        if value["capped"] or not value["latency"]["requests"]:
            result.append(f"role_{role}_sample_cap_or_empty")
        if row["logs"][role]["errors"]:
            result.append(f"role_{role}_log_validation")
    if mode in MODES[:2]:
        require(row["scrapes"] == [] and row["metrics_before"] is None and row["metrics_after"] is None, "disabled instrumentation mismatch")
        if any(log["bytes"] for log in row["logs"]):
            result.append("disabled_logging_emitted")
        if any(receipt["batches"] for receipt in row["collector"]["workers"]):
            result.append("disabled_exporter_emitted")
        return result
    require(len(row["scrapes"]) == len(row["metrics_before"]) == len(row["metrics_after"]) == count, "missing scrape/metrics instrument")
    for role, (scrape, before, after, log, receipt) in enumerate(zip(row["scrapes"], row["metrics_before"], row["metrics_after"], row["logs"], row["collector"]["workers"])):
        require(scrape["scheduled"] == 40 and sum(scrape[key] for key in ("completed", "failed", "skipped")) == 40, "scrape accounting mismatch")
        if scrape["failed"] or scrape["skipped"]:
            result.append(f"role_{role}_scrape_loss")
        if mode == "metrics":
            if log["bytes"] or receipt["batches"]:
                result.append(f"role_{role}_unexpected_trace_log")
            continue
        if any(log["counts"].get(name, 0) != 1 for name in
               ("orishu.worker.ready", "orishu.worker.stopping", "orishu.worker.stopped")):
            result.append(f"role_{role}_lifecycle_delivery_incomplete")
        expected = {f"orishu_worker_trace_{name}_total" for name in COUNTERS}
        expected |= {f"orishu_worker_log_{name}_total" for name in ("accepted", "written", "queue_full", "contended", "encoding_failed", "invalid_source", "output_failed", "closed", "shutdown_dropped")}
        require(expected <= set(before) and expected <= set(after), "missing telemetry counters")
        require(all(after[key] >= before[key] >= 0 for key in expected), "non-monotonic telemetry counters")
        if set(log["accounting"]) != COUNTERS:
            result.append(f"role_{role}_missing_final_trace_accounting")
        else:
            counters = log["accounting"]
            if any(counters[key] for key in COUNTERS - {"sampled_out", "enqueued", "accepted", "warnings"}):
                result.append(f"role_{role}_lifetime_trace_loss")
            if counters["accepted"] != sum(receipt[key] for key in ("client", "peer", "admission")):
                result.append(f"role_{role}_receipt_count_mismatch")
        if any(after[f"orishu_worker_log_{key}_total"] for key in ("queue_full", "contended", "encoding_failed", "invalid_source", "output_failed", "closed", "shutdown_dropped")):
            result.append(f"role_{role}_log_loss_before_shutdown")
        emitted = sum(log["counts"].get(name, 0) for name in ("orishu.client.request", "orishu.peer.exchange", "orishu.admission"))
        if emitted != sum(receipt[key] for key in ("client", "peer", "admission")):
            result.append(f"role_{role}_log_receipt_count_mismatch")
    return result


def pair(row, base):
    require((row["worker_count"], row["round"], row["manifest_sha256"]) ==
            (base["worker_count"], base["round"], base["manifest_sha256"]), "mismatched paired identities")
    requests = sum(value["latency"]["requests"] for value in row["load"]["workers"])
    baseline_requests = sum(value["latency"]["requests"] for value in base["load"]["workers"])
    cpu = sum(value["cpu_seconds"] for value in row["resources"])
    baseline_cpu = sum(value["cpu_seconds"] for value in base["resources"])
    require(requests > 0 and baseline_requests > 0, "missing validated requests")
    return {"round": row["round"], "requests_per_second": requests / 10,
            "cpu_seconds": cpu, "cpu_per_request_us": cpu / requests * 1e6,
            "cpu_change_percent": ratio(cpu / requests, baseline_cpu / baseline_requests),
            "throughput_change_percent": ratio(requests, baseline_requests),
            "p95_change_percent": [ratio(value["latency"]["p95_us"], previous["latency"]["p95_us"])
                                   for value, previous in zip(row["load"]["workers"], base["load"]["workers"])],
            "rss_change_kib": [value["rss_peak_kib"] - previous["rss_peak_kib"]
                               for value, previous in zip(row["resources"], base["resources"])]}


def report(root):
    root = Path(root)
    manifest = read_json(root / "manifest.json")
    require(manifest["schema_version"] == 2 and manifest["kind"] == "formation-telemetry", "unsupported manifest")
    require(manifest["profile"]["sizes"] == list(SIZES) and manifest["profile"]["modes"] == list(MODES)
            and manifest["profile"]["rounds"] == 6 and manifest["profile"]["window_seconds"] == 10, "unsupported profile")
    digest = hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest()
    paths = sorted(root.glob("cell-*.json"))
    require(len(paths) <= 108, "cell count cap")
    rows = {}
    invalid = []
    for index, path in enumerate(paths):
        row = read_json(path)
        require(row["schema_version"] == 2 and row["manifest_sha256"] == digest, "cell schema/manifest mismatch")
        require(row["index"] == index, "missing/reordered cell index")
        require(row["worker_count"] in SIZES and type(row["round"]) is int and 0 <= row["round"] < 6, "cell identity outside matrix")
        expected_mode = MODES[(row["round"] + index % 6) % 6]
        require(row["mode"] == expected_mode, "mode rotation mismatch")
        key = row["worker_count"], row["round"], row["mode"]
        require(key not in rows, "duplicate measurement cell")
        rows[key] = row
        found = issues(row)
        if found:
            invalid.append({"cell": index, "issues": found})
    expected = {(size, round_, mode) for size in SIZES for round_ in range(6) for mode in MODES}
    finished = read_json(root / "finished.json") if (root / "finished.json").exists() else None
    output = {"schema_version": 2, "cells": len(rows), "missing_cells": len(expected - set(rows)),
              "finished": finished, "source_stability_verified": bool(finished and finished.get("source_unchanged")),
              "acceptance_run": manifest["acceptance_run"], "invalid": invalid, "comparisons": []}
    for size in SIZES:
        for mode in MODES[1:]:
            baseline_mode = "omitted" if mode == "compiled_off" else "compiled_off"
            pairs = []
            for round_ in range(6):
                row, base = rows.get((size, round_, mode)), rows.get((size, round_, baseline_mode))
                if row and base and row["status"] == base["status"] == "complete":
                    try:
                        pairs.append(pair(row, base))
                    except ValueError as error:
                        invalid.append({"cell": row["index"], "issues": [str(error)]})
            if len(pairs) != 6:
                output["comparisons"].append({"workers": size, "mode": mode, "status": "incomplete", "pairs": pairs})
                continue
            p95 = max(statistics.median(value["p95_change_percent"][role] for value in pairs) for role in range(size))
            cpu = statistics.median(value["cpu_change_percent"] for value in pairs)
            rss = max(statistics.median(value["rss_change_kib"][role] for value in pairs) for role in range(size))
            throughput = statistics.median(value["throughput_change_percent"] for value in pairs)
            baselines = [rows[size, round_, baseline_mode] for round_ in range(6)]
            base_rates = [sum(value["latency"]["requests"] for value in row["load"]["workers"]) for row in baselines]
            noisy = max(base_rates) / min(base_rates) > 1.10
            for role in range(size):
                values = [row["load"]["workers"][role]["latency"]["p95_us"] for row in baselines]
                noisy |= max(values) / min(values) > 1.10
            noisy |= any(value["cpu_change_percent"] > 20 or max(value["p95_change_percent"]) > 20 for value in pairs)
            output["comparisons"].append({"workers": size, "mode": mode, "pairs": pairs,
                "status": "inconclusive_noise" if noisy else "measured_requires_gate_review",
                "worst_role_median_p95_change_percent": p95, "median_cpu_change_percent": cpu,
                "p95_band": band(p95), "cpu_band": band(cpu),
                "worst_role_median_rss_change_kib": rss, "median_throughput_change_percent": throughput,
                "other_budgets_met": rss <= (2048 if mode == "metrics" else 8192)
                    and throughput >= (-10 if mode == "metrics" else -15),
                "separate_high_cost_profile": mode == "full_sample"})
    # No automatic M4 acceptance: complete measurements still need final source,
    # loss/shutdown and applicability disposition. Retain absolute raw cells.
    return output


if __name__ == "__main__":
    import json
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    print(json.dumps(report(args.directory), indent=2, allow_nan=False))
