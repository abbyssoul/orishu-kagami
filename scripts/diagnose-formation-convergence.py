#!/usr/bin/env python3
"""Four bounded formation diagnostics isolating repeated CLI startup cost.

CLI/persistent/persistent/CLI order, one verification sweep/second in both
variants. No load, acceptance retry, worker change or setup-deadline extension.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import signal
import selectors
import subprocess
import sys
import tempfile
import time

_spec = importlib.util.spec_from_file_location(
    "formation_curve", Path(__file__).with_name("measure-formation-telemetry.py"))
curve = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(curve)

STAGES = ("exact-members", "formation-views", "lock", "unlock")
ORDER = ("cli", "persistent", "persistent", "cli")
METRIC_SUFFIXES = (
    "swim_packets_received_total", "anti_entropy_packets_received_total", "gossip_items_received_total",
    "send_failures_total", "peer_decode_rejections_total", "core_diagnostics_total",
    "peer_registry_slots_in_use", "peer_registry_provisional_slots_in_use",
    "membership_direct_probe_deadlines_total", "membership_indirect_probe_deadlines_total",
    "membership_suspicion_deadlines_total", "membership_anti_entropy_deadlines_total",
    "peer_outbound_attempt_completed_total", "peer_outbound_attempt_failed_total",
    "peer_outbound_attempt_timed_out_total", "peer_outbound_tls_failed_total",
    "peer_outbound_capacity_refused_total", "peer_inbound_handshake_completed_total",
    "peer_inbound_handshake_failed_total", "peer_inbound_handshake_timed_out_total",
    "peer_datagrams_submitted_total", "peer_datagrams_submit_refused_total",
    "peer_datagrams_submit_failed_total", "peer_datagrams_received_total",
)


def observer_reply(stream, timeout=3, cap=1_048_576):
    """One outstanding query: bounded chunk reads, never one syscall per byte."""
    deadline = time.monotonic() + timeout
    result = bytearray()
    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_READ)
        while len(result) <= cap:
            curve.require(selector.select(max(0, deadline - time.monotonic())), "observer output deadline")
            chunk = os.read(stream.fileno(), min(65536, cap + 1 - len(result)))
            curve.require(chunk, "unexpected observer EOF")
            result.extend(chunk)
            if b"\n" in chunk:
                curve.require(result.endswith(b"\n") and result.count(b"\n") == 1, "extra observer output")
                curve.require(len(result) - 1 <= cap, "observer reply byte cap")
                return bytes(result[:-1])
    raise ValueError("observer reply byte cap")


class ObservedCell(curve.Cell):
    observer = None

    def cli(self, role, arguments):
        stage = self.row.get("formation_stage")
        observed = stage in STAGES and arguments in (["ls"], ["cluster", "info"])
        began = time.monotonic()
        if observed and self.row["observer"] == "persistent":
            self.commands += 1
            curve.require(self.commands <= 4096, "cell operator command cap")
            if self.observer is None:
                self.observer = self.spawn(
                    [str(self.args.observer), "observe", str(self.root), str(self.count)],
                    stdout=subprocess.PIPE, stdin=subprocess.PIPE)
                curve.require(curve.bounded_line(self.observer.stdout, 2) == b"READY", "observer startup")
            query = {"role": role, "operation": "members" if arguments == ["ls"] else "summary"}
            self.observer.stdin.write(json.dumps(query).encode() + b"\n")
            self.observer.stdin.flush()
            value = json.loads(observer_reply(self.observer.stdout), object_pairs_hook=curve.unique)
            if arguments == ["cluster", "info"]:
                # The typed protocol and CLI JSON intentionally have distinct
                # projections. Rename only the three CLI presentation fields.
                for wire, cli in (("memberCount", "nodes"), ("aliveCount", "alive"), ("membershipLocked", "locked")):
                    value[cli] = value.pop(wire)
        else:
            value = super().cli(role, arguments)
        if observed:
            records = self.row.setdefault("read_observations", [])
            curve.require(len(records) < 4096, "read observation cap")
            record = {"stage": stage, "role": role, "start_seconds": began - self.started,
                      "end_seconds": time.monotonic() - self.started}
            if arguments == ["ls"]:
                record["members"] = len(value)
                record["alive"] = sum(node["liveness"] == "alive" for node in value)
                known = {node["nodeId"] for node in value}
                record["missing_roles"] = [index for index, node in enumerate(self.assigned) if node not in known]
            else:
                record["locked"] = value["locked"]
            records.append(record)
        return value

    def poll(self, check, setup=True, *, convergence=False):
        if self.row.get("formation_stage") not in STAGES:
            return super().poll(check, setup, convergence=convergence)
        next_check = time.monotonic()

        def paced():
            nonlocal next_check
            time.sleep(max(0, next_check - time.monotonic()))
            next_check = time.monotonic() + 1
            result = check()
            if getattr(self.args, "metrics", False):
                samples = self.row.setdefault("metric_observations", [])
                curve.require(len(samples) < 240, "metric observation cap")
                for role in (0, 15, 29):
                    snapshot = curve.metrics(self.ports[role])
                    samples.append({"role": role, "seconds": time.monotonic() - self.started,
                                    "stage": self.row["formation_stage"],
                                    "values": {suffix: snapshot["orishu_worker_" + suffix] for suffix in METRIC_SUFFIXES}})
            return result

        return super().poll(paced, setup, convergence=convergence)

    def close_observer(self):
        if self.observer is not None:
            self.observer.stdin.close()
            self.row["observer_exit"] = self.observer.wait(timeout=2)
            curve.require(self.row["observer_exit"] == 0, "observer did not exit cleanly")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "ctl", "probe", "observer", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--case-index", type=int, choices=range(len(ORDER)),
                        help="one separately labeled development verification, not a resumed series")
    parser.add_argument("--metrics", action="store_true",
                        help="diagnostic-only metrics on roles 0/15/29; requires the combined worker")
    args = parser.parse_args()
    indices = list(range(len(ORDER))) if args.case_index is None else [args.case_index]
    budget = 90 * len(indices)
    curve.require(sys.platform == "linux" and not curve.competing_jobs(), "Linux quiet fixture required")
    os.umask(0o077)
    args.output.mkdir(mode=0o700, exist_ok=False)
    artifacts = {}
    for name in ("worker", "ctl", "probe", "observer"):
        path = getattr(args, name).resolve(strict=True)
        setattr(args, name, path)
        artifacts[name] = {"path": str(path), "sha256": curve.sha(path)}
    args.omitted = args.worker
    source = curve.source_identity()
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(("ORISHU_", "OTEL_")) and k != "TOKIO_WORKER_THREADS"}
    manifest = {"schema_version": 1, "kind": "formation-observer-comparison", "acceptance_run": False,
                "order": ORDER, "workers": 30, "sweep_period_seconds": 1, "setup_budget_seconds": 60,
                "case_indices": indices, "cell_budget_seconds": 90, "batch_budget_seconds": budget,
                "metrics_roles": [0, 15, 29] if args.metrics else [],
                "source": source, "artifacts": artifacts}
    curve.write_json(args.output / "manifest.json", manifest)
    started = time.monotonic()
    for index in indices:
        observer = ORDER[index]
        row = {"schema_version": 1, "index": index, "observer": observer, "status": "failed"}
        cell = ObservedCell(args, 30, "metrics" if args.metrics else "omitted", environment, row)
        signal.setitimer(signal.ITIMER_REAL, max(0.01, min(80, budget - 10 - (time.monotonic() - started))))
        try:
            with tempfile.TemporaryDirectory(prefix="ofc-") as temporary:
                cell.root = Path(temporary)
                try:
                    cell.form()
                    row["status"] = "complete"
                finally:
                    signal.setitimer(signal.ITIMER_REAL, 0)
                    try:
                        cell.close_observer()
                    finally:
                        row["cleanup_clean"] = curve.stop(cell.children)
                        row["logs"] = [log.finish() for log in cell.logs]
        except (Exception, KeyboardInterrupt) as error:
            row["status"] = "failed"
            row["error_type"] = type(error).__name__
            row["error"] = str(error)[:256] if isinstance(error, (ValueError, TimeoutError)) else "fixture failure"
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            row["elapsed_seconds"] = time.monotonic() - cell.started
            row["source_unchanged"] = curve.source_identity() == source
            row["artifacts_unchanged"] = all(curve.sha(Path(v["path"])) == v["sha256"] for v in artifacts.values())
            curve.write_json(args.output / f"cell-{index}.json", row)
        print(index, observer, row["status"], row["elapsed_seconds"], row.get("error"), flush=True)
        curve.require(row.get("cleanup_clean") and row["source_unchanged"] and row["artifacts_unchanged"],
                      "unclean fixture or changed checkpoint; stop diagnostic series")
        curve.require(time.monotonic() - started < budget, "diagnostic batch budget")
    curve.write_json(args.output / "finished.json", {"elapsed_seconds": time.monotonic() - started,
                                                     "note": "diagnostic only; inspect every cell outcome"})


if __name__ == "__main__":
    def expired(_signal, _frame):
        raise TimeoutError("convergence diagnostic deadline")
    signal.signal(signal.SIGALRM, expired)
    main()
