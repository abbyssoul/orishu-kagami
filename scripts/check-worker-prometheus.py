#!/usr/bin/env python3
"""Bounded local worker/promtool/Prometheus acceptance; no Python dependencies."""
import argparse
import contextlib
import http.client
import json
import math
import os
from pathlib import Path
import socket
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
from worker_trace_collector import trace_collector


VERSION = "3.5.0"
ROOT = Path(__file__).resolve().parent.parent
GAUGES = {"owner_responsive", "ready", "startup_complete"}
COUNTERS = {
    "swim_packets_received_total", "anti_entropy_packets_received_total",
    "gossip_items_received_total",
    "peer_decode_rejections_total",
    "admissions_accepted_total", "admissions_rejected_total",
    "admission_assignment_replays_total",
    "membership_transitions_total", "stale_inputs_total", "core_diagnostics_total",
    "foreign_gossip_total", "reliable_replies_total", "send_failures_total",
    "client_requests_completed_total", "client_requests_rejected_total",
    "client_requests_failed_total", "client_requests_cancelled_total",
    "client_request_duration_seconds_total",
}
COUNTERS |= {f"peer_inbound_{name}_total" for name in (
    "tls_completed", "tls_failed", "tls_timed_out", "tls_cancelled",
    "handshake_completed", "handshake_failed", "handshake_timed_out", "handshake_cancelled",
    "tls_capacity_refused", "connection_capacity_refused")}
CLIENT_GAUGES = {"client_requests_in_flight"}
COUNTERS |= {f"peer_reliable_{role}_{outcome}_total"
             for role in ("request", "serve")
             for outcome in ("completed", "failed", "timed_out", "capacity_refused", "cancelled")}
COUNTERS |= {"peer_reliable_bytes_sent_total", "peer_reliable_bytes_received_total"}
PACKET_COUNTERS = {"peer_" + name for name in (
    "datagrams_submitted_total", "datagrams_submit_refused_total", "datagrams_submit_failed_total",
    "datagrams_received_total", "datagrams_oversized_total", "datagram_bytes_submitted_total",
    "datagram_bytes_received_total", "stream_capacity_refused_total")}
COUNTERS |= PACKET_COUNTERS
FORMATION_COUNTERS = {"membership_" + name for name in (
    "direct_probe_deadlines_total", "indirect_probe_deadlines_total", "suspicion_deadlines_total",
    "anti_entropy_deadlines_total", "join_retry_due_total", "stale_timer_inputs_total",
    "join_abandoned_total", "anti_entropy_abandoned_total")}
COUNTERS |= FORMATION_COUNTERS
COUNTERS |= {"catchup_" + suffix + "_total" for suffix in (
    "started", "transfer_validated", "transfer_binding_rejected", "transfer_invalid",
    "transfer_rejected", "transfer_unavailable", "transfer_timed_out", "transfer_cancelled",
    "owner_adopted", "owner_not_adopted", "owner_fenced", "owner_abandoned")}
COUNTERS |= {f"peer_outbound_{stage}_{outcome}_total"
             for stage in ("attempt", "tls")
             for outcome in ("completed", "failed", "timed_out", "cancelled")}
COUNTERS.add("peer_outbound_capacity_refused_total")
OUTBOUND_GAUGES = {"peer_outbound_slots_in_use", "peer_outbound_slots_capacity"}
INBOUND_GAUGES = {f"peer_inbound_{stage}_slots_{field}"
                  for stage in ("tls", "connection") for field in ("in_use", "capacity")}
REGISTRY_GAUGES = {f"peer_registry_{prefix}slots_{field}"
                   for prefix in ("", "provisional_") for field in ("in_use", "capacity")}
RELIABLE_GAUGES = {"peer_reliable_slots_in_use", "peer_reliable_slots_capacity"}
LANES = {"peer": 64, "control": 16, "completion": 64, "shutdown": 1}
SLOTS = {f"{lane}_slots_{suffix}" for lane in LANES for suffix in ["in_use", "capacity"]}
NAMES = {"orishu_worker_" + name for name in GAUGES | CLIENT_GAUGES | RELIABLE_GAUGES | OUTBOUND_GAUGES | INBOUND_GAUGES | REGISTRY_GAUGES | COUNTERS | SLOTS}
HISTOGRAM = "orishu_worker_client_request_duration_seconds"
HISTOGRAMS = {HISTOGRAM} | {f"orishu_worker_peer_reliable_{role}_duration_seconds"
                           for role in ("request", "serve")}
HISTOGRAMS |= {f"orishu_worker_peer_outbound_{stage}_duration_seconds" for stage in ("attempt", "tls")}
BUCKETS = ("0.001", "0.005", "0.025", "0.1", "0.5", "1", "5", "+Inf")
SAMPLE_KEYS = NAMES | {f'{name}_bucket{{le="{bound}"}}' for name in HISTOGRAMS for bound in BUCKETS} | {
    name + suffix for name in HISTOGRAMS for suffix in ("_sum", "_count")}
NAMES |= {name + suffix for name in HISTOGRAMS for suffix in ("_bucket", "_sum", "_count")}
TRACE_NAMES = {f"orishu_worker_trace_{name}_total" for name in (
    "sampled_out", "active_full", "queue_full", "closed", "invalid_source", "enqueued",
    "accepted", "rejected", "failed", "encoding_dropped", "shutdown_dropped", "warnings")}
LOG_NAMES = {f"orishu_worker_log_{name}_total" for name in (
    "accepted", "written", "queue_full", "contended", "encoding_failed",
    "invalid_source", "output_failed", "closed", "shutdown_dropped")}
FORMATION_ALERT_NAMES = {
    "OrishuWorkerOwnerUnresponsive", "OrishuWorkerAdmissionRefusals",
    "OrishuWorkerCatchupTransferFailures", "OrishuWorkerMembershipAbandoned",
    "OrishuWorkerPeerTimeouts", "OrishuWorkerCapacityExhausted",
}


def check_histogram(buckets, count, minimum=0):
    assert set(buckets) == {float(bound) for bound in BUCKETS}
    values = [buckets[float(bound)] for bound in BUCKETS]
    assert all(value.is_integer() and value >= 0 for value in values)
    assert values == sorted(values), "histogram buckets must be cumulative"
    assert values[-1] == count and count >= minimum, "histogram count does not match observed work"


def logging_ingested(values, closed_output=False, minimum=None):
    """Validate finite log counters and await observable writes or sink failure.

    Minimum is a named post-scraper-outage counter/value; old TSDB samples must
    not establish new ingestion. Independent live counters are not transactional.
    """
    counters = {name: values[name] for name in LOG_NAMES}
    assert all(math.isfinite(value) and value >= 0 and float(value).is_integer()
               for value in counters.values()), "invalid logging counter"
    prefix = "orishu_worker_log_"
    for suffix in ("queue_full", "contended", "encoding_failed", "invalid_source", "shutdown_dropped"):
        assert counters[prefix + suffix + "_total"] == 0, "unexpected local logging loss"
    if closed_output:
        assert counters[prefix + "written_total"] == 0, "closed pipe acknowledged a write"
        failures = counters[prefix + "output_failed_total"]
        assert failures <= 1, "terminal sink failure retried"
        ready = failures == 1
    else:
        assert counters[prefix + "output_failed_total"] == counters[prefix + "closed_total"] == 0
        ready = counters[prefix + "written_total"] >= 1
    if minimum is not None:
        name, count = minimum
        assert name in LOG_NAMES and math.isfinite(count) and count >= 0
        ready = ready and counters[name] >= count
    return ready and counters[prefix + "accepted_total"] >= 1


def run(command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, timeout=10, **kwargs)


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


@contextlib.contextmanager
def process(command, environment=None, closed_stdout=False):
    # Never print potentially secret-bearing worker output on failure.
    with contextlib.ExitStack() as handles:
        output = subprocess.DEVNULL
        if closed_stdout:
            read_fd, write_fd = os.pipe()
            os.close(read_fd)
            output = handles.enter_context(os.fdopen(write_fd, "wb", buffering=0))
        child = subprocess.Popen(command, env=environment, stdout=output,
                                 stderr=subprocess.DEVNULL)
    try:
        yield child
    finally:
        was_running = child.poll() is None
        if was_running:
            child.terminate()
        try:
            code = child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=3)
            raise AssertionError("process exceeded graceful shutdown budget")
        if was_running and code != 0:
            raise AssertionError(f"process failed graceful shutdown: {code}")


def get(listen_port, path, limit):
    connection = http.client.HTTPConnection("127.0.0.1", listen_port, timeout=1)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        body = response.read(limit + 1)
        assert len(body) <= limit, "response exceeded byte budget"
        return response.status, dict(response.getheaders()), body
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
        except (OSError, http.client.HTTPException):
            pass
        time.sleep(0.05)
    raise AssertionError("observation deadline expired")


def transitions(listen_port, limit=32768):
    status, _, body = get(listen_port, "/metrics", limit)
    assert status == 200
    prefix = "orishu_worker_membership_transitions_total "
    return int(next(line[len(prefix):] for line in body.decode("ascii").splitlines()
                    if line.startswith(prefix)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, default=ROOT / "target/debug/orishu-worker")
    parser.add_argument("--ctl", type=Path, default=ROOT / "target/debug/orishuctl")
    parser.add_argument("--promtool", type=Path, required=True)
    parser.add_argument("--prometheus", type=Path, required=True)
    parser.add_argument("--trace-metrics", action="store_true",
                        help="also ingest live trace counters across collector failure/recovery")
    parser.add_argument("--log-metrics", action="store_true",
                        help="also ingest independent structured-stdout logging counters")
    parser.add_argument("--closed-log-output", action="store_true",
                        help="with --log-metrics, exercise a real stdout pipe with no reader")
    parser.add_argument("--formation-alerts", action="store_true",
                        help="also validate/load six optional formation-stage alert examples")
    args = parser.parse_args()
    if args.closed_log_output and not args.log_metrics:
        parser.error("--closed-log-output requires --log-metrics")
    optional = (TRACE_NAMES if args.trace_metrics else set()) | (LOG_NAMES if args.log_metrics else set())
    sample_keys = SAMPLE_KEYS | optional
    names = NAMES | optional
    scrape_limit = 32768
    args.promtool = Path(shutil.which(str(args.promtool)) or args.promtool).resolve()
    args.prometheus = Path(shutil.which(str(args.prometheus)) or args.prometheus).resolve()
    for tool in [args.promtool, args.prometheus]:
        version = run([str(tool.resolve()), "--version"], text=True)
        assert f"version {VERSION} " in version.stdout + version.stderr, "unexpected test-tool version"
    invalid = subprocess.run([str(args.promtool), "check", "metrics"],
                             input=b'broken{label="unterminated\n',
                             capture_output=True, timeout=10, check=False)
    assert invalid.returncode != 0, "validator accepted deliberately malformed exposition"
    with tempfile.TemporaryDirectory(prefix="orishu-prometheus-") as temporary, contextlib.ExitStack() as fixtures:
        root = Path(temporary)
        collector = fixtures.enter_context(trace_collector()) if args.trace_metrics else None
        worker_port, server_port = port(), port()
        while server_port == worker_port:
            server_port = port()
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith("ORISHU_")}
        worker_command = [str(args.worker.resolve()), "--state-dir", str(root / "state"),
                          "--listen.clients", str(root / "api.sock"),
                          "--observability.enabled", "true", "--observability.bind",
                          f"127.0.0.1:{worker_port}"]
        if collector:
            worker_command += ["--tracing.enabled", "true", "--tracing.endpoint", collector.endpoint,
                               "--tracing.sample-ppm", "1000000", "--tracing.batch-size", "1",
                               "--tracing.export-max-bytes", "1024", "--tracing.export-timeout-ms", "1000",
                               "--tracing.shutdown-timeout-ms", "100"]
        if args.log_metrics:
            worker_command += ["--logging.enabled", "true", "--logging.queue-records", "256",
                               "--logging.shutdown-ms", "250"]
        with process(worker_command, environment, closed_stdout=args.closed_log_output) as worker:
            poll(lambda: get(worker_port, "/readyz", 1024)[0] == 200, [worker])

            def operator(arguments, authenticated=False):
                command = [str(args.ctl.resolve()), "--host", str(root / "api.sock"),
                           "--output", "json", "--timeout", "2s"]
                if authenticated:
                    command += ["--operator-token-file", str(root / "state/operator.token")]
                return json.loads(run(command + arguments, env=environment).stdout)

            original = operator(["cluster", "info"])
            assert original["locked"] is False
            original_member = operator(["inspect", original["sourceNodeId"]])

            def incident_reads():
                # The runbook uses only reads to separate telemetry outage
                # from domain health. Mutations below are test controls only.
                current = operator(["cluster", "info"])
                assert current["formationId"] == original["formationId"]
                assert current["sourceNodeId"] == original["sourceNodeId"]
                assert current["participation"] == "standalone"
                members = operator(["ls"])
                assert len(members) == 1 and members[0]["nodeId"] == original["sourceNodeId"]
                member = operator(["inspect", original["sourceNodeId"]])
                for field in ("formationId", "sourceNodeId", "nodeId", "certFingerprint", "view"):
                    assert member[field] == original_member[field], "outage changed inspected identity/source"
                assert member["liveness"] == "alive"
                for route in ("livez", "readyz", "startupz"):
                    status, headers, body = get(worker_port, "/" + route, 1024)
                    assert status == 200 and body == b"ok\n"
                    assert headers["cache-control"] == "no-store"

            incident_reads()
            status, headers, body = get(worker_port, "/metrics", scrape_limit)
            assert status == 200
            assert headers["content-type"] == "text/plain; version=0.0.4; charset=utf-8"
            assert headers["cache-control"] == "no-store"
            token = (root / "state/operator.token").read_bytes().strip()
            assert token and token not in body
            run([str(args.promtool.resolve()), "check", "metrics"], input=body)
            # Cross-check catalogue in addition to the official parser/linter.
            samples = [line.split() for line in body.decode("ascii").splitlines()
                       if line and not line.startswith("#")]
            assert len(samples) == len(sample_keys)
            assert {sample[0] for sample in samples} == sample_keys
            assert all(len(sample) == 2 and math.isfinite(float(sample[1]))
                       and float(sample[1]) >= 0 for sample in samples)
            sample_values = {name: float(value) for name, value in samples}
            for name in HISTOGRAMS:
                check_histogram({float(bound): sample_values[f'{name}_bucket{{le="{bound}"}}']
                                 for bound in BUCKETS}, sample_values[name + "_count"],
                                minimum=int(name == HISTOGRAM))
            assert sample_values[HISTOGRAM + "_sum"] == sample_values[HISTOGRAM + "_total"]
            # No peer listener is enabled in this fixture: measured zeros are
            # valid, and the process-wide exchange capacity is still real.
            assert sample_values["orishu_worker_peer_reliable_slots_capacity"] == 64
            assert sample_values["orishu_worker_peer_reliable_slots_in_use"] == 0
            assert sample_values["orishu_worker_peer_outbound_slots_capacity"] == 4
            # This fixture configures no peer listener: no inbound budget exists.
            assert all(sample_values["orishu_worker_" + name] == 0 for name in INBOUND_GAUGES)
            # The owner registry exists even when no inbound listener runs.
            for name, expected in (("slots_in_use", 0), ("slots_capacity", 64),
                                   ("provisional_slots_in_use", 0), ("provisional_slots_capacity", 16)):
                assert sample_values["orishu_worker_peer_registry_" + name] == expected
            assert all(sample_values["orishu_worker_" + name] == 0 for name in PACKET_COUNTERS)
            assert all(sample_values["orishu_worker_" + name] == 0 for name in FORMATION_COUNTERS)
            for name in SAMPLE_KEYS:
                if name.startswith(("orishu_worker_peer_reliable_", "orishu_worker_peer_outbound_")) and not name.endswith("slots_capacity"):
                    assert sample_values[name] == 0
            config = root / "prometheus.yml"
            rules = ROOT / "etc/prometheus-worker-alerts.yml"
            run([str(args.promtool), "check", "rules", str(rules)])
            run([str(args.promtool), "test", "rules",
                 str(ROOT / "etc/prometheus-worker-alerts.test.yml")])
            shutil.copyfile(rules, root / rules.name)
            example = (ROOT / "etc/prometheus-local.yml").read_text()
            if collector:
                trace_rules = ROOT / "etc/prometheus-worker-trace-alerts.yml"
                run([str(args.promtool), "check", "rules", str(trace_rules)])
                run([str(args.promtool), "test", "rules",
                     str(ROOT / "etc/prometheus-worker-trace-alerts.test.yml")])
                shutil.copyfile(trace_rules, root / trace_rules.name)
                rule_entry = "  - prometheus-worker-alerts.yml"
                assert example.count(rule_entry) == 1
                example = example.replace(rule_entry, rule_entry + "\n  - " + trace_rules.name)
            if args.formation_alerts:
                formation_rules = ROOT / "etc/prometheus-worker-formation-alerts.yml"
                run([sys.executable, str(ROOT / "scripts/test_worker_formation_alerts.py"),
                     "--promtool", str(args.promtool)])
                shutil.copyfile(formation_rules, root / formation_rules.name)
                rule_entry = "  - prometheus-worker-alerts.yml"
                assert example.count(rule_entry) == 1
                example = example.replace(rule_entry, rule_entry + "\n  - " + formation_rules.name)
            assert example.count("127.0.0.1:9168") == 1
            config.write_text(example.replace("127.0.0.1:9168", f"127.0.0.1:{worker_port}"))
            run([str(args.promtool.resolve()), "check", "config", str(config)])
            server_command = [str(args.prometheus.resolve()), "--config.file", str(config),
                              "--web.listen-address", f"127.0.0.1:{server_port}",
                              "--storage.tsdb.path", str(root / "tsdb"),
                              "--storage.tsdb.retention.time", "30m"]
            with process(server_command) as server:
                query = urllib.parse.urlencode({"query": '{__name__=~"orishu_worker_.+"}'})

                def ingested(minimum_transitions=0, trace_phase=None, minimum_log=None):
                    if collector:
                        collector.check()
                    status, _, response = get(server_port, "/api/v1/query?" + query, 65536)
                    if status != 200:
                        return False
                    document = json.loads(response)
                    assert document["status"] == "success"
                    assert document["data"]["resultType"] == "vector"
                    series = document["data"]["result"]
                    if not series:
                        return False
                    assert len(series) == len(sample_keys)
                    assert {item["metric"]["__name__"] for item in series} == names
                    values = {item["metric"]["__name__"]: float(item["value"][1]) for item in series}
                    buckets = {name: {} for name in HISTOGRAMS}
                    for item in series:
                        labels = item["metric"]
                        family = labels["__name__"].removesuffix("_bucket")
                        is_bucket = family in HISTOGRAMS
                        assert set(labels) == ({"__name__", "job", "instance", "le"}
                                               if is_bucket else {"__name__", "job", "instance"})
                        assert labels["job"] == "orishu-worker"
                        assert labels["instance"] == f"127.0.0.1:{worker_port}"
                        value = float(item["value"][1])
                        assert math.isfinite(value) and value >= 0
                        if is_bucket:
                            bound = float(labels["le"])
                            assert bound not in buckets[family]
                            buckets[family][bound] = value
                        for lane, capacity in LANES.items():
                            if labels["__name__"] == f"orishu_worker_{lane}_slots_capacity":
                                assert value == capacity
                            if labels["__name__"] == f"orishu_worker_{lane}_slots_in_use":
                                assert value <= capacity and value.is_integer()
                        if labels["__name__"] in {"orishu_worker_" + name for name in GAUGES}:
                            assert value == 1
                        if labels["__name__"] == "orishu_worker_client_requests_completed_total":
                            assert value >= 1, "real operator requests must be measured"
                        if labels["__name__"] == "orishu_worker_membership_transitions_total":
                            if value < minimum_transitions:
                                return False
                    for name in HISTOGRAMS:
                        check_histogram(buckets[name], values[name + "_count"],
                                        minimum=int(name == HISTOGRAM))
                    if args.log_metrics and not logging_ingested(values, args.closed_log_output, minimum_log):
                        return False
                    if trace_phase:
                        if values["orishu_worker_trace_failed_total"] < 1:
                            return False
                        accepted = values["orishu_worker_trace_accepted_total"]
                        if trace_phase == "failed":
                            assert accepted == 0, "collector must not accept before recovery"
                        elif accepted < 1:
                            return False
                    return True

                poll(lambda: ingested(trace_phase="failed" if collector else None), [worker, server])
                if collector:
                    # Generate an authenticated domain operation while all
                    # export attempts fail, then recover only the collector.
                    receipt = operator(["cluster", "lock", "--formation-id", original["formationId"],
                                        "--operation-id", "collector-outage-lock"], True)
                    assert receipt["locked"] is True
                    current = operator(["cluster", "info"])
                    assert current["formationId"] == original["formationId"] and current["locked"] is True
                    assert get(worker_port, "/readyz", 1024)[0] == 200
                    incident_reads()
                    collector.recover()
                    receipt = operator(["cluster", "unlock", "--formation-id", original["formationId"],
                                        "--operation-id", "collector-recovered-unlock"], True)
                    assert receipt["locked"] is False
                    # Pre-recovery TSDB samples have accepted=0 and cannot
                    # satisfy this check; require fresh ingested delivery.
                    poll(lambda: ingested(trace_phase="recovered"), [worker, server])
                status, _, response = get(server_port, "/api/v1/rules", 65536)
                assert status == 200
                groups = json.loads(response)["data"]["groups"]
                by_name = {group["name"]: group for group in groups}
                expected_groups = {"orishu-worker-local-v1"}
                if collector:
                    expected_groups.add("orishu-worker-trace-v1")
                if args.formation_alerts:
                    expected_groups.add("orishu-worker-formation-v1")
                assert len(groups) == len(expected_groups) and set(by_name) == expected_groups
                rules = by_name["orishu-worker-local-v1"]["rules"]
                assert {rule["name"] for rule in rules} == {
                    "OrishuWorkerScrapeUnavailable", "OrishuWorkerSustainedUnready",
                    "OrishuWorkerReadinessSeriesMissing",
                }
                assert all(rule["type"] == "alerting" and not rule["alerts"] for rule in rules)
                if collector:
                    trace_rules = by_name["orishu-worker-trace-v1"]["rules"]
                    assert len(trace_rules) == 3
                    assert {rule["name"] for rule in trace_rules} == {
                        "OrishuWorkerTraceLocalDrops", "OrishuWorkerTraceDeliveryFailures",
                        "OrishuWorkerTraceCollectorRejections",
                    }
                    # Rule transitions use promtool's synthetic clock. This
                    # short journey proves loading, not two-minute firing.
                    assert all(rule["type"] == "alerting" and rule["health"] != "err"
                               for rule in trace_rules)
                if args.formation_alerts:
                    formation_rules = by_name["orishu-worker-formation-v1"]["rules"]
                    assert len(formation_rules) == 6
                    assert {rule["name"] for rule in formation_rules} == FORMATION_ALERT_NAMES
                    assert all(rule["type"] == "alerting" and rule["health"] != "err"
                               for rule in formation_rules)
                # Loading rules before their scheduled evaluation does not
                # prove they can evaluate a complete worker's coexisting series.
                # Require two actual samples, then execute every loaded expression
                # through Prometheus without shortening its rule/alert deadlines.
                def two_samples():
                    expression = 'count_over_time(orishu_worker_owner_responsive{job="orishu-worker"}[5m]) >= 2'
                    query = urllib.parse.urlencode({"query": expression})
                    status, _, response = get(server_port, "/api/v1/query?" + query, 4096)
                    assert status == 200
                    result = json.loads(response)
                    assert result["status"] == "success"
                    return len(result["data"]["result"]) == 1

                poll(two_samples, [worker, server])
                for group in groups:
                    for rule in group["rules"]:
                        rule_query = urllib.parse.urlencode({"query": rule["query"]})
                        status, _, response = get(server_port, "/api/v1/query?" + rule_query, 16384)
                        assert status == 200, f"live expression failed: {rule['name']}"
                        result = json.loads(response)
                        assert result["status"] == "success" and result["data"]["resultType"] == "vector"
                        assert not result.get("warnings"), f"live expression warning: {rule['name']}"
                        if group["name"] != "orishu-worker-trace-v1":
                            assert result["data"]["result"] == [], "healthy idle formation must not alert"
                assert get(worker_port, "/readyz", 1024)[0] == 200
            # The context manager has stopped and reaped the actual scraper.
            assert server.poll() == 0
            log_minimum = None
            if args.log_metrics and collector:
                # With full tracing, subsequent operator reads generate log
                # records even while Prometheus is stopped. A terminal pipe
                # instead increments closure refusals without retrying output.
                log_key = "orishu_worker_log_" + ("closed_total" if args.closed_log_output else "written_total")

                def log_count():
                    status, _, body = get(worker_port, "/metrics", scrape_limit)
                    assert status == 200
                    return float(next(line.split()[1] for line in body.decode("ascii").splitlines()
                                      if line.startswith(log_key + " ")))

                before_log = log_count()
            incident_reads()
            before = transitions(worker_port, scrape_limit)
            for action, expected in [("lock", True), ("unlock", False)]:
                receipt = operator(["cluster", action, "--formation-id", original["formationId"],
                                    "--operation-id", "scraper-outage-" + action], True)
                assert receipt["locked"] is expected
                current = operator(["cluster", "info"])
                assert current["formationId"] == original["formationId"]
                assert current["locked"] is expected
                assert get(worker_port, "/readyz", 1024)[0] == 200
            after = poll(lambda: value if (value := transitions(worker_port, scrape_limit)) > before else None,
                         [worker])
            if args.log_metrics and collector:
                after_log = poll(lambda: value if (value := log_count()) > before_log else None, [worker])
                log_minimum = log_key, after_log
            # Require ingestion of a counter value reached only while the
            # scraper was absent; retained pre-outage TSDB samples cannot pass.
            with process(server_command) as recovered:
                poll(lambda: ingested(after, "recovered" if collector else None, log_minimum), [worker, recovered])
                assert operator(["cluster", "info"])["formationId"] == original["formationId"]
                assert get(worker_port, "/readyz", 1024)[0] == 200
                incident_reads()
    alert_count = 3 + (3 if args.trace_metrics else 0) + (6 if args.formation_alerts else 0)
    print(f"PASS: Prometheus {VERSION}; {len(sample_keys)} series and {alert_count} live-evaluated alert expressions; "
          f"trace collector failure/recovery {'verified' if args.trace_metrics else 'not selected'}; "
          f"logging {'closed pipe' if args.closed_log_output else 'discard sink' if args.log_metrics else 'disabled'}; "
          "operator control survives scraper outage and re-scrape")


if __name__ == "__main__":
    main()
