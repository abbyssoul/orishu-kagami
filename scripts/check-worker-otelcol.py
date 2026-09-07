#!/usr/bin/env python3
"""Verify the pinned local Collector recipe with real worker/CLI processes."""
import argparse
import contextlib
from http import client as http_client
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import ssl
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
VERSION = "0.160.0"
MAX_RECEIPT_BYTES = 262144
MAX_SPANS = 32


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        assert key not in result, "duplicate receipt field"
        result[key] = value
    return result


def read_spans(path, forbidden=(), complete=False):
    """Read only complete, byte-bounded JSONL records; never print their content."""
    if not path.exists():
        return []
    with path.open("rb") as stream:
        raw = stream.read(MAX_RECEIPT_BYTES + 1)
    assert len(raw) <= MAX_RECEIPT_BYTES, "collector receipt exceeded test byte budget"
    assert all(secret not in raw for secret in forbidden if secret), "secret in collector receipt"
    if complete:
        assert not raw or raw.endswith(b"\n"), "incomplete final collector receipt"
    spans = []
    for line in raw.splitlines(keepends=True):
        if not line.endswith(b"\n"):
            break  # A concurrent write is not a complete receipt yet.
        document = json.loads(line, object_pairs_hook=unique_object)
        assert set(document) == {"resourceSpans"}, "unexpected collector signal"
        for resource in document["resourceSpans"]:
            assert resource["resource"]["attributes"] == [
                {"key": "service.name", "value": {"stringValue": "orishu-worker"}}]
            assert len(resource["scopeSpans"]) == 1
            scope = resource["scopeSpans"][0]
            assert scope["scope"]["name"] == "orishu.worker"
            assert 0 < len(scope["scope"]["version"]) <= 64
            for span in scope["spans"]:
                assert set(span) <= {
                    "traceId", "spanId", "parentSpanId", "traceState", "flags", "name",
                    "kind", "startTimeUnixNano", "endTimeUnixNano", "attributes",
                    "droppedAttributesCount", "events", "droppedEventsCount", "links",
                    "droppedLinksCount", "status"}, "unexpected span field"
                assert span["name"] == "orishu.client.request"
                assert span["kind"] == 2, "expected client-service server span"
                assert not span.get("parentSpanId"), "local span unexpectedly has a parent"
                assert not span.get("traceState")
                assert not span.get("events") and not span.get("links")
                assert not span.get("status", {}).get("message")
                for field, size in (("traceId", 32), ("spanId", 16)):
                    value = span[field]
                    assert re.fullmatch(f"[0-9a-f]{{{size}}}", value) and int(value, 16)
                start, end = int(span["startTimeUnixNano"]), int(span["endTimeUnixNano"])
                assert 0 < start <= end < 2**64
                attributes = span["attributes"]
                assert len(attributes) == 1 and attributes[0]["key"] == "orishu.outcome"
                assert attributes[0]["value"] in (
                    {"stringValue": "completed"}, {"stringValue": "rejected"},
                    {"stringValue": "failed"}, {"stringValue": "cancelled"})
                spans.append(span)
                assert len(spans) <= MAX_SPANS, "collector receipt exceeded span budget"
    ids = {(span["traceId"], span["spanId"]) for span in spans}
    assert len(ids) == len(spans), "duplicate collected span"
    return spans


def run(command, environment, success=True, cwd=None):
    # Known tools emit small output in these bounded invocations. Keep it private.
    with tempfile.TemporaryFile() as output:
        result = subprocess.run(command, env=environment, stdout=output,
                                stderr=subprocess.STDOUT, timeout=10, check=False, cwd=cwd)
        output.seek(0)
        body = output.read(16385)
    assert len(body) <= 16384, "command output exceeded test budget"
    assert (result.returncode == 0) == success, "unexpected command exit status"
    return body


@contextlib.contextmanager
def process(command, environment, cwd=None):
    child = subprocess.Popen(command, env=environment, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, umask=0o077, cwd=cwd)
    failed = False
    try:
        yield child
    except BaseException:
        failed = True
        raise
    finally:
        running = child.poll() is None
        if running:
            child.terminate()
        try:
            code = child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=3)
            raise AssertionError("process exceeded graceful shutdown budget") from None
        if not failed:
            assert code == 0, "fixture process did not exit cleanly"


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def http(listen_port, method, path, body=None, context=None, server_name="127.0.0.1"):
    connection = http_client.HTTPConnection("127.0.0.1", listen_port, timeout=1)
    try:
        if context is not None:
            assert context.check_hostname and context.verify_mode == ssl.CERT_REQUIRED
            stream = socket.create_connection(("127.0.0.1", listen_port), timeout=1)
            try:
                # Verify the explicitly selected fixture server identity, including
                # the valid control for a certificate mismatching the worker's URL.
                connection.sock = context.wrap_socket(stream, server_hostname=server_name)
            except BaseException:
                stream.close()
                raise
        connection.request(method, path, body, {"Content-Type": "application/x-protobuf"})
        response = connection.getresponse()
        limit = 32768 if path == "/metrics" else 16384
        content = response.read(limit + 1)
        assert len(content) <= limit, "HTTP response exceeded test byte budget"
        return response.status, content
    finally:
        connection.close()


def poll(check, children):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        assert all(child.poll() is None for child in children), "fixture process exited"
        try:
            result = check()
            if result:
                return result
        except (OSError, http_client.HTTPException):
            pass
        time.sleep(0.05)
    raise AssertionError("observation deadline expired")


def metrics(listen_port):
    status, body = http(listen_port, "GET", "/metrics")
    assert status == 200
    return {key: float(value) for line in body.decode("ascii").splitlines()
            if line and not line.startswith("#") for key, value in [line.split()]}


def trace_count(listen_port, name):
    return metrics(listen_port)[f"orishu_worker_trace_{name}_total"]


def expect_tls_alert(request, reason):
    try:
        request()
    except ssl.SSLError as error:
        assert error.reason == reason, "unexpected TLS refusal"
    else:
        raise AssertionError("receiver did not enforce TLS access policy")


def collector_tls_startup_failures(root, args, environment, tls):
    root.mkdir(mode=0o700)
    listen_port = port()
    config = root / "collector.yml"
    config.write_text((ROOT / "etc/otelcol-worker-local.yml").read_text().replace(
        "127.0.0.1:4318", f"127.0.0.1:{listen_port}"))
    original = (ROOT / "etc/otelcol-worker-mtls.yml").read_text()
    malformed = tls.directory / "malformed-client-ca.crt"
    malformed.write_text("invalid-test-ca-marker\n")
    malformed.chmod(0o600)
    receiver_environment = dict(environment, ORISHU_OTEL_TRACE_FILE=str(root / "unexpected.jsonl"),
                                GOMEMLIMIT="100MiB")
    for name, source, replacement, diagnostic in (
            ("missing-client-ca", "/client-ca.crt", "/missing-client-ca.crt", b"no such file or directory"),
            ("malformed-client-ca", "/client-ca.crt", "/malformed-client-ca.crt", b"failed to load client CA"),
            ("mismatched-server-key", "/server.key", "/client.key", b"private key does not match public key")):
        assert original.count(source) == 1
        overlay = root / f"{name}.yml"
        overlay.write_text(original.replace(source, replacement))
        command = [str(args.otelcol), "--config", str(config), "--config", str(overlay)]
        output = run(command, receiver_environment, success=False, cwd=tls.directory.parent)
        assert diagnostic in output, "collector failed for an unexpected configuration reason"
        with socket.socket() as connection:
            connection.settimeout(1)
            assert connection.connect_ex(("127.0.0.1", listen_port)) != 0, "failed collector left a listener"
        assert not read_spans(root / "unexpected.jsonl", complete=True)
        print(f"collector: {name} fails startup without a surviving listener", flush=True)


def scenario(mode, root, args, environment, tls=None, failure=None):
    root.mkdir(mode=0o700)
    receiver_port, worker_port = port(), port()
    while receiver_port == worker_port:
        worker_port = port()
    example = (ROOT / "etc/otelcol-worker-local.yml").read_text()
    assert example.count("127.0.0.1:4318") == 1
    config = root / "collector.yml"
    config.write_text(example.replace("127.0.0.1:4318", f"127.0.0.1:{receiver_port}"))
    receipt_path = root / "received.jsonl"
    collector_environment = dict(environment, ORISHU_OTEL_TRACE_FILE=str(receipt_path),
                                 GOMEMLIMIT="100MiB")
    collector_command = [str(args.otelcol), "--config", str(config)]
    collector_cwd = tls.directory.parent if tls else None
    context = tls.client_context() if tls else None
    server_name = "other.invalid" if failure == "wrong-server-name" else "127.0.0.1"
    if tls:
        overlay = ROOT / "etc/otelcol-worker-mtls.yml"
        if failure == "wrong-server-name":
            wrong_name = root / "wrong-name.yml"
            wrong_name.write_text(overlay.read_text().replace("/server.crt", "/wrong-name.crt")
                                  .replace("/server.key", "/wrong-name.key"))
            overlay = wrong_name
        collector_command += ["--config", str(overlay)]
    run([collector_command[0], "validate", *collector_command[1:]], collector_environment,
        cwd=collector_cwd)
    label = failure or mode
    print(f"collector: {'mTLS ' if tls else ''}{label} recipe validates", flush=True)

    def receiver_request(method, body=None):
        return http(receiver_port, method, "/v1/traces", body, context, server_name)

    worker_command = [str(args.worker), "--state-dir", str(root / "state"),
                      "--listen.clients", str(root / "api.sock"),
                      "--name", "private-worker-label-marker",
                      "--observability.enabled", "true", "--observability.bind",
                      f"127.0.0.1:{worker_port}", "--tracing.enabled",
                      "false" if mode == "disabled" else "true",
                      "--tracing.endpoint", f"{'https' if tls else 'http'}://127.0.0.1:{receiver_port}/v1/traces",
                      "--tracing.sample-ppm", "0" if mode == "zero" else "1000000",
                      "--tracing.batch-size", "1", "--tracing.export-timeout-ms", "1000",
                      "--tracing.shutdown-timeout-ms", "100"]
    if tls:
        worker_command += tls.worker_arguments(failure)
    with contextlib.ExitStack() as collectors:
        collector = collectors.enter_context(process(collector_command, collector_environment, collector_cwd))
        # GET cannot add a span to this trace-only pipeline.
        poll(lambda: receiver_request("GET")[0] == 405, [collector])
        assert receiver_request("POST", b"\xff")[0] == 400
        if tls:
            assert http(receiver_port, "GET", "/v1/traces")[0] == 400, "plaintext accepted at TLS receiver"
            expect_tls_alert(lambda: http(receiver_port, "GET", "/v1/traces",
                                         context=tls.client_context(None), server_name=server_name),
                             "TLSV13_ALERT_CERTIFICATE_REQUIRED")
            old_tls = tls.client_context()
            old_tls.minimum_version = old_tls.maximum_version = ssl.TLSVersion.TLSv1_2
            expect_tls_alert(lambda: http(receiver_port, "GET", "/v1/traces",
                                         context=old_tls, server_name=server_name),
                             "TLSV1_ALERT_PROTOCOL_VERSION")
        with process(worker_command, environment) as worker:
            poll(lambda: http(worker_port, "GET", "/readyz")[0] == 200, [worker, collector])

            def operator(arguments, authenticated=False, success=True):
                command = [str(args.ctl), "--host", str(root / "api.sock"),
                           "--output", "json", "--timeout", "2s"]
                if authenticated:
                    command += ["--operator-token-file", str(root / "state/operator.token")]
                output = run(command + arguments, environment, success)
                return json.loads(output) if success else None

            original = operator(["cluster", "info"])
            assert original["locked"] is False
            token = (root / "state/operator.token").read_bytes().strip()
            forbidden = (token, b"private-worker-label-marker", b"collector-outage-lock",
                         b"collector-recovered-unlock", b"BEGIN PRIVATE KEY")
            if tls:
                forbidden += tls.forbidden_material()

            def current(expected):
                value = operator(["cluster", "info"])
                assert value["formationId"] == original["formationId"]
                assert value["locked"] is expected
                assert http(worker_port, "GET", "/readyz")[0] == 200

            if failure:
                receipt = operator(["cluster", "lock", "--formation-id", original["formationId"],
                                    "--operation-id", "collector-outage-lock"], True)
                assert receipt["locked"] is True
                current(True)
                poll(lambda: trace_count(worker_port, "failed") == 3, [worker, collector])
                assert trace_count(worker_port, "accepted") == 0
                assert not read_spans(receipt_path, forbidden)
                assert receiver_request("GET")[0] == 405, "trusted receiver control stopped working"
                print(f"collector: {failure} refused; authenticated control stays ready", flush=True)
            elif mode != "enabled":
                current(False)
                observed = metrics(worker_port)
                if mode == "disabled":
                    assert not any(key.startswith("orishu_worker_trace_") for key in observed)
                else:
                    assert trace_count(worker_port, "sampled_out") == 2
                    assert trace_count(worker_port, "enqueued") == 0
            else:
                # Rejected authority is observable but never grants a mutation.
                operator(["cluster", "lock", "--formation-id", original["formationId"],
                          "--operation-id", "unauthenticated-lock"], success=False)
                current(False)
                poll(lambda: len(read_spans(receipt_path, forbidden)) == 3
                     and trace_count(worker_port, "accepted") == 3, [worker, collector])
                first = read_spans(receipt_path, forbidden)
                assert sorted(span["attributes"][0]["value"]["stringValue"] for span in first) == [
                    "completed", "completed", "rejected"]
                collectors.close()  # Actual stopped/reaped collector, not a simulated 503.
                assert collector.poll() == 0
                receipt = operator(["cluster", "lock", "--formation-id", original["formationId"],
                                    "--operation-id", "collector-outage-lock"], True)
                assert receipt["locked"] is True
                current(True)
                poll(lambda: trace_count(worker_port, "failed") == 2, [worker])
                assert trace_count(worker_port, "accepted") == 3
                assert len(read_spans(receipt_path, forbidden)) == 3
                print("collector: stopped receiver; authenticated lock and readiness pass", flush=True)
                # A distinct destination file prevents old receipts proving recovery.
                receipt_path = root / "recovered.jsonl"
                collector_environment["ORISHU_OTEL_TRACE_FILE"] = str(receipt_path)
                recovered = collectors.enter_context(process(collector_command, collector_environment, collector_cwd))
                poll(lambda: receiver_request("GET")[0] == 405, [worker, recovered])
                receipt = operator(["cluster", "unlock", "--formation-id", original["formationId"],
                                    "--operation-id", "collector-recovered-unlock"], True)
                assert receipt["locked"] is False
                current(False)
                poll(lambda: len(read_spans(receipt_path, forbidden)) == 2
                     and trace_count(worker_port, "accepted") == 5, [worker, recovered])
                second = read_spans(receipt_path, forbidden)
                assert all(span["attributes"][0]["value"]["stringValue"] == "completed"
                           for span in second)
                assert {span["traceId"] for span in first}.isdisjoint(
                    {span["traceId"] for span in second})
                assert trace_count(worker_port, "failed") == 2
                for name in ("active_full", "queue_full", "encoding_dropped", "invalid_source",
                             "rejected", "shutdown_dropped"):
                    assert trace_count(worker_port, name) == 0
                print("collector: restarted receiver; fresh receipts and unlock pass", flush=True)
        # Worker shutdown/drain precedes the collector's shutdown, including off modes.
        assert worker.poll() == 0 and not (root / "api.sock").exists()
    received = read_spans(receipt_path, forbidden, complete=True)
    assert len(received) == (2 if mode == "enabled" and not failure else 0)
    if mode == "enabled" and not failure:
        # Exercise the manual operator inspection command too, not only its helper.
        result = run(["python3", str(Path(__file__).resolve()), "--inspect-traces", str(receipt_path)],
                     environment)
        assert result.startswith(b"Validated 2 local client-service spans;")
    print(f"collector: {'mTLS ' if tls else ''}{label} receipt/secret checks and clean shutdown pass", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, default=ROOT / "target/debug/orishu-worker")
    parser.add_argument("--ctl", type=Path, default=ROOT / "target/debug/orishuctl")
    parser.add_argument("--otelcol", type=Path)
    parser.add_argument("--mtls", action="store_true",
                        help="use the mTLS overlay and exercise real worker certificate refusals")
    parser.add_argument("--inspect-traces", type=Path,
                        help="check a stopped local recipe's JSONL file without starting processes")
    args = parser.parse_args()
    if args.inspect_traces:
        assert args.inspect_traces.is_file(), "receipt file does not exist"
        spans = read_spans(args.inspect_traces, complete=True)
        print(f"Validated {len(spans)} local client-service spans; not domain receipts or peer traces")
        return
    if args.otelcol is None:
        parser.error("--otelcol is required for the process journey")
    assert os.name == "posix", "Unix process/socket recipe required"
    for name in ("worker", "ctl", "otelcol"):
        value = getattr(args, name)
        setattr(args, name, Path(shutil.which(str(value)) or value).resolve())
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("ORISHU_", "OTEL_"))}
    assert run([str(args.otelcol), "--version"], environment).strip() == f"otelcol version {VERSION}".encode()
    with tempfile.TemporaryDirectory(prefix="orishu-collector-") as temporary:
        root = Path(temporary)
        # A deliberately bad file proves 'validate' is actually enforcing the schema.
        invalid = root / "invalid.yml"
        invalid.write_text("unknown_component: true\n")
        run([str(args.otelcol), "validate", "--config", str(invalid)], environment, success=False)
        tls = None
        if args.mtls:
            from worker_otelcol_tls import CollectorTLS
            tls = CollectorTLS(root, run, environment)
            collector_tls_startup_failures(root / "startup-refusal", args, environment, tls)
        for mode in ("disabled", "zero", "enabled"):
            scenario(mode, root / mode, args, environment, tls)
        if tls:
            for failure in ("missing-client", "wrong-client-ca", "wrong-server-ca", "wrong-server-name"):
                scenario("enabled", root / failure, args, environment, tls, failure)
    print(f"PASS: Collector {VERSION}; {'mTLS' if args.mtls else 'local HTTP'} "
          "disabled/zero/enabled, outage/recovery and redaction")


if __name__ == "__main__":
    def expired(_signum, _frame):
        raise TimeoutError("collector journey exceeded 120-second whole-run budget")

    signal.signal(signal.SIGALRM, expired)
    signal.alarm(120)
    try:
        main()
    finally:
        signal.alarm(0)
