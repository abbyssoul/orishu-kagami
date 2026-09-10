#!/usr/bin/env python3
"""Official Collector/log receipt through temporary systemd and rootless Podman."""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time
import uuid

from worker_deployment_receipts import journal_messages, match_invocations
from worker_formation_receipts import MAX_LOG_BYTES, collector, read_logs

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("worker_service", ROOT / "scripts/check-worker-user-service.py")
service = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(service)


def run(command, environment=None, timeout=15, success=True, limit=512 * 1024):
    # Known tools only; retain no unbounded in-memory stdout and never print it.
    with tempfile.TemporaryFile() as output:
        result = subprocess.run(command, env=environment, stdout=output, stderr=subprocess.STDOUT,
                                timeout=timeout, check=False)
        output.seek(0)
        raw = output.read(limit + 1)
    assert len(raw) <= limit, "deployment tool output byte budget"
    if (result.returncode == 0) != success:
        # Finite classifications only; runtime output may contain credentials.
        reason = next((label for marker, label in (
            (b"Temporary failure resolving", "package repository DNS unavailable"),
            (b"Unable to locate package", "required runtime package unavailable"),
            (b"No space left on device", "filesystem space exhausted"),
        ) if marker in raw), "output withheld")
        raise AssertionError(f"{Path(command[0]).name} failed ({reason})")
    return raw


def poll(check, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("deployment receipt observation deadline")


def configure_collector(root, endpoint):
    path = root / "collector.yml"
    text = (ROOT / "etc/otelcol-worker-local.yml").read_text()
    path.write_text(text.replace("127.0.0.1:4318", endpoint))
    return path


class UserService:
    def __init__(self, root, args, environment, stack):
        assert service.ctl("show", "--property=Version", check=False).returncode == 0
        assert not any(line.startswith(("ORISHU_", "OTEL_")) for line in
                       service.ctl("show-environment").stdout.decode().splitlines())
        self.root, self.args, self.environment = root, args, environment
        self.state, self.socket = root / "state", root / "worker.sock"
        self.port, receiver = collector.port(), collector.port()
        while receiver == self.port:
            receiver = collector.port()
        self.receipt = root / "received.jsonl"
        config = configure_collector(root, f"127.0.0.1:{receiver}")
        receiver_env = dict(environment, ORISHU_OTEL_TRACE_FILE=str(self.receipt), GOMEMLIMIT="100MiB")
        run([str(args.otelcol), "validate", "--config", str(config)], receiver_env)
        receiver_child = stack.enter_context(collector.process([str(args.otelcol), "--config", str(config)], receiver_env))
        collector.poll(lambda: collector.http(receiver, "GET", "/v1/traces")[0] == 405, [receiver_child])
        contents = service.TEMPLATE.read_text()
        replacements = {"WORKER": str(args.worker), "STATE_DIR": str(self.state),
                        "CLIENT_SOCKET": str(self.socket), "WORKING_DIRECTORY": str(root),
                        "OBSERVABILITY": "true", "METRICS": "true", "METRICS_BIND": f"127.0.0.1:{self.port}"}
        for key, value in replacements.items():
            assert re.fullmatch(r"[A-Za-z0-9_./:-]+", value), "unsafe systemd substitution"
            contents = contents.replace("@" + key + "@", value)
        assert contents.count("--tracing.enabled false") == 1 and "@" not in contents
        contents = contents.replace("--tracing.enabled false", " ".join(telemetry(f"http://127.0.0.1:{receiver}/v1/traces")))
        unit_file = root / f"orishu-poc-test-{uuid.uuid4().hex}.service"
        unit_file.write_text(contents)
        self.unit = stack.enter_context(service.linked_unit(unit_file))

    def start(self):
        service.ctl("start", self.unit)
        props = service.properties(self.unit)
        assert props["ActiveState"] == "active" and props["Restart"] == "no"
        assert props["KillMode"] == "control-group" and props["TimeoutStopUSec"] == "10s"
        self.pid = props["MainPID"]
        self.invocation = service.ctl("show", self.unit, "--property=InvocationID", "--value").stdout.decode().strip()
        assert re.fullmatch("[0-9a-f]{32}", self.invocation)

    def http(self, route):
        try:
            return collector.http(self.port, "GET", route)
        except (OSError, collector.http_client.HTTPException):
            return 0, b""

    def stop(self):
        before = time.monotonic()
        service.ctl("stop", self.unit)
        assert time.monotonic() - before < 10
        props = service.properties(self.unit)
        assert props["Result"] == "success" and props["ExecMainStatus"] == "0"
        assert props["MainPID"] == "0" and props["NRestarts"] == "0"
        assert not self.socket.exists()

    def logs(self, forbidden):
        raw = run(["journalctl", "--user", "--unit", self.unit, "--no-pager", "--all",
                   "--output=json", "--lines=513", "_SYSTEMD_INVOCATION_ID=" + self.invocation,
                   "_PID=" + self.pid, "_TRANSPORT=stdout"], self.environment)
        # Journald attaches _CMDLINE, which legitimately includes the public
        # configured name. Credentials remain forbidden in every journal field;
        # names/operation markers are additionally forbidden in worker MESSAGE.
        secrets = [(self.state / "operator.token").read_bytes().strip(), b"BEGIN PRIVATE KEY"]
        return journal_messages(raw, self.invocation, self.pid, secrets, record_forbidden=forbidden)


class Container:
    def __init__(self, root, args, environment, stack):
        self.root, self.args, self.environment = root, args, environment
        self.scope = uuid.uuid4().hex
        self.names = []
        self.image = f"localhost/orishu-poc-test-{self.scope}:logs"
        self.image_created = False
        self.name = f"orishu-poc-test-{self.scope}-worker"
        self.receiver_name = f"orishu-poc-test-{self.scope}-collector"
        self.state, runtime = root / "state", root / "run"
        self.socket = runtime / "worker.sock"
        self.prefix = b""
        self.started = False
        assert os.getuid() != 0
        assert self.podman("info", "--format", "{{.Host.Security.Rootless}}").strip() == b"true"
        assert re.fullmatch(r"docker\.io/library/debian@sha256:[0-9a-f]{64}", args.base_image)
        self.podman("image", "exists", args.base_image)
        stack.callback(self.cleanup)
        context = root / "build"
        context.mkdir(mode=0o700)
        for name, source in (("orishu-worker", args.worker), ("orishuctl", args.ctl)):
            shutil.copyfile(source, context / name)
            (context / name).chmod(0o755)
        self.podman("image", "exists", self.image, success=False)
        self.image_created = True
        # Like the existing recipe, apt may access Debian repositories during
        # image construction. Runtime containers still use an isolated namespace.
        self.podman("build", "--pull=never", "--layers", "--tag", self.image,
                    "--label", f"org.orishu.poc.test={self.scope}",
                    "--file", str(ROOT / "etc/containers/Containerfile.worker-poc"),
                    "--build-arg", "BASE_IMAGE=" + args.base_image,
                    "--build-arg", "WORKER_FEATURES=observability,otlp-tracing", str(context), timeout=180)
        print("container image", self.podman("image", "inspect", self.image, "--format", "{{.Id}}").decode().strip(), flush=True)
        self.common = ["--pull=never", "--userns=keep-id", "--user", f"{os.getuid()}:{os.getgid()}",
                       "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges",
                       "--restart=no", "--pids-limit=128", "--log-driver=k8s-file", "--log-opt=max-size=64k"]
        receiver = root / "collector"
        receiver.mkdir(mode=0o700)
        self.receipt = receiver / "received.jsonl"
        configure_collector(receiver, "127.0.0.1:4318")
        # Collector sees only its own configuration/receipt directory and binary.
        mounts = ["--volume", f"{receiver}:/collector:rw", "--volume", f"{args.otelcol}:/otelcol:ro"]
        self.names.append(self.receiver_name)
        receiver_command = ["run", "--detach", "--name", self.receiver_name,
                            "--label", f"org.orishu.poc.test={self.scope}", *self.common,
                            "--network=none", *mounts, "--env", "ORISHU_OTEL_TRACE_FILE=/collector/received.jsonl",
                            "--env", "GOMEMLIMIT=100MiB", "--entrypoint=/otelcol", self.image,
                            "--config", "/collector/collector.yml"]
        self.podman(*receiver_command)
        receiver_view = json.loads(self.podman("inspect", self.receiver_name))[0]
        assert not receiver_view["NetworkSettings"]["Ports"]
        assert receiver_view["HostConfig"]["NetworkMode"] == "none"
        assert {mount["Destination"] for mount in receiver_view["Mounts"]} == {"/collector", "/otelcol"}
        ready = self.podman("exec", self.receiver_name, "curl", "--silent", "--retry", "5",
                            "--retry-connrefused", "--retry-delay", "1", "--retry-max-time", "5",
                            "--max-time", "1", "--write-out", "\n%{http_code}", "http://127.0.0.1:4318/v1/traces")
        assert ready.endswith(b"\n405")
        self.state.mkdir(mode=0o700)
        runtime.mkdir(mode=0o700)
        self.worker_command = ["run", "--detach", "--name", self.name, "--label", f"org.orishu.poc.test={self.scope}",
                               *self.common, "--network=container:" + self.receiver_name,
                               "--volume", f"{self.state}:/state:rw", "--volume", f"{runtime}:/run/orishu:rw", self.image,
                               "--state-dir=/state", "--listen.clients=/run/orishu/worker.sock", "--accepts.peers=false",
                               "--observability.enabled=true", "--observability.bind=127.0.0.1:9168",
                               *telemetry("http://127.0.0.1:4318/v1/traces")]

    def podman(self, *command, **kwargs):
        return run([self.args.podman, *command], self.environment, **kwargs)

    def start(self):
        if self.started:
            self.podman("start", self.name)
        else:
            self.names.append(self.name)
            self.podman(*self.worker_command)
            self.started = True
        view = json.loads(self.podman("inspect", self.name))[0]
        assert view["HostConfig"]["ReadonlyRootfs"] and not view["NetworkSettings"]["Ports"]
        assert view["HostConfig"]["NetworkMode"].startswith("container:")
        assert view["HostConfig"]["RestartPolicy"]["Name"] in ("no", "")
        assert self.podman("exec", self.name, "id", "-u").strip() == str(os.getuid()).encode()

    def http(self, route):
        raw = self.podman("exec", self.name, "curl", "--silent", "--max-time", "1",
                          "--write-out", "\n%{http_code}", "http://127.0.0.1:9168" + route, limit=32773)
        body, status = raw.rsplit(b"\n", 1)
        assert len(body) <= (32768 if route == "/metrics" else 1024)
        return int(status), body

    def stop(self):
        before = time.monotonic()
        self.podman("stop", "--time=10", self.name)
        assert time.monotonic() - before < 12
        view = json.loads(self.podman("inspect", self.name))[0]
        assert not view["State"]["Running"] and view["State"]["ExitCode"] == 0
        assert not self.socket.exists()

    def logs(self, forbidden):
        raw = self.podman("logs", "--tail=513", self.name, limit=MAX_LOG_BYTES)
        assert all(secret not in raw for secret in forbidden if secret), "secret in container log"
        assert raw.startswith(self.prefix), "container log prefix was lost or rotated"
        return raw[len(self.prefix):]

    def next_invocation(self, raw):
        self.prefix += raw

    def cleanup(self):
        failures = []
        for name in reversed(self.names):
            exists = subprocess.run([self.args.podman, "container", "exists", name], env=self.environment, timeout=15).returncode == 0
            if exists:
                assert self.podman("inspect", name, "--format", '{{index .Config.Labels "org.orishu.poc.test"}}').strip() == self.scope.encode()
                self.podman("stop", "--time=10", name)
                state = json.loads(self.podman("inspect", name))[0]["State"]
                assert not state["Running"], "deployment child survived stop"
                if state["ExitCode"] != 0:
                    failures.append(name)
                self.podman("rm", name)
        if self.image_created:
            exists = subprocess.run([self.args.podman, "image", "exists", self.image], env=self.environment, timeout=15).returncode == 0
            if exists:
                assert self.podman("image", "inspect", self.image, "--format", '{{index .Labels "org.orishu.poc.test"}}').strip() == self.scope.encode()
                self.podman("rmi", self.image)
        assert not failures, "deployment child did not exit cleanly (fixture resources removed)"


def telemetry(endpoint):
    return ["--name", "deployment-worker-marker",
            "--logging.enabled", "true", "--logging.queue-records", "256", "--logging.shutdown-ms", "250",
            "--tracing.enabled", "true", "--tracing.endpoint", endpoint, "--tracing.sample-ppm", "1000000",
            "--tracing.batch-size", "1", "--tracing.export-timeout-ms", "1000", "--tracing.shutdown-timeout-ms", "1000"]


def journey(mode, root, args, environment):
    records = []
    forbidden = [b"deployment-lock", b"deployment-unlock", b"deployment-worker-marker", b"BEGIN PRIVATE KEY"]
    with contextlib.ExitStack() as stack:
        deployment = (UserService if mode == "systemd" else Container)(root, args, environment, stack)
        previous, credentials = None, None
        for phase in range(2):
            deployment.start()
            if mode == "container" and phase == 0:
                versions = deployment.podman("exec", deployment.name, "dpkg-query", "--show",
                                             "--showformat=${Package}=${Version}\\n",
                                             "ca-certificates", "curl", "libc6", "libgcc-s1", limit=2048)
                print("container runtime packages:", versions.decode().strip(), flush=True)
            poll(lambda: deployment.http("/readyz")[0] == 200)
            files = [(deployment.state / name).read_bytes() for name in ("identity.json", "operator.token")]
            if credentials is not None:
                assert files == credentials, "restart changed retained credentials"
            credentials = files
            forbidden.append(files[1].strip())
            for path in (deployment.state / "identity.json", deployment.state / "operator.token", deployment.socket):
                assert path.stat().st_mode & 0o777 == 0o600
                assert path.stat().st_uid == os.getuid()

            def operator(arguments, authenticated=False, success=True):
                command = [str(args.ctl), "--host", str(deployment.socket), "--output", "json", "--timeout", "2s"]
                if authenticated:
                    command += ["--operator-token-file", str(deployment.state / "operator.token")]
                raw = run(command + arguments, environment, success=success, limit=16384)
                return json.loads(raw) if success else None

            original = operator(["cluster", "info"])
            member = operator(["inspect", original["sourceNodeId"]])
            if previous is not None:
                assert original["formationId"] != previous["formationId"]
                assert original["sourceNodeId"] != previous["sourceNodeId"]
            assert not original["locked"] and original["nodes"] == original["alive"] == 1
            operator(["cluster", "lock", "--formation-id", original["formationId"], "--operation-id", "deployment-lock"], success=False)
            for action, expected in (("lock", True), ("unlock", False)):
                assert operator(["cluster", action, "--formation-id", original["formationId"],
                                 "--operation-id", "deployment-" + action], True)["locked"] is expected
                current = operator(["cluster", "info"])
                assert current["formationId"] == original["formationId"]
                assert current["sourceNodeId"] == original["sourceNodeId"] and current["locked"] is expected
            members = operator(["ls"])
            assert len(members) == 1 and members[0]["nodeId"] == original["sourceNodeId"]
            assert members[0]["certFingerprint"] == member["certFingerprint"] and members[0]["liveness"] == "alive"

            def flushed():
                status, body = deployment.http("/metrics")
                assert status == 200 and len(body) <= 32768
                assert all(secret not in body for secret in forbidden)
                values = {line.split()[0]: int(line.split()[1]) for line in body.decode().splitlines()
                          if line.startswith(("orishu_worker_trace_", "orishu_worker_log_"))}
                assert len(values) == 21
                for key, value in values.items():
                    if key not in {"orishu_worker_trace_enqueued_total", "orishu_worker_trace_accepted_total",
                                   "orishu_worker_log_accepted_total", "orishu_worker_log_written_total"}:
                        assert value == 0, "quiet deployment fixture lost telemetry"
                return (values["orishu_worker_trace_accepted_total"] == values["orishu_worker_trace_enqueued_total"] == 8
                        and values["orishu_worker_log_written_total"] == values["orishu_worker_log_accepted_total"] == 9)

            poll(flushed)
            for route in ("/livez", "/readyz", "/startupz"):
                assert deployment.http(route) == (200, b"ok\n")
            deployment.stop()
            log_path = root / f"logs-{phase}.jsonl"

            def collected():
                raw = deployment.logs(forbidden)
                assert len(raw) <= MAX_LOG_BYTES
                log_path.write_bytes(raw)
                rows = read_logs(log_path, forbidden, complete=True)
                return (raw, rows) if len(rows) == 23 else None

            raw, rows = poll(collected)
            records.append(rows)
            if mode == "container":
                deployment.next_invocation(raw)
            previous = original
            print(f"{mode}: invocation {phase + 1}; control/probes, collected logs and clean stop pass", flush=True)
        # The outer stack stops the Collector (and cleans fixture resources).
        receipt = deployment.receipt
    spans = collector.read_spans(receipt, forbidden, complete=True)
    total = match_invocations(spans, records)
    print(f"PASS: {mode}; {total} received spans matched to collected logs; two fresh identities; retained credentials; bounded stop", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deployment", choices=("systemd", "container", "both"), default="both")
    for name in ("worker", "ctl", "otelcol"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--podman", default="podman")
    parser.add_argument("--base-image", default="docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f")
    args = parser.parse_args()
    os.umask(0o077)
    environment = {key: value for key, value in os.environ.items() if not key.startswith(("ORISHU_", "OTEL_"))}
    for name in ("worker", "ctl", "otelcol"):
        path = Path(shutil.which(str(getattr(args, name))) or getattr(args, name)).resolve()
        setattr(args, name, path)
    with tempfile.TemporaryDirectory(prefix="orishu-deployment-logs-") as temporary:
        # All restarts use private snapshots; later builds cannot replace them.
        for name in ("worker", "ctl", "otelcol"):
            snapshot = Path(temporary) / name
            shutil.copyfile(getattr(args, name), snapshot)
            snapshot.chmod(0o700)
            setattr(args, name, snapshot)
            with snapshot.open("rb") as stream:
                print(name, "SHA-256", hashlib.file_digest(stream, "sha256").hexdigest(), flush=True)
        assert run([str(args.otelcol), "--version"], environment).strip() == b"otelcol version 0.160.0"
        for mode in (("systemd", "container") if args.deployment == "both" else (args.deployment,)):
            root = Path(temporary) / mode
            root.mkdir(mode=0o700)
            journey(mode, root, args, environment)


if __name__ == "__main__":
    def expired(_signum, _frame):
        raise TimeoutError("deployment logging check exceeded 360 seconds")
    signal.signal(signal.SIGALRM, expired)
    signal.alarm(360)
    try:
        main()
    finally:
        signal.alarm(0)
