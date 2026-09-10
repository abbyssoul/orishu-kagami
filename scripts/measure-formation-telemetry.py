#!/usr/bin/env python3
"""Approved 3/10/30-worker telemetry curve; Linux, isolated source-built binaries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import time

from formation_telemetry import (CLK_TCK, MODES, SIZES, LogDrain, Scraper, bounded_line,
                                metrics, proc, read_json, require, unique, write_json)
from worker_formation_receipts import collector as recipe, join_phase_complete

ROOT = Path(__file__).resolve().parents[1]
PROFILE = {"schema_version": 2, "sizes": list(SIZES), "rounds": 6, "modes": list(MODES),
           "clients_per_worker": 2, "warmup_per_client": 64, "window_seconds": 10,
           "sample_cap_per_client": 500000, "scrape_ms": 250, "scrape_cap_bytes": 32768,
           "proc_sample_ms": 50, "size_budget_seconds": 1800, "batch_budget_seconds": 5400,
           "setup_budget_seconds": 60, "cell_budget_seconds": 180,
           "observation_budget_seconds": 10,
           "convergence_budget_policy": "remaining_whole_setup_v1",
           "operator_list_cap_bytes": 65536, "operator_reply_cap_bytes": 16384,
           "trace_queue": 1024, "trace_active": 128, "trace_batch": 128,
           "trace_flush_ms": 1000, "trace_attempt_ms": 2000, "trace_shutdown_ms": 3000,
           "trace_export_bytes": 1048576, "trace_response_bytes": 16384,
           "log_queue": 256, "log_shutdown_ms": 250,
           "log_sink": "continuously drained stdout; bounded Python record validation",
           "normal_p95_cpu_percent_exclusive": 10, "temporary_percent_inclusive": 20,
           "troubleshooting_above_percent": 25}


def sha(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def source_identity():
    paths = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT).split(b"\0")
    files = {os.fsdecode(path): sha(ROOT / os.fsdecode(path)) for path in paths if path and (ROOT / os.fsdecode(path)).is_file()}
    return {"head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
            "files": files}


def host_snapshot():
    result = {"load_average": os.getloadavg()}
    for name, path, cap in (("memory", "/proc/meminfo", 8192),
                            ("vmstat", "/proc/vmstat", 16384),
                            ("cpu_stat", "/proc/stat", 16384),
                            ("cpu_pressure", "/proc/pressure/cpu", 2048),
                            ("memory_pressure", "/proc/pressure/memory", 2048),
                            ("scaling_policy", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor", 128)):
        try:
            with open(path) as source:
                text = source.read(cap + 1)
            result[name] = text if len(text) <= cap else {"unavailable": "byte_cap"}
        except OSError:
            result[name] = {"unavailable": "not_readable"}
    return result


def competing_jobs():
    """Names only: do not collect arguments that might contain credentials."""
    raw = subprocess.check_output(["ps", "-eo", "comm="], timeout=2)
    require(len(raw) <= 65536, "process inventory byte cap")
    return sorted({name for name in raw.decode().splitlines()
                   if name in ("cargo", "rustc", "rust-lld")
                   or name.startswith(("standalone-", "orishu_worker-", "orishu-worker-t"))})


def stop(children):
    """Terminate only this cell's exact children, with one shared grace budget."""
    for child in children:
        if child.poll() is None:
            child.terminate()
    deadline = time.monotonic() + 8
    clean = True
    for child in children:
        try:
            clean &= child.wait(timeout=max(0.01, deadline - time.monotonic())) == 0
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=2)
            clean = False
    return clean


class Cell:
    def __init__(self, args, count, mode, environment, row):
        self.args, self.count, self.mode, self.environment, self.row = args, count, mode, environment, row
        self.children, self.workers, self.logs = [], [], []
        self.ports = []
        self.started = time.monotonic()
        self.commands = 0

    def spawn(self, command, **options):
        child = subprocess.Popen(command, env=self.environment, stderr=subprocess.DEVNULL,
                                 bufsize=0, umask=0o077, **options)
        self.children.append(child)
        return child

    def cli(self, role, arguments):
        self.commands += 1
        require(self.commands <= 4096, "cell operator command cap")
        base = self.root / str(role)
        command = [str(self.args.ctl), "--host", str(base / "api.sock"), "--output", "json",
                   "--timeout", "2s", "--operator-token-file", str(base / "state/operator.token")]
        cap = PROFILE["operator_list_cap_bytes" if arguments == ["ls"] else "operator_reply_cap_bytes"]
        return json.loads(recipe.run(command + arguments, self.environment, max_output_bytes=cap), object_pairs_hook=unique)

    def poll(self, check, setup=True, *, convergence=False):
        require(not convergence or setup, "convergence requires the original setup deadline")
        deadline = time.monotonic() + PROFILE["observation_budget_seconds"]
        if setup:
            setup_deadline = self.started + PROFILE["setup_budget_seconds"]
            deadline = setup_deadline if convergence else min(deadline, setup_deadline)
        while time.monotonic() < deadline:
            require(all(child.poll() is None for child in self.children), "cell child exited")
            result = check()
            if result:
                # A slow operator reply cannot turn a late observation into a
                # pass. Every convergence stage shares the original deadline.
                if time.monotonic() >= deadline:
                    break
                return result
            time.sleep(0.05)
        raise TimeoutError(f"formation observation/setup deadline: {self.row.get('formation_stage', 'unspecified')}")

    def views(self, locked=False):
        views = [self.cli(role, ["cluster", "info"]) for role in range(self.count)]
        self.row["last_formation_views"] = [{key: view[key] for key in
            ("formationId", "sourceNodeId", "nodes", "alive", "introducerReady", "locked", "participation")}
            for view in views]
        for role, view in enumerate(views):
            require(view["formationId"] == self.formation and view["sourceNodeId"] == self.assigned[role], "formation/source mismatch")
        return all(view["nodes"] == view["alive"] == self.count and view["introducerReady"]
                   and view["locked"] is locked for view in views)

    def exact_members(self):
        views = [self.cli(role, ["ls"]) for role in range(self.count)]
        self.row["last_members"] = [[{key: node[key] for key in ("nodeId", "certFingerprint", "liveness")}
                                    for node in view[:31]] for view in views]
        return all({node["nodeId"]: node["certFingerprint"] for node in view} == self.expected
                   and len(view) == self.count and all(node["liveness"] == "alive" for node in view)
                   for view in views)

    def await_join(self, role, operation, initial):
        self.row["formation_stage"] = f"join-{role}"
        assigned = None

        def joined():
            nonlocal assigned
            status = self.cli(role, ["join-status", operation])
            require(status["sourceFormationId"] == initial["formationId"]
                    and status["sourceNodeId"] == initial["sourceNodeId"]
                    and status["targetFormationId"] == self.formation, "join status identity mismatch")
            phase = status["state"]["phase"]
            # This is the worker's bounded automatic retry, not a new admission
            # or a verifier retry. Exhaustion still fails the original deadline.
            complete = join_phase_complete(phase)
            self.row.setdefault("join_phases", {})[str(role)] = phase
            if phase in ("catchingUp", "catchUpFailed", "joined"):
                node = status["state"]["nodeId"]
                require(node != initial["sourceNodeId"] and (assigned is None or node == assigned),
                        "catch-up changed assigned identity")
                assigned = node
            return status if complete else None

        return self.poll(joined)

    def form(self):
        self.collector = self.spawn([str(self.args.probe), "collect", str(self.count)], stdout=subprocess.PIPE)
        receiver = int(bounded_line(self.collector.stdout, 5))
        diagnostic = self.mode not in MODES[:2]
        tracing = self.mode in MODES[3:]
        sampling = {"default_sample": "1000", "full_sample": "1000000"}.get(self.mode, "0")
        for role in range(self.count):
            base = self.root / str(role)
            base.mkdir(mode=0o700)
            port = recipe.port()
            require(port not in self.ports and port != receiver, "diagnostic port reservation collision")
            self.ports.append(port)
            command = [str(self.args.omitted if self.mode == "omitted" else self.args.worker),
                       "--state-dir", str(base / "state"), "--listen.clients", str(base / "api.sock"),
                       "--listen.peers", "127.0.0.1:0", "--accepts.peers", "true",
                       "--observability.enabled", str(diagnostic).lower(), "--observability.bind", f"127.0.0.1:{port}",
                       "--tracing.enabled", str(tracing).lower(), "--tracing.sample-ppm", sampling,
                       "--tracing.endpoint", f"http://127.0.0.1:{receiver}/v1/traces/{role}",
                       "--tracing.queue-capacity", "1024", "--tracing.active-span-capacity", "128",
                       "--tracing.batch-size", "128", "--tracing.flush-interval-ms", "1000",
                       "--tracing.export-timeout-ms", "2000", "--tracing.shutdown-timeout-ms", "3000",
                       "--tracing.export-max-bytes", "1048576", "--tracing.response-max-bytes", "16384",
                       "--logging.enabled", str(tracing).lower(), "--logging.queue-records", "256",
                       "--logging.shutdown-ms", "250"]
            child = self.spawn(command, stdout=subprocess.PIPE)
            self.workers.append(child)
            self.logs.append(LogDrain(child.stdout))
            self.row["formation_stage"] = f"socket-{role}"
            self.poll(lambda: (base / "api.sock").exists())
        self.row["formation_stage"] = "initial-identities"
        initial = [self.cli(role, ["cluster", "info"]) for role in range(self.count)]
        require(len({view["formationId"] for view in initial}) == self.count, "initial formations not distinct")
        pins = [self.cli(role, ["inspect", view["sourceNodeId"]])["certFingerprint"] for role, view in enumerate(initial)]
        self.formation = initial[0]["formationId"]
        self.assigned = [initial[0]["sourceNodeId"]]
        joins = []
        for role in range(1, self.count):
            self.row["formation_stage"] = f"submit-join-{role}"
            began = time.monotonic()
            material = self.cli(role - 1, ["token"])
            require(material["introducerReady"] and material["formationId"] == self.formation
                    and material["introducerNodeId"] == self.assigned[-1]
                    and material["introducerFingerprint"] == pins[role - 1], "introducer material mismatch")
            path = self.root / f"join-{role}.json"
            write_json(path, material)
            operation = f"measure-admit-{role}"
            self.cli(role, ["join", "--join-material-file", str(path), "--formation-id", initial[role]["formationId"], "--operation-id", operation])

            status = self.await_join(role, operation, initial[role])
            self.assigned.append(status["state"]["nodeId"])
            require(self.assigned[-1] != initial[role]["sourceNodeId"], "join retained standalone identity")
            self.row["formation_stage"] = f"introducer-ready-{role}"
            self.poll(lambda: self.cli(role, ["cluster", "info"])["introducerReady"])
            joins.append(time.monotonic() - began)
        self.expected = dict(zip(self.assigned, pins))
        require(len(self.expected) == self.count, "duplicate assigned identity")
        self.row["formation_stage"] = "exact-members"
        self.poll(self.exact_members, convergence=True)
        self.row["formation_stage"] = "formation-views"
        self.poll(self.views, convergence=True)
        policy = []
        for locked, role in ((True, 0), (False, self.count - 1)):
            self.row["formation_stage"] = "lock" if locked else "unlock"
            began = time.monotonic()
            receipt = self.cli(role, ["cluster", "lock" if locked else "unlock", "--formation-id", self.formation,
                                      "--operation-id", "measure-lock" if locked else "measure-unlock"])
            require(receipt["locked"] is locked, "policy not accepted")
            self.poll(lambda: self.views(locked), convergence=True)
            policy.append(time.monotonic() - began)
        self.row.update(setup_seconds=time.monotonic() - self.started, joins_seconds=joins,
                        policy_seconds=policy, formation=self.formation, nodes=self.assigned)
        require(self.row["setup_seconds"] <= PROFILE["setup_budget_seconds"], "whole setup budget exceeded")

    def measure(self):
        config = self.root / "load.json"
        write_json(config, [{"socket": str(self.root / str(role) / "api.sock"), "formation": self.formation, "node": node}
                            for role, node in enumerate(self.assigned)])
        load = self.spawn([str(self.args.probe), "load", str(config)], stdout=subprocess.PIPE, stdin=subprocess.PIPE)
        require(bounded_line(load.stdout, 15) == b"READY", "load warmup marker")
        require(self.exact_members() and self.views(), "pre-window formation mismatch")
        diagnostic = self.mode not in MODES[:2]
        before_metrics = [metrics(port) for port in self.ports] if diagnostic else None
        self.collector.send_signal(signal.SIGUSR1)
        self.row["collector_before"] = json.loads(bounded_line(self.collector.stdout, 2), object_pairs_hook=unique)
        self.row["logs_before"] = [{"counts": dict(log.counts), "bytes": log.bytes, "errors": log.errors} for log in self.logs]
        pids = [worker.pid for worker in self.workers] + [self.collector.pid, load.pid, os.getpid()]
        before = [proc(pid) for pid in pids]
        peaks = [row["rss_kib"] for row in before]
        swaps = [row["swap_kib"] for row in before]
        scrapers = [Scraper(port, 0) for port in self.ports] if diagnostic else []
        started = time.monotonic()
        for scraper in scrapers:
            scraper.start = started
            scraper.thread.start()
        load.stdin.write(b"G")
        samples, missed = 0, 0
        next_sample = started
        while time.monotonic() < started + 10:
            current = [proc(pid) for pid in pids]
            for index, row in enumerate(current):
                peaks[index] = max(peaks[index], row["rss_kib"])
                swaps[index] = max(swaps[index], row["swap_kib"])
            samples += 1
            next_sample += 0.05
            now = time.monotonic()
            if next_sample < now:
                skipped = int((now - next_sample) / 0.05) + 1
                missed += skipped
                next_sample += skipped * 0.05
            time.sleep(max(0, min(started + 10, next_sample) - time.monotonic()))
        require(bounded_line(load.stdout, 2) == b"END", "load end marker")
        after = [proc(pid) for pid in pids]
        bracket = time.monotonic() - started
        self.row["logs_after"] = [{"counts": dict(log.counts), "bytes": log.bytes, "errors": log.errors} for log in self.logs]
        self.collector.send_signal(signal.SIGUSR1)
        self.row["collector_after"] = json.loads(bounded_line(self.collector.stdout, 2), object_pairs_hook=unique)
        load.stdin.write(b"A")
        result = json.loads(bounded_line(load.stdout, 5), object_pairs_hook=unique)
        load.stdin.close()
        require(load.wait(timeout=2) == 0, "load exit")
        self.children.remove(load)
        scrape_results = [scraper.finish() for scraper in scrapers]
        after_metrics = [metrics(port) for port in self.ports] if diagnostic else None
        resources = [{"cpu_ticks": end["ticks"] - begin["ticks"],
                      "cpu_seconds": (end["ticks"] - begin["ticks"]) / CLK_TCK,
                      "rss_peak_kib": max(peak, end["rss_kib"]),
                      "setup_hwm_kib": begin["hwm_kib"], "lifetime_hwm_kib": end["hwm_kib"],
                      "swap_peak_kib": max(swap, end["swap_kib"])}
                     for begin, end, peak, swap in zip(before, after, peaks, swaps)]
        self.row.update(load=result, resources=resources[:self.count],
                        tooling=dict(zip(("collector", "load_generator", "runner_and_log_sink"), resources[self.count:])),
                        cpu_bracket_seconds=bracket, proc_samples=samples, proc_missed=missed,
                        scrapes=scrape_results, metrics_before=before_metrics, metrics_after=after_metrics)
        require(self.exact_members() and self.views(), "post-window formation mismatch")
        require(result["schema_version"] == 2 and result["seconds"] == 10
                and len(result["workers"]) == self.count, "load report identity")
        require(all(not row["capped"] and row["latency"]["requests"] > 0 for row in result["workers"]), "load sample cap or empty client")

    def run(self, root):
        self.root = root
        try:
            self.form()
            self.row["phase"] = "measurement"
            self.measure()
            self.row["phase"] = "shutdown"
            require(stop(self.workers), "worker shutdown deadline/exit")
            self.row["logs"] = [log.finish() for log in self.logs]
            self.collector.terminate()
            self.row["collector"] = json.loads(bounded_line(self.collector.stdout, 5), object_pairs_hook=unique)
            require(self.collector.wait(timeout=2) == 0, "collector shutdown exit")
            self.row["status"] = "complete"
        finally:
            # Cancellation cannot leave a worker/collector behind. This is normal
            # fixture cleanup, never an extra successful-delivery retry budget.
            self.row["cleanup_clean"] = stop(self.children)
            partial = []
            for log in self.logs:
                partial.append(log.finish())
            self.row.setdefault("logs", partial)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "omitted", "ctl", "probe", "otelcol"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--smoke", action="store_true", help="one three-worker six-mode round; not acceptance")
    args = parser.parse_args()
    require(sys.platform == "linux", "Linux /proc experiment required")
    require(not competing_jobs(), "competing build/test process; wait for a quiet host (do not stop unrelated jobs)")
    os.umask(0o077)
    args.output.mkdir(mode=0o700, parents=False, exist_ok=False)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("ORISHU_", "OTEL_")) and key not in ("TOKIO_WORKER_THREADS",)}
    artifacts = {}
    for name in ("worker", "omitted", "ctl", "probe", "otelcol"):
        source = getattr(args, name).resolve(strict=True)
        target = args.output / name
        shutil.copyfile(source, target)
        target.chmod(0o700)
        require(sha(source) == sha(target), "artifact changed during snapshot")
        artifacts[name] = {"source": str(source), "sha256": sha(target)}
        setattr(args, name, target.resolve())
    manifest = {"schema_version": 2, "kind": "formation-telemetry", "acceptance_run": not args.smoke,
                "profile": PROFILE, "artifacts": artifacts, "source": source_identity(),
                "plan_sha256": sha(ROOT / "docs/measurements/formation-telemetry-plan.md"),
                "command": sys.argv, "host": {"platform": platform.platform(), "cpu_count": os.cpu_count(),
                "affinity": sorted(os.sched_getaffinity(0)), "clk_tck": CLK_TCK,
                "load_average": os.getloadavg(), "cpuinfo": Path("/proc/cpuinfo").read_text()[:16384],
                "memory": Path("/proc/meminfo").read_text()[:8192],
                "cgroup": Path("/proc/self/cgroup").read_text()[:4096]}}
    manifest["toolchain"] = subprocess.check_output(["rustc", "-Vv"], cwd=ROOT, timeout=10).decode()
    manifest["build_commands"] = [
        "cargo build --locked --offline --release -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --bins --example formation-telemetry-probe --target-dir target/formation-telemetry-enabled",
        "cargo build --locked --offline --release -p orishu-worker -p orishuctl --target-dir target/formation-telemetry-omitted"]
    manifest["build_commands_note"] = "repository recipe; executable hashes identify actual supplied artifacts"
    write_json(args.output / "manifest.json", manifest)
    manifest_hash = sha(args.output / "manifest.json")
    began = time.monotonic()
    # Actual causal ID/log receipt uses the established official-Collector
    # preflight, outside timed cells. Its batch=1/full-sampling settings are
    # functional evidence, not substituted performance cells.
    preflight = subprocess.run([sys.executable, str(ROOT / "scripts/check-worker-formation-otelcol.py"),
                               "--worker", str(args.worker), "--ctl", str(args.ctl), "--otelcol", str(args.otelcol)],
                              env=environment, timeout=130, check=False, capture_output=True)
    require(len(preflight.stdout) + len(preflight.stderr) <= 65536, "preflight output cap")
    write_json(args.output / "preflight.json", {"exit": preflight.returncode,
               "stdout": preflight.stdout.decode(errors="replace"),
               "stderr_sha256": hashlib.sha256(preflight.stderr).hexdigest(),
               "last_error_line": preflight.stderr.decode(errors="replace").splitlines()[-1:],
               "traceback_locations": [{"file": Path(path).name, "line": int(line), "function": function}
                   for path, line, function in re.findall(r'File "([^"]+)", line ([0-9]+), in ([^\n]+)', preflight.stderr.decode(errors="replace"))]})
    require(preflight.returncode == 0, "causal receipt preflight failed; no measurement")
    index = 0
    for count in ((3,) if args.smoke else SIZES):
        size_started = time.monotonic()
        for round_ in range(1 if args.smoke else 6):
            for offset in range(6):
                mode = MODES[(round_ + offset) % 6]
                remaining = min(5400 - (time.monotonic() - began), 1800 - (time.monotonic() - size_started))
                if remaining <= 0:
                    print(f"INCOMPLETE: deadline before {count}/{round_}/{mode}", flush=True)
                    return 2
                if competing_jobs():
                    write_json(args.output / "interrupted.json", {"reason": "competing_build_or_test", "before_cell": index})
                    print("INCOMPLETE: another build/test started; retained completed cells", flush=True)
                    return 2
                row = {"schema_version": 2, "manifest_sha256": manifest_hash, "index": index,
                       "worker_count": count, "round": round_, "mode": mode, "status": "failed", "phase": "formation",
                       "host_before": host_snapshot()}
                print(f"CELL {index + 1}: workers={count} round={round_ + 1} mode={mode}", flush=True)
                try:
                    signal.setitimer(signal.ITIMER_REAL, min(180, remaining))
                    with tempfile.TemporaryDirectory(prefix="ofm-") as temporary:
                        Cell(args, count, mode, environment, row).run(Path(temporary))
                except (Exception, KeyboardInterrupt) as error:
                    row["error_type"] = type(error).__name__
                    # Fixed harness errors only; never dump credential-bearing IO.
                    row["error"] = str(error)[:256] if isinstance(error, (ValueError, TimeoutError)) else "cell failed; inspect bounded phase evidence"
                    write_json(args.output / f"cell-{index:03}.json", row)
                    print(f"FAILED: {row['phase']}: {row['error']}", flush=True)
                    return 2
                finally:
                    signal.setitimer(signal.ITIMER_REAL, 0)
                    row["host_after"] = host_snapshot()
                write_json(args.output / f"cell-{index:03}.json", row)
                print(f"COMPLETE: {count}/{round_ + 1}/{mode}", flush=True)
                index += 1
    write_json(args.output / "finished.json", {"schema_version": 2, "cells": index,
               "elapsed_seconds": time.monotonic() - began, "source_unchanged": source_identity() == manifest["source"]})
    return 0


if __name__ == "__main__":
    def expired(_signal, _frame):
        raise TimeoutError("measurement cell/batch deadline")
    signal.signal(signal.SIGALRM, expired)
    raise SystemExit(main())
