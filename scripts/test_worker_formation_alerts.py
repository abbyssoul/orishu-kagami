#!/usr/bin/env python3
"""Finite formation-alert fixtures evaluated by pinned promtool, not a mock.

Generate the repetitive metric/budget matrix as JSON (also valid YAML). The
checked-in rule file is the implementation under test; no PromQL is copied
into this fixture generator. This command downloads nothing.
"""

import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
PREFIX = "orishu_worker_"
RULES = ROOT / "etc/prometheus-worker-formation-alerts.yml"
ALERTS = {
    "OrishuWorkerOwnerUnresponsive": (
        "warning", "Reachable diagnostics report an unresponsive worker owner"),
    "OrishuWorkerAdmissionRefusals": (
        "info", "Worker recently refused admission; policy may be working as intended"),
    "OrishuWorkerCatchupTransferFailures": (
        "warning", "Worker recently failed admission-state transfer; inspect the original operation"),
    "OrishuWorkerMembershipAbandoned": (
        "warning", "Worker exhausted a join or reconciliation budget; outcome needs inspection"),
    "OrishuWorkerPeerTimeouts": (
        "warning", "Worker recently observed peer IO timeouts; this is not a peer-death count"),
    "OrishuWorkerCapacityExhausted": (
        "warning", "Worker has sustained full slot occupancy including reservations"),
}
COUNTERS = {
    "OrishuWorkerAdmissionRefusals": ["admissions_rejected_total"],
    "OrishuWorkerCatchupTransferFailures": [
        "catchup_transfer_" + event + "_total"
        for event in ("binding_rejected", "invalid", "rejected", "unavailable", "timed_out")],
    "OrishuWorkerMembershipAbandoned": [
        "membership_join_abandoned_total", "membership_anti_entropy_abandoned_total"],
    "OrishuWorkerPeerTimeouts": [
        "peer_" + stage + "_timed_out_total" for stage in (
            "reliable_request", "reliable_serve", "outbound_attempt", "outbound_tls",
            "inbound_tls", "inbound_handshake")],
}
BUDGETS = {
    "peer": 64, "control": 16, "completion": 64, "shutdown": 1,
    "peer_inbound_tls": 16, "peer_inbound_connection": 64,
    "peer_registry": 64, "peer_registry_provisional": 16,
    "peer_reliable": 64, "peer_outbound": 4,
}


def series(name, values, instance="worker", job="orishu-worker"):
    return {"series": f'{name}{{job="{job}",instance="{instance}"}}', "values": values}


def expected(alert, instance="worker", budget=None):
    severity, summary = ALERTS[alert]
    labels = {"job": "orishu-worker", "instance": instance, "severity": severity}
    if budget:
        labels["budget"] = budget
    return {"exp_labels": labels, "exp_annotations": {"summary": summary}}


def scenario(name, inputs, active=None):
    """Every scenario checks all six rules, before pending, firing and recovery."""
    active = active or {}
    checks = [{"eval_time": at, "alertname": alert,
               "exp_alerts": active.get(alert, []) if at == "4m" else []}
              for at in ("1m", "4m", "10m") for alert in ALERTS]
    return {"name": name, "interval": "1m", "input_series": inputs,
            "alert_rule_test": checks}


def fixtures(catalogue):
    # Catch invented/retired metric names even if a synthetic series would make
    # an otherwise valid PromQL expression pass. The same catalogue is checked
    # against actual worker exposition by the parent ingestion harness.
    counters = [name for names in COUNTERS.values() for name in names]
    metrics = counters + [budget + "_slots_" + field for budget in BUDGETS
                          for field in ("in_use", "capacity")] + ["owner_responsive"]
    assert {PREFIX + name for name in metrics} <= catalogue, "fixture metric absent from live catalogue"
    up = series("up", "1x10")
    tests = []
    for alert, names in COUNTERS.items():
        for name in names:
            tests.append(scenario(name, [up, series(PREFIX + name, "0+1x4 4x6")],
                                  {alert: [expected(alert)]}))
    owner = "OrishuWorkerOwnerUnresponsive"
    tests.append(scenario("owner-stall-recovery", [up, series(PREFIX + "owner_responsive", "0x4 1x6")],
                          {owner: [expected(owner)]}))
    capacity_alert = "OrishuWorkerCapacityExhausted"
    for budget, capacity in BUDGETS.items():
        tests.append(scenario(budget + "-capacity", [up,
            series(PREFIX + budget + "_slots_in_use", f"{capacity}x4 0x6"),
            series(PREFIX + budget + "_slots_capacity", f"{capacity}x10")],
            {capacity_alert: [expected(capacity_alert, budget=budget)]}))
    # Multiple budgets on one worker must match their own capacity, not each
    # other, and emit exactly ten bounded labels rather than a many-to-many join.
    all_budgets = [entry for budget, capacity in BUDGETS.items() for entry in (
        series(PREFIX + budget + "_slots_in_use", f"{capacity}x4 0x6"),
        series(PREFIX + budget + "_slots_capacity", f"{capacity}x10"))]
    tests.append(scenario("all-ten-budgets", [up] + all_budgets,
                          {capacity_alert: [expected(capacity_alert, budget=budget) for budget in BUDGETS]}))
    all_counters = [series(PREFIX + name, "0+1x4 4x6") for name in counters]
    tests.append(scenario("failed-scrape-suppresses-retained-symptoms",
                          [series("up", "0x10"), series(PREFIX + "owner_responsive", "0x10")]
                          + all_counters + all_budgets))
    tests.append(scenario("deleted-target-needs-inventory",
                          [series("up", "1x1 stale"), series(PREFIX + "owner_responsive", "0x10")]
                          + all_counters + all_budgets))
    tests.append(scenario("metric-absent-is-not-zero", [up]))
    tests.append(scenario("idle-counters", [up] + [series(PREFIX + name, "0x10") for name in counters]))
    tests.append(scenario("process-reset-without-new-errors", [up]
                          + [series(PREFIX + name, "9x2 0x8") for name in counters]))
    tests.append(scenario("transient-owner-stall", [up, series(PREFIX + "owner_responsive", "0 1x10")]))
    tests.append(scenario("stale-owner-is-not-measured-unresponsive", [up,
                          series(PREFIX + "owner_responsive", "0 stale")]))
    tests.append(scenario("zero-unavailable-and-missing-capacity", [up,
        series(PREFIX + "peer_slots_in_use", "0x10"),
        series(PREFIX + "peer_slots_capacity", "0x10"),
        series(PREFIX + "control_slots_in_use", "16x10"),
        series(PREFIX + "completion_slots_capacity", "64x10"),
        series(PREFIX + "shutdown_slots_in_use", "1 stale"),
        series(PREFIX + "shutdown_slots_capacity", "1x10"),
        series(PREFIX + "unknown_slots_in_use", "1x10"),
        series(PREFIX + "unknown_slots_capacity", "1x10")]))
    tests.append(scenario("transient-and-below-capacity", [up] + [entry
        for budget, capacity in BUDGETS.items() for entry in (
            series(PREFIX + budget + "_slots_in_use", f"{capacity} {capacity - 1}x10"),
            series(PREFIX + budget + "_slots_capacity", f"{capacity}x10"))]))
    excluded = ["admissions_accepted_total", "admission_assignment_replays_total",
                "catchup_transfer_cancelled_total", "catchup_transfer_validated_total",
                "catchup_owner_adopted_total", "catchup_owner_fenced_total",
                "catchup_owner_not_adopted_total", "catchup_owner_abandoned_total",
                "membership_direct_probe_deadlines_total", "membership_suspicion_deadlines_total",
                "membership_stale_timer_inputs_total", "peer_outbound_attempt_cancelled_total",
                "peer_inbound_tls_cancelled_total", "peer_reliable_request_cancelled_total"]
    assert {PREFIX + name for name in excluded} <= catalogue
    tests.append(scenario("activity-cancellation-fencing-and-core-timers-are-distinct", [up]
                          + [series(PREFIX + name, "0+1x10") for name in excluded]))
    # Rate before combining conditions: resetting one counter must not hide
    # another's new failures, and all counters coexist on the same target.
    for alert in ("OrishuWorkerCatchupTransferFailures", "OrishuWorkerPeerTimeouts"):
        names = COUNTERS[alert]
        tests.append(scenario(alert + "-reset-does-not-hide-other-errors", [up,
            series(PREFIX + names[0], "50x2 0x8"), series(PREFIX + names[1], "0+1x4 4x6")],
            {alert: [expected(alert)]}))
    tests.append(scenario("workers-and-jobs-do-not-share-symptoms", [up,
        series("up", "1x10", instance="other"),
        series("up", "1x10", job="not-orishu"),
        series(PREFIX + "admissions_rejected_total", "0+1x4 4x6"),
        series(PREFIX + "admissions_rejected_total", "0x10", instance="other"),
        series(PREFIX + "owner_responsive", "0x4 1x6", job="not-orishu")],
        {"OrishuWorkerAdmissionRefusals": [expected("OrishuWorkerAdmissionRefusals")]}))
    return {"rule_files": [str(RULES)], "evaluation_interval": "1m", "tests": tests}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--promtool", required=True)
    args = parser.parse_args()
    tool = Path(shutil.which(args.promtool) or args.promtool).resolve()
    version = subprocess.run([str(tool), "--version"], capture_output=True, text=True, check=True, timeout=10)
    assert "version 3.5.0 " in version.stdout + version.stderr, "use the pinned promtool 3.5.0"
    spec = importlib.util.spec_from_file_location("worker_prometheus", ROOT / "scripts/check-worker-prometheus.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    suite = fixtures(module.NAMES)
    with tempfile.TemporaryDirectory(prefix="orishu-formation-alerts-") as temporary:
        path = Path(temporary) / "formation.test.yml"
        path.write_text(json.dumps(suite), encoding="utf-8")
        subprocess.run([str(tool), "check", "rules", str(RULES)], check=True, timeout=10)
        subprocess.run([str(tool), "test", "rules", str(path)], check=True, timeout=10)
    print(f"PASS: {len(ALERTS)} formation rules; {len(suite['tests'])} finite scenarios; pending/firing/recovery and exclusions")


if __name__ == "__main__":
    main()
