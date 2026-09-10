#!/usr/bin/env python3
"""One five-minute, six-cell baseline diagnostic; never overhead acceptance.

Uses the curve's actual formation/load/cleanup path, with read-only Linux
observations. No retries, conditioning, affinity or host-policy changes.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import signal
import sys
import tempfile
import threading
import time

_spec = importlib.util.spec_from_file_location(
    "formation_curve", Path(__file__).with_name("measure-formation-telemetry.py"))
curve = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(curve)

PROFILE = {"cells": 6, "workers": 3, "mode": "compiled_off", "budget_seconds": 300,
           "cleanup_reserve_seconds": 15, "sample_seconds": 0.5,
           "sample_cap_per_cell": 48, "threads_per_process_cap": 128,
           "process_cap": 6, "sensor_cap": 128,
           "note": "sampled cpufreq is not measured effective frequency; RSS is not allocation count"}


def bounded_text(path, cap=4096):
    try:
        with Path(path).open() as source:
            value = source.read(cap + 1)
        curve.require(len(value) <= cap, "observation byte cap")
        return value.strip()
    except OSError:
        return None


def thread_stat(text):
    """Linux stat fields, including command names containing spaces/parentheses."""
    fields = text.rsplit(")", 1)[1].split()
    curve.require(len(fields) >= 37, "short thread stat")
    return {"state": fields[0], "minor_faults": int(fields[7]),
            "major_faults": int(fields[9]), "user_ticks": int(fields[11]),
            "system_ticks": int(fields[12]), "start_ticks": int(fields[19]),
            "last_cpu": int(fields[36])}


def process_snapshot(pid):
    tasks = sorted(Path(f"/proc/{pid}/task").iterdir(), key=lambda path: int(path.name))
    curve.require(len(tasks) <= PROFILE["threads_per_process_cap"], "thread inventory cap")
    threads = {}
    for task in tasks:
        raw = bounded_text(task / "stat")
        if raw is None:  # A short-lived thread may exit during this observation.
            continue
        row = thread_stat(raw)
        row["schedstat"] = bounded_text(task / "schedstat", 256)
        status = bounded_text(task / "status", 8192)
        row["switches"] = [line for line in (status or "").splitlines()
                           if line.startswith(("voluntary_ctxt_switches:", "nonvoluntary_ctxt_switches:"))]
        threads[task.name] = row
    return {"pid": pid, "memory": curve.proc(pid), "threads": threads}


def sensors():
    result = []
    for pattern in ("/sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq",
                    "/sys/devices/system/cpu/cpu[0-9]*/thermal_throttle/*throttle_count",
                    "/sys/class/thermal/thermal_zone*/temp",
                    "/sys/class/hwmon/hwmon*/temp*_input"):
        import glob
        result.extend(glob.glob(pattern))
    curve.require(len(result) <= PROFILE["sensor_cap"], "sensor inventory cap")
    return sorted(set(result))


class ObservedCell(curve.Cell):
    def measure(self):
        ended = threading.Event()
        errors = []
        paths = sensors()

        def observe():
            began = time.monotonic()
            try:
                for index in range(PROFILE["sample_cap_per_cell"]):
                    tick = time.monotonic()
                    children = [child for child in list(self.children) if child.poll() is None]
                    curve.require(len(children) <= PROFILE["process_cap"], "process inventory cap")
                    processes = []
                    for child in children:
                        try:
                            processes.append(process_snapshot(child.pid))
                        except FileNotFoundError:
                            processes.append({"pid": child.pid, "exited_during_sample": True})
                    curve.write_json(self.observation_dir / f"sample-{index:03}.json", {
                        "seconds": tick - began,
                        "worker_pids": [worker.pid for worker in self.workers],
                        "processes": processes,
                        "sensors": {path: bounded_text(path, 128) for path in paths},
                        "cpu_pressure": bounded_text("/proc/pressure/cpu"),
                        "collection_seconds": time.monotonic() - tick})
                    if ended.wait(max(0, PROFILE["sample_seconds"] - (time.monotonic() - tick))):
                        return
                errors.append("observation sample cap reached")
            except Exception as error:
                errors.append(type(error).__name__)

        observer = threading.Thread(target=observe, daemon=True)
        observer.start()
        try:
            super().measure()
        finally:
            ended.set()
            observer.join(timeout=2)
            self.row["observation_errors"] = errors
            self.row["observation_thread_stopped"] = not observer.is_alive()
        curve.require(not errors and not observer.is_alive(), "observation failure")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "ctl", "probe", "output"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    args = parser.parse_args()
    curve.require(sys.platform == "linux", "Linux fixture required")
    curve.require(not curve.competing_jobs(), "competing build/test process")
    os.umask(0o077)
    args.output.mkdir(mode=0o700, parents=False, exist_ok=False)
    artifacts = {}
    for name in ("worker", "ctl", "probe"):
        source = getattr(args, name).resolve(strict=True)
        target = args.output.resolve() / name
        shutil.copyfile(source, target)
        target.chmod(0o700)
        curve.require(curve.sha(source) == curve.sha(target), "artifact changed during copy")
        artifacts[name] = {"source": str(source), "sha256": curve.sha(target)}
        setattr(args, name, target)
    args.omitted = args.worker
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("ORISHU_", "OTEL_")) and key != "TOKIO_WORKER_THREADS"}
    manifest = {"schema_version": 1, "kind": "baseline-diagnostic", "acceptance_run": False,
                "profile": PROFILE, "curve_profile": curve.PROFILE, "artifacts": artifacts,
                "source": curve.source_identity(), "host": curve.host_snapshot(),
                "affinity": sorted(os.sched_getaffinity(0)), "command": sys.argv}
    curve.write_json(args.output / "manifest.json", manifest)
    began = time.monotonic()
    complete = 0
    failure = None
    for index in range(PROFILE["cells"]):
        row = {"index": index, "worker_count": 3, "mode": "compiled_off", "status": "failed",
               "phase": "formation", "host_before": curve.host_snapshot()}
        directory = args.output / f"cell-{index:03}"
        directory.mkdir(mode=0o700)
        fixture = ObservedCell(args, 3, "compiled_off", environment, row)
        fixture.observation_dir = directory
        try:
            remaining = PROFILE["budget_seconds"] - PROFILE["cleanup_reserve_seconds"] - (time.monotonic() - began)
            curve.require(remaining > 0, "diagnostic budget exhausted")
            curve.require(not curve.competing_jobs(), "competing build/test process")
            signal.setitimer(signal.ITIMER_REAL, min(180, remaining))
            with tempfile.TemporaryDirectory(prefix="ofb-") as temporary:
                fixture.run(Path(temporary))
            curve.require(row.get("cleanup_clean"), "unclean fixture shutdown")
            complete += 1
        except (Exception, KeyboardInterrupt) as error:
            row["status"] = "failed"
            failure = type(error).__name__
            row["error_type"] = failure
            row["error"] = str(error)[:256] if isinstance(error, (ValueError, TimeoutError)) else "fixture failure"
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            row["elapsed_seconds"] = time.monotonic() - fixture.started
            row["host_after"] = curve.host_snapshot()
            curve.write_json(directory / "result.json", row)
        print(f"cell {index + 1}/6: {row['status']} ({row['elapsed_seconds']:.3f}s)", flush=True)
        if failure:
            break
    unchanged = all(curve.sha(getattr(args, name)) == value["sha256"] for name, value in artifacts.items())
    source_unchanged = curve.source_identity() == manifest["source"]
    elapsed = time.monotonic() - began
    passed = complete == 6 and unchanged and source_unchanged and elapsed <= 300
    curve.write_json(args.output / "finished.json", {"cells": complete, "elapsed_seconds": elapsed,
                     "artifacts_unchanged": unchanged, "source_unchanged": source_unchanged,
                     "status": "complete" if passed else "incomplete", "failure": failure})
    return 0 if passed else 2


if __name__ == "__main__":
    def expired(_signal, _frame):
        raise TimeoutError("baseline diagnostic deadline")
    signal.signal(signal.SIGALRM, expired)
    raise SystemExit(main())
