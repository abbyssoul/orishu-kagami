#!/usr/bin/env python3
"""Real A-admits-B/B-admits-C CLI journey with official Collector and stdout logs."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import threading

from worker_formation_receipts import COUNTERS, MAX_LOG_BYTES, collector as check, correlate, read_logs, join_phase_complete


@contextlib.contextmanager
def recorded_worker(command, environment, path):
    """Drain continuously but persist at most the capture cap; overflow fails."""
    reader = None
    result = []
    try:
        with check.process(command, environment, stdout=subprocess.PIPE) as child:
            def drain():
                total = 0
                try:
                    with path.open("xb") as output:
                        while data := child.stdout.read1(4096):
                            keep = max(0, MAX_LOG_BYTES - total)
                            output.write(data[:keep])
                            total += len(data)
                        output.flush()
                    result.append(total <= MAX_LOG_BYTES)
                except BaseException:
                    result.append(False)
                finally:
                    child.stdout.close()
            reader = threading.Thread(target=drain, daemon=True)
            reader.start()
            yield child
    finally:
        if reader is not None and reader.ident is not None:
            reader.join(timeout=1)
            assert not reader.is_alive(), "worker stdout reader did not finish"
            assert result == [True], "worker stdout capture failed or overflowed"


def journey(root, args, environment):
    receiver = check.port()
    ports = []
    while len(ports) < 3:
        candidate = check.port()
        if candidate != receiver and candidate not in ports:
            ports.append(candidate)
    config = root / "collector.yml"
    config.write_text((check.ROOT / "etc/otelcol-worker-local.yml").read_text().replace(
        "127.0.0.1:4318", f"127.0.0.1:{receiver}"))
    receipt = root / "received.jsonl"
    collector_environment = dict(environment, ORISHU_OTEL_TRACE_FILE=str(receipt), GOMEMLIMIT="100MiB")
    collector_command = [str(args.otelcol), "--config", str(config)]
    check.run([str(args.otelcol), "validate", "--config", str(config)], collector_environment)
    commands = [0, 0, 0]
    nodes = [root / role for role in ("a", "b", "c")]
    logs = [node / "stdout.jsonl" for node in nodes]
    forbidden = [b"private-worker-label-marker", b"BEGIN PRIVATE KEY", b"collector-admit-b", b"collector-admit-c"]

    def operator(role, arguments):
        commands[role] += 1
        assert commands[role] <= 64, "operator command count exceeded per-worker budget"
        command = [str(args.ctl), "--host", str(nodes[role] / "api.sock"), "--output", "json",
                   "--timeout", "2s", "--operator-token-file", str(nodes[role] / "state/operator.token")]
        raw = check.run(command + arguments, environment)
        return json.loads(raw, object_pairs_hook=check.unique_object)

    with check.process(collector_command, collector_environment) as collector:
        children = [collector]
        check.poll(lambda: check.http(receiver, "GET", "/v1/traces")[0] == 405, children)
        with contextlib.ExitStack() as workers:
            for role, node in enumerate(nodes):
                node.mkdir(mode=0o700)
                command = [str(args.worker), "--state-dir", str(node / "state"),
                           "--listen.clients", str(node / "api.sock"), "--listen.peers", "127.0.0.1:0",
                           "--accepts.peers", "true", "--name", "private-worker-label-marker",
                           "--observability.enabled", "true", "--observability.bind", f"127.0.0.1:{ports[role]}",
                           "--logging.enabled", "true", "--logging.queue-records", "256", "--logging.shutdown-ms", "250",
                           "--tracing.enabled", "true", "--tracing.endpoint", f"http://127.0.0.1:{receiver}/v1/traces",
                           "--tracing.sample-ppm", "1000000", "--tracing.batch-size", "1",
                           "--tracing.export-timeout-ms", "1000", "--tracing.shutdown-timeout-ms", "1000"]
                child = workers.enter_context(recorded_worker(command, environment, logs[role]))
                children.append(child)
                check.poll(lambda: check.http(ports[role], "GET", "/readyz")[0] == 200, children)
                forbidden.append((node / "state/operator.token").read_bytes().strip())
            initial = [operator(role, ["cluster", "info"]) for role in range(3)]
            assert len({view["formationId"] for view in initial}) == 3
            assert len({view["sourceNodeId"] for view in initial}) == 3
            pins = [operator(role, ["inspect", initial[role]["sourceNodeId"]])["certFingerprint"] for role in range(3)]
            assigned = [initial[0]["sourceNodeId"]]
            formation = initial[0]["formationId"]

            for source, target, operation in ((1, 0, "collector-admit-b"), (2, 1, "collector-admit-c")):
                material = operator(target, ["token"])
                assert material["introducerReady"] and material["formationId"] == formation
                assert material["introducerNodeId"] == assigned[target]
                assert material["introducerFingerprint"] == pins[target]
                forbidden.append(material["token"].encode())
                path = root / f"join-{source}.json"
                with path.open("x") as stream:
                    json.dump(material, stream)
                submitted = operator(source, ["join", "--join-material-file", str(path),
                                             "--formation-id", initial[source]["formationId"],
                                             "--operation-id", operation])
                assert submitted["schemaVersion"] == 2

                def joined():
                    status = operator(source, ["join-status", operation])
                    assert status["sourceFormationId"] == initial[source]["formationId"]
                    assert status["sourceNodeId"] == initial[source]["sourceNodeId"]
                    assert status["targetFormationId"] == formation
                    phase = status["state"]["phase"]
                    return status if join_phase_complete(phase) else None

                status = check.poll(joined, children)
                identity = status["state"]["nodeId"]
                assert identity != initial[source]["sourceNodeId"]
                assigned.append(identity)
                view = operator(source, ["cluster", "info"])
                assert view["sourceNodeId"] == identity and view["introducerReady"]
                assert check.http(ports[source], "GET", "/readyz")[0] == 200
                print(f"formation Collector: {'AB'[target]} admitted {'BC'[source - 1]}; catch-up ready", flush=True)

            expected = dict(zip(assigned, pins))
            assert len(expected) == 3

            def converged():
                views = [operator(role, ["ls"]) for role in range(3)]
                return all({node["nodeId"]: node["certFingerprint"] for node in view} == expected
                           and all(node["liveness"] == "alive" for node in view) for view in views)

            check.poll(converged, children)
            for locked, source in ((True, 0), (False, 2)):
                operation = "collector-policy-lock" if locked else "collector-policy-unlock"
                forbidden.append(operation.encode())
                result = operator(source, ["cluster", "lock" if locked else "unlock",
                                          "--formation-id", formation, "--operation-id", operation])
                assert result["locked"] is locked

                def visible():
                    views = [operator(role, ["cluster", "info"]) for role in range(3)]
                    for role, view in enumerate(views):
                        assert view["formationId"] == formation and view["sourceNodeId"] == assigned[role], "policy observation identity mismatch"
                        assert view["nodes"] == view["alive"] == 3 and view["introducerReady"], "policy observation membership/readiness mismatch"
                    return all(view["locked"] is locked for view in views)

                check.poll(visible, children)

            # Only diagnostic reads below: stop generating client spans before
            # requiring exporter receipt. A successful scrape is not backend ingestion.
            def delivered():
                observations = [check.metrics(port) for port in ports]
                for values in observations:
                    assert len(values) == 172, "expected current complete optional catalogue"
                    assert values["orishu_worker_ready"] == values["orishu_worker_owner_responsive"] == 1, "preflight diagnostic readiness/owner responsiveness"
                    for family, suffixes in (("trace", ("active_full", "queue_full", "closed", "invalid_source", "rejected", "failed", "encoding_dropped", "shutdown_dropped")),
                                             ("log", ("queue_full", "contended", "encoding_failed", "invalid_source", "output_failed", "closed"))):
                        assert all(values[f"orishu_worker_{family}_{name}_total"] == 0 for name in suffixes), "unexpected telemetry loss in quiet receipt fixture"
                return all(values["orishu_worker_trace_enqueued_total"] == values["orishu_worker_trace_accepted_total"]
                           and values["orishu_worker_trace_enqueued_total"] > 0 for values in observations)

            check.poll(delivered, children)
            for role, port in enumerate(ports):
                for route in ("/livez", "/readyz", "/startupz"):
                    assert check.http(port, "GET", route)[0] == 200, "preflight process probe not healthy"
                read_logs(logs[role], forbidden)
            check.read_spans(receipt, forbidden, admission=True)
        # Workers stop while the collector still accepts their final spans.
    spans = check.read_spans(receipt, forbidden, complete=True, admission=True)
    records = [read_logs(path, forbidden, complete=True) for path in logs]
    total = correlate(spans, records)
    for role, rows in enumerate(records):
        values = [row for row in rows if row["event"] == "orishu.trace.accounting"]
        assert len(values) == 12 and {row["counter"] for row in values} == COUNTERS, "preflight final trace accounting incomplete"
        counts = {row["counter"]: row["value"] for row in values}
        emitted = sum(row["event"] in ("orishu.client.request", "orishu.peer.exchange", "orishu.admission") for row in rows)
        assert counts["accepted"] == counts["enqueued"] == emitted, "preflight final receipt/accounting mismatch"
        assert counts["shutdown_dropped"] == counts["failed"] == 0, "preflight final shutdown/delivery loss"
    print(f"PASS: Collector {check.VERSION}; A→B→C formation; two causal chains; {total} matched spans/logs; 172 series per worker; probes and cross-worker policy pass")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "ctl", "otelcol"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    args = parser.parse_args()
    assert os.name == "posix", "Unix source-build recipe required"
    os.umask(0o077)
    for name in ("worker", "ctl", "otelcol"):
        value = getattr(args, name)
        setattr(args, name, Path(shutil.which(str(value)) or value).resolve())
    environment = {key: value for key, value in os.environ.items() if not key.startswith(("ORISHU_", "OTEL_"))}
    assert check.run([str(args.otelcol), "--version"], environment).strip() == f"otelcol version {check.VERSION}".encode()
    with tempfile.TemporaryDirectory(prefix="orishu-formation-collector-") as temporary:
        journey(Path(temporary), args, environment)


if __name__ == "__main__":
    def expired(_signum, _frame):
        raise TimeoutError("formation Collector journey exceeded 120-second budget")
    signal.signal(signal.SIGALRM, expired)
    signal.alarm(120)
    try:
        main()
    finally:
        signal.alarm(0)
