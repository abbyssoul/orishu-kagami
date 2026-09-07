#!/usr/bin/env python3
"""Bounded rootless Podman worker/probe recipe; no published ports or images."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
CONTAINERFILE = ROOT / "etc/containers/Containerfile.worker-poc"


def run(command, check=True, timeout=15, input=None):
    result = subprocess.run(command, input=input, capture_output=True, timeout=timeout)
    assert len(result.stdout) + len(result.stderr) <= 512 * 1024, "fixture tool output budget"
    if check:
        assert result.returncode == 0, f"{Path(command[0]).name} failed: {result.stderr[:2048].decode(errors='replace')}"
    return result


def poll(check):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise AssertionError("container observation deadline")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--podman", default="podman")
    parser.add_argument("--base-image", required=True, help="already-pulled Debian trixie-slim repository@sha256:digest")
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--minimal-worker", type=Path, required=True)
    parser.add_argument("--ctl", type=Path, required=True)
    parser.add_argument("--promtool", type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch(r"docker\.io/library/debian@sha256:[a-f0-9]{64}", args.base_image), "explicit official Debian digest required"
    assert os.getuid() != 0, "rootless evaluation only"

    def podman(*command, **kwargs):
        return run([args.podman, *command], **kwargs)

    assert podman("info", "--format", "{{.Host.Security.Rootless}}").stdout.strip() == b"true"
    podman("image", "exists", args.base_image)
    print(podman("--version").stdout.decode().strip(), "base", args.base_image, flush=True)
    scope = uuid.uuid4().hex
    images = []
    containers = []
    with tempfile.TemporaryDirectory(prefix="orishu-container-") as temporary:
        root = Path(temporary)
        try:
            for mode, source, features in (("enabled", args.worker, "observability,otlp-tracing"),
                                           ("minimal", args.minimal_worker, "none")):
                context = root / ("image-" + mode)
                context.mkdir(mode=0o700)
                for name, original in (("orishu-worker", source), ("orishuctl", args.ctl)):
                    target = context / name
                    shutil.copyfile(original.resolve(), target)
                    target.chmod(0o755)
                    print(mode, name, "SHA-256", hashlib.sha256(target.read_bytes()).hexdigest(), flush=True)
                image = f"localhost/orishu-poc-test-{scope}:{mode}"
                assert podman("image", "exists", image, check=False).returncode != 0
                images.append(image)
                podman("build", "--pull=never", "--layers", "--tag", image,
                       "--label", f"org.orishu.poc.test={scope}", "--file", str(CONTAINERFILE),
                       "--build-arg", "BASE_IMAGE=" + args.base_image,
                       "--build-arg", "WORKER_FEATURES=" + features, str(context), timeout=180)
                print(mode, "image", podman("image", "inspect", image, "--format", "{{.Id}}").stdout.decode().strip(), flush=True)

            common = ["--pull=never", "--userns=keep-id", "--user", f"{os.getuid()}:{os.getgid()}",
                      "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges",
                      "--restart=no", "--pids-limit=128", "--log-driver=k8s-file", "--log-opt=max-size=64k"]
            for mode, image, enabled, metrics in (("default", images[0], False, True),
                    ("enabled", images[0], True, True), ("probes-only", images[0], True, False),
                    ("feature-omitted", images[1], False, True), ("omitted-requested", images[1], True, True)):
                work = root / mode
                work.mkdir(mode=0o700)
                state, runtime = work / "state", work / "run"
                state.mkdir(mode=0o700)
                runtime.mkdir(mode=0o700)
                name = f"orishu-poc-test-{scope}-{mode}"
                containers.append(name)
                command = ["run", "--detach", "--name", name, "--label", f"org.orishu.poc.test={scope}",
                           *common, "--network=none", "--volume", f"{state}:/state:rw",
                           "--volume", f"{runtime}:/run/orishu:rw"]
                if enabled:
                    command += ["--health-cmd", "curl --fail --silent --max-time 1 http://127.0.0.1:9168/livez",
                                "--health-interval=disable", "--health-on-failure=none",
                                "--health-timeout=2s", "--health-max-log-count=3", "--health-max-log-size=1024"]
                command += [image]
                if enabled:
                    command += ["--state-dir=/state", "--listen.clients=/run/orishu/worker.sock", "--accepts.peers=false",
                                "--observability.enabled=true", "--observability.bind=127.0.0.1:9168",
                                "--observability.metrics=" + str(metrics).lower(), "--tracing.enabled=false"]
                podman(*command)

                def inspect():
                    return json.loads(podman("inspect", name).stdout)[0]

                if mode == "omitted-requested":
                    poll(lambda: not inspect()["State"]["Running"])
                    assert inspect()["State"]["ExitCode"] == 2
                    assert not list(state.iterdir()) and not list(runtime.iterdir())
                    print("PASS:", mode, "startup refusal without credentials/socket", flush=True)
                    continue

                def client(arguments, auth=False, check=True):
                    options = ["exec", name, "/usr/local/bin/orishuctl", "--host", "/run/orishu/worker.sock",
                               "--timeout", "2s", "--output", "json"]
                    if auth:
                        options += ["--operator-token-file", "/state/operator.token"]
                    return podman(*options, *arguments, check=check)

                def summary():
                    result = client(["cluster", "info"], check=False)
                    return json.loads(result.stdout) if result.returncode == 0 else None

                original = poll(summary)
                observed = inspect()
                assert observed["HostConfig"]["ReadonlyRootfs"]
                assert observed["HostConfig"]["NetworkMode"] == "none"
                assert not observed["NetworkSettings"]["Ports"]
                assert observed["HostConfig"]["RestartPolicy"]["Name"] in ("no", "")
                assert podman("exec", name, "id", "-u").stdout.strip() == str(os.getuid()).encode()
                assert stat.S_IMODE((runtime / "worker.sock").stat().st_mode) == 0o600
                credentials = [(state / filename).read_bytes() for filename in ("identity.json", "operator.token")]
                for filename in ("identity.json", "operator.token"):
                    assert stat.S_IMODE((state / filename).stat().st_mode) == 0o600
                    assert (state / filename).stat().st_uid == os.getuid()
                for action, expected in (("lock", True), ("unlock", False)):
                    arguments = ["cluster", action, "--formation-id", original["formationId"], "--operation-id", "container-" + action]
                    assert client(arguments, check=False).returncode != 0
                    assert json.loads(client(arguments, auth=True).stdout)["locked"] is expected
                if enabled:
                    for route in ("startupz", "livez", "readyz"):
                        podman("exec", name, "curl", "--fail", "--silent", "--max-time", "1", "http://127.0.0.1:9168/" + route)
                    podman("healthcheck", "run", name)
                    assert inspect()["State"]["Health"]["Status"] == "healthy"
                    response = podman("exec", name, "curl", "--silent", "--max-time", "1", "--write-out", "\n%{http_code}", "http://127.0.0.1:9168/metrics").stdout
                    assert len(response) <= 32773
                    body, status = response.rsplit(b"\n", 1)
                    assert status == (b"200" if metrics else b"404")
                    assert credentials[1].strip() not in body
                    if metrics:
                        run([str(args.promtool.resolve()), "check", "metrics"], input=body)
                else:
                    result = podman("exec", name, "curl", "--silent", "--max-time", "1", "http://127.0.0.1:9168/livez", check=False)
                    assert result.returncode == 7, "disabled diagnostics must refuse connection"

                if mode == "enabled":
                    print("Runtime packages:", podman("exec", name, "dpkg-query", "--show",
                          "--showformat=${Package}=${Version}\\n", "ca-certificates", "curl", "libc6", "libgcc-s1").stdout.decode().strip(), flush=True)
                    for sharing in (False, True):
                        probe_name = f"orishu-poc-test-{scope}-probe-{sharing}"
                        containers.append(probe_name)
                        response = podman("run", "--name", probe_name, "--label", f"org.orishu.poc.test={scope}", *common,
                            "--network=" + ("container:" + name if sharing else "none"), "--entrypoint=/usr/bin/curl",
                            image, "--fail", "--silent", "--max-time", "1", "http://127.0.0.1:9168/metrics", check=False)
                        assert response.returncode == (0 if sharing else 7), "network namespace isolation"
                        if sharing:
                            assert b"orishu_worker_ready 1\n" in response.stdout
                            run([str(args.promtool.resolve()), "check", "metrics"], input=response.stdout)
                            assert not json.loads(podman("inspect", probe_name).stdout)[0]["Mounts"], "scraper must not receive worker state"
                before = time.monotonic()
                podman("stop", "--time=10", name)
                assert time.monotonic() - before < 12
                assert inspect()["State"]["ExitCode"] == 0 and not inspect()["State"]["Running"]
                assert not (runtime / "worker.sock").exists()
                logs = podman("logs", name)
                assert credentials[1].strip() not in logs.stdout + logs.stderr
                podman("start", name)
                restarted = poll(summary)
                assert restarted["formationId"] != original["formationId"]
                assert restarted["sourceNodeId"] != original["sourceNodeId"]
                assert credentials == [(state / filename).read_bytes() for filename in ("identity.json", "operator.token")]
                print("PASS:", mode, "private non-root state, control, probes, stop and explicit restart", flush=True)
        finally:
            for name in reversed(containers):
                if podman("container", "exists", name, check=False).returncode == 0:
                    assert podman("inspect", name, "--format", '{{index .Config.Labels "org.orishu.poc.test"}}').stdout.strip() == scope.encode()
                    podman("stop", "--time=10", name)
                    podman("rm", name)
            for image in images:
                if podman("image", "exists", image, check=False).returncode == 0:
                    assert podman("image", "inspect", image, "--format", '{{index .Labels "org.orishu.poc.test"}}').stdout.strip() == scope.encode()
                    podman("rmi", image)
    print("PASS: five container modes and isolated/shared network probes; fixture containers/images removed")


if __name__ == "__main__":
    main()
