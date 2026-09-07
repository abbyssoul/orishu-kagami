#!/usr/bin/env python3
"""Test only uniquely named runtime-linked user units; never install or enable."""
import argparse
import contextlib
import hashlib
import http.client
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import socket
import stat
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "etc/systemd/orishu-worker-poc.service.example"


def run(command, check=True, timeout=15):
    environment = {key: value for key, value in os.environ.items() if not key.startswith("ORISHU_")}
    result = subprocess.run(command, capture_output=True, timeout=timeout, env=environment)
    assert len(result.stdout) + len(result.stderr) <= 65536, "tool output budget"
    if check:
        assert result.returncode == 0, f"{Path(command[0]).name} failed (output withheld)"
    return result


def ctl(*arguments, check=True):
    return run(["systemctl", "--user", "--no-pager", *arguments], check=check)


def properties(unit):
    result = ctl("show", unit, "--property=LoadState,ActiveState,SubState,Result,MainPID,ExecMainStatus,NRestarts,Restart,KillMode,TimeoutStopUSec,UMask,NoNewPrivileges")
    return dict(line.split("=", 1) for line in result.stdout.decode().splitlines())


def poll(check, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, http.client.HTTPException):
            pass
        time.sleep(0.05)
    raise AssertionError("service observation deadline expired")


@contextlib.contextmanager
def linked_unit(unit_file):
    unit = unit_file.name
    assert re.fullmatch(r"orishu-poc-test-[0-9a-f]{32}\.service", unit)
    assert properties(unit)["LoadState"] == "not-found", "refuse existing unit"
    linked = False
    try:
        run(["systemd-analyze", "--user", "verify", str(unit_file)])
        ctl("link", "--runtime", str(unit_file))
        linked = True
        yield unit
    finally:
        if linked:
            # Exact fixture unit only. Never disable, reset or reload a broad pattern.
            ctl("stop", unit)
            state = properties(unit)
            assert state["MainPID"] == "0", "service process survived stop"
            ctl("reset-failed", unit, check=False)
            ctl("disable", "--runtime", unit)
            poll(lambda: properties(unit)["LoadState"] == "not-found")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--minimal-worker", type=Path, required=True)
    parser.add_argument("--ctl", type=Path, required=True)
    parser.add_argument("--promtool", type=Path, required=True)
    args = parser.parse_args()
    # An unrelated failed user unit is not this test's authority to repair it.
    assert ctl("show", "--property=Version", check=False).returncode == 0, "user manager unavailable"
    manager_env = ctl("show-environment").stdout.decode().splitlines()
    assert not any(line.startswith("ORISHU_") for line in manager_env), "PoC requires a user manager without inherited ORISHU_* settings"
    spec = importlib.util.spec_from_file_location("worker_prometheus", ROOT / "scripts/check-worker-prometheus.py")
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    assert "version 3.5.0 " in run([str(args.promtool.resolve()), "--version"]).stdout.decode()
    with tempfile.TemporaryDirectory(prefix="orishu-user-service-") as temporary:
        root = Path(temporary)
        binaries = {}
        for name, source in (("worker", args.worker), ("minimal", args.minimal_worker), ("ctl", args.ctl)):
            target = root / name
            shutil.copyfile(source.resolve(), target)
            target.chmod(0o700)
            binaries[name] = target
            print(name, "SHA-256", hashlib.sha256(target.read_bytes()).hexdigest())
        for mode, binary, enabled, metrics in (
            ("enabled", "worker", True, True), ("runtime-disabled", "worker", False, True),
            ("probes-only", "worker", True, False), ("feature-omitted", "minimal", False, True),
            ("omitted-requested", "minimal", True, True),
        ):
            work = root / mode
            work.mkdir(mode=0o700)
            state_dir, client_socket = work / "state", work / "worker.sock"
            port = harness.port()
            unit_file = root / f"orishu-poc-test-{uuid.uuid4().hex}.service"
            substitutions = {"WORKER": str(binaries[binary]), "STATE_DIR": str(state_dir),
                "CLIENT_SOCKET": str(client_socket), "OBSERVABILITY": str(enabled).lower(),
                "METRICS_BIND": f"127.0.0.1:{port}", "METRICS": str(metrics).lower(),
                "WORKING_DIRECTORY": str(work)}
            contents = TEMPLATE.read_text()
            for key, value in substitutions.items():
                assert re.fullmatch(r"[A-Za-z0-9_./:-]+", value), "unsafe unit substitution"
                contents = contents.replace("@" + key + "@", value)
            assert "@" not in contents and "[Install]" not in contents
            unit_file.write_text(contents)
            with linked_unit(unit_file) as unit:
                ctl("start", unit, check=False)
                if mode == "omitted-requested":
                    poll(lambda: properties(unit)["ActiveState"] == "failed")
                    status = properties(unit)
                    assert status["Result"] == "exit-code" and status["ExecMainStatus"] == "2"
                    assert status["NRestarts"] == "0" and status["Restart"] == "no"
                    assert not state_dir.exists() and not client_socket.exists()
                    print("PASS:", mode, "fails before credentials/socket creation; no restart")
                    continue

                def summary():
                    result = run([str(binaries["ctl"]), "--host", str(client_socket), "--timeout", "2s",
                                  "--output", "json", "cluster", "info"], check=False)
                    return json.loads(result.stdout) if result.returncode == 0 else None

                original = poll(summary)
                status = properties(unit)
                assert status["ActiveState"] == "active" and status["Restart"] == "no"
                assert status["KillMode"] == "control-group" and status["TimeoutStopUSec"] == "10s"
                assert status["UMask"] == "0077" and status["NoNewPrivileges"] == "yes"
                assert stat.S_IMODE(client_socket.stat().st_mode) == 0o600
                for name in ("identity.json", "operator.token"):
                    assert stat.S_IMODE((state_dir / name).stat().st_mode) == 0o600
                credentials = [(state_dir / name).read_bytes() for name in ("identity.json", "operator.token")]
                for action, expected in (("lock", True), ("unlock", False)):
                    request = [str(binaries["ctl"]), "--host", str(client_socket), "--timeout", "2s", "--output", "json"]
                    operation = ["cluster", action, "--formation-id", original["formationId"],
                                 "--operation-id", "service-" + action]
                    assert run(request + operation, check=False).returncode != 0, "unauthenticated service mutation succeeded"
                    receipt = json.loads(run(request + ["--operator-token-file", str(state_dir / "operator.token")] + operation).stdout)
                    assert receipt["locked"] is expected
                    current = summary()
                    assert current["formationId"] == original["formationId"] and current["locked"] is expected
                if enabled:
                    for route in ("/startupz", "/livez", "/readyz"):
                        assert harness.get(port, route, 1024)[0] == 200
                    code, _, body = harness.get(port, "/metrics", 32768)
                    assert code == (200 if metrics else 404)
                    if metrics:
                        validated = subprocess.run([str(args.promtool.resolve()), "check", "metrics"],
                                                   input=body, capture_output=True, timeout=10)
                        assert validated.returncode == 0, "service metrics parser failed"
                        assert b"orishu_worker_ready 1\n" in body
                    assert credentials[1].strip() not in body
                    assert harness.get(port, "/api/v1/cluster", 1024)[0] == 404
                else:
                    with socket.socket() as probe:
                        probe.settimeout(1)
                        assert probe.connect_ex(("127.0.0.1", port)) != 0, "disabled listener opened"
                # Incomplete real diagnostic requests stay open until the service
                # exits; client cleanup cannot manufacture server cancellation.
                with contextlib.ExitStack() as clients:
                    if enabled:
                        pending = clients.enter_context(socket.create_connection(("127.0.0.1", port), timeout=1))
                        pending.sendall(b"GET /metrics HTTP/1.1\r\nHost:")
                    before = time.monotonic()
                    ctl("stop", unit)
                    assert time.monotonic() - before < 10, "service graceful stop budget"
                    status = properties(unit)
                    assert status["ActiveState"] == "inactive" and status["Result"] == "success"
                    assert status["ExecMainStatus"] == "0" and status["MainPID"] == "0"
                    assert not client_socket.exists(), "owned socket survived graceful stop"
                    if enabled:
                        pending.settimeout(1)
                        assert pending.recv(1) == b"", "diagnostics survived service stop"
                # Explicit restart only; this is a fresh standalone formation,
                # never automatic recovery of an old assignment.
                ctl("start", unit)
                restarted = poll(summary)
                assert restarted["formationId"] != original["formationId"]
                assert restarted["sourceNodeId"] != original["sourceNodeId"]
                assert credentials == [(state_dir / name).read_bytes() for name in ("identity.json", "operator.token")]
                if mode == "enabled":
                    ctl("kill", "--kill-whom=main", "--signal=SIGKILL", unit)
                    poll(lambda: properties(unit)["ActiveState"] == "failed")
                    failed = properties(unit)
                    assert failed["Result"] == "signal" and failed["ExecMainStatus"] == "9"
                    assert failed["MainPID"] == "0" and failed["NRestarts"] == "0"
                    ctl("start", unit)
                    recovered = poll(summary)
                    assert recovered["formationId"] != restarted["formationId"]
                    assert recovered["sourceNodeId"] != restarted["sourceNodeId"]
                    assert credentials == [(state_dir / name).read_bytes() for name in ("identity.json", "operator.token")]
                    assert harness.get(port, "/readyz", 1024)[0] == 200
                journal = run(["journalctl", "--user", "--unit", unit, "--no-pager", "-n", "100", "--output=cat"]).stdout
                assert credentials[1].strip() not in journal
                print("PASS:", mode, "service/probes/permissions/stop/explicit fresh restart")
    print("PASS: five systemd user-service modes; fixture runtime links removed; journal records retained")


if __name__ == "__main__":
    main()
