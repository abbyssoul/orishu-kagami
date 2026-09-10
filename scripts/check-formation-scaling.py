#!/usr/bin/env python3
"""One bounded formation-only regression using the telemetry curve's exact gates.

No timed load, automatic retries or overhead acceptance. Always use a new output
directory; failures and cleanup disposition are retained separately from curves.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

_spec = importlib.util.spec_from_file_location(
    "formation_curve", Path(__file__).with_name("measure-formation-telemetry.py"))
curve = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(curve)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "ctl", "probe", "output"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    parser.add_argument("--workers", required=True, type=int, choices=(3, 10, 30))
    parser.add_argument("--mode", choices=("omitted", "metrics"), default="omitted")
    args = parser.parse_args()
    curve.require(sys.platform == "linux", "Linux fixture required")
    curve.require(not curve.competing_jobs(), "competing build/test process")
    os.umask(0o077)
    args.output.mkdir(mode=0o700, parents=False, exist_ok=False)
    artifacts = {}
    for name in ("worker", "ctl", "probe"):
        path = getattr(args, name).resolve(strict=True)
        setattr(args, name, path)
        artifacts[name] = {"path": str(path), "sha256": curve.sha(path)}
    args.omitted = args.worker
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("ORISHU_", "OTEL_")) and key != "TOKIO_WORKER_THREADS"}
    row = {"schema_version": 1, "kind": "formation-only", "acceptance_run": False,
           "worker_count": args.workers, "mode": args.mode, "status": "failed",
           "artifacts": artifacts, "profile": curve.PROFILE, "source": curve.source_identity()}
    fixture = curve.Cell(args, args.workers, args.mode, environment, row)
    signal.setitimer(signal.ITIMER_REAL, 90)
    try:
        with tempfile.TemporaryDirectory(prefix="ofs-") as temporary:
            fixture.root = Path(temporary)
            try:
                fixture.form()
                row["status"] = "complete"
            finally:
                # Only fixture-owned children. Cleanup runs before credentials
                # and sockets disappear, including on timeout or interruption.
                signal.setitimer(signal.ITIMER_REAL, 0)
                row["cleanup_clean"] = curve.stop(fixture.children)
                row["logs"] = [log.finish() for log in fixture.logs]
    except (Exception, KeyboardInterrupt) as error:
        row["error_type"] = type(error).__name__
        row["error"] = (str(error)[:256] if isinstance(error, (ValueError, TimeoutError))
                        else "fixture failure; inspect bounded stage evidence")
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        row["elapsed_seconds"] = time.monotonic() - fixture.started
        row["artifacts_unchanged"] = all(
            curve.sha(Path(value["path"])) == value["sha256"] for value in artifacts.values())
        if not row.get("cleanup_clean", False) or not row["artifacts_unchanged"]:
            row["status"] = "failed"
        curve.write_json(args.output / "result.json", row)
    clean = row.get("cleanup_clean", False) and row["artifacts_unchanged"]
    print(f"{row['status']}: {args.workers} workers; cleanup/identity clean={clean}")
    return 0 if row["status"] == "complete" and clean else 2


if __name__ == "__main__":
    def expired(_signal, _frame):
        raise TimeoutError("formation-only diagnostic deadline")
    signal.signal(signal.SIGALRM, expired)
    raise SystemExit(main())
