#!/usr/bin/env python3
"""Real CLI three-worker introducer handoff; the full fault matrix is separate."""

import argparse
import http.client
import json
import os
import signal
import shutil
import socket
import subprocess
import tempfile
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
from pathlib import Path

from formation_evidence import Evidence
from formation_udp_relay import UdpRelay

# Current PoC owner: an outstanding anti-entropy round can outlive its departed
# peer for 10s, then wait for the next 5s cadence. Allow 2s scheduling margin.
# This is a harness observation budget, not a runtime deadline or fleet SLO.
# See peer::readmission_timing_tests and docs/protocol-p2p.md.
READMISSION_CONVERGENCE_SECONDS = 10 + 5 + 2


def snapshot_executable(source, destination):
    """Own restart bytes for this run; never hard-link a mutable build output."""
    shutil.copyfile(source, destination)
    destination.chmod(0o700)


def hold_mutation_body(stack, path, credential):
    """Start a real authenticated body read, then hold it incomplete."""
    slow = stack.enter_context(socket.socket(socket.AF_UNIX, socket.SOCK_STREAM))
    slow.settimeout(1)
    slow.connect(str(path))
    slow.sendall(("POST /api/v1/membership/leaves HTTP/1.1\r\n"
                  "Host: localhost\r\nContent-Type: application/cbor\r\n"
                  "Content-Length: 4096\r\nExpect: 100-continue\r\n"
                  "Authorization: Bearer " + credential + "\r\n\r\n").encode())
    interim = bytearray()
    while not interim.endswith(b"\r\n\r\n"):
        chunk = slow.recv(1)
        assert chunk and len(interim) < 1024, "missing bounded Continue response"
        interim.extend(chunk)
    assert bytes(interim).startswith(b"HTTP/1.1 100 Continue\r\n"), "body not admitted"
    slow.sendall(b"\xa0")
    return slow


def assert_http_rejection(connection, path, body, headers, expected):
    """Require an actual bounded HTTP rejection, not just a transport failure."""
    try:
        connection.request("POST", path, body, headers)
    except BrokenPipeError:
        # A server can reject headers and close its read side before the body
        # is sent. The response may already be queued; this is not itself a
        # passing rejection, and no request is retried.
        pass
    response = connection.getresponse()
    assert response.status == expected, "unexpected inspection rejection"
    assert len(response.read(4097)) <= 4096, "unbounded inspection error"


def discard_one_http_response(listener, upstream_path):
    """Forward one bounded local request; prove success but deliver no response.

    Credentials only pass through memory to the selected harness-owned worker.
    Neither request nor response bytes enter logs or failure artifacts.
    """
    deadline = time.monotonic() + 5

    def receive(connection, limit):
        connection.settimeout(max(0.001, deadline - time.monotonic()))
        assert time.monotonic() < deadline, "response-loss proxy deadline"
        chunk = connection.recv(limit)
        assert chunk, "response-loss proxy unexpected EOF"
        return chunk

    def headers(connection):
        data = b""
        while b"\r\n\r\n" not in data:
            assert len(data) < 8192, "response-loss proxy header limit"
            data += receive(connection, min(1024, 8192 - len(data)))
        return data.split(b"\r\n\r\n", 1)

    listener.settimeout(5)
    client, _ = listener.accept()
    with client, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as upstream:
        head, body = headers(client)
        lengths = [line.split(b":", 1)[1].strip() for line in head.split(b"\r\n")[1:]
                   if line.lower().startswith(b"content-length:")]
        assert len(lengths) == 1 and lengths[0].isdigit(), "expected one bounded request body"
        length = int(lengths[0])
        assert length <= 4096 and len(body) <= length, "response-loss proxy body limit"
        while len(body) < length:
            body += receive(client, length - len(body))
        upstream.settimeout(max(0.001, deadline - time.monotonic()))
        upstream.connect(str(upstream_path))
        upstream.sendall(head + b"\r\n\r\n" + body)
        response, _ = headers(upstream)
        status = response.split(b"\r\n", 1)[0].split(b" ")
        assert len(status) >= 2 and status[1] == b"200", "worker did not accept proxied leave"
        # Closing both sockets discards the response; the CLI receives EOF.
        return 200


def readmission_view_matches(nodes, expected):
    """Match exact identity, certificate and liveness, not labels or counts alone."""
    if not isinstance(nodes, list) or len(nodes) != len(expected):
        return False
    if not all(isinstance(node, dict) and isinstance(node.get("nodeId"), str) for node in nodes):
        return False
    return {node["nodeId"]: (node.get("certFingerprint"), node.get("liveness"))
            for node in nodes} == expected


def assert_incident_member(view, formation, source, node, fingerprint):
    """Check the runbook's read-only identity guards on one local inspection."""
    expected = {"schemaVersion": 1, "formationId": formation, "sourceNodeId": source,
                "nodeId": node, "certFingerprint": fingerprint, "view": "localAtRequest"}
    assert isinstance(view, dict), "incident inspection is not a resource"
    assert all(view.get(key) == value for key, value in expected.items()), \
        "incident inspection identity/source mismatch"
    assert view.get("liveness") in {"alive", "suspected", "dead"}, "unexpected incident liveness"


def capture_readmission_views(invoke, evidence, expected, verb="readmission-diagnostic"):
    """One bounded, best-effort public read per worker after a failed assertion.

    Correlate known identities before normal evidence redaction erases them.
    Emit only fixed harness roles, finite states, counts and binding booleans;
    never emit credentials, identity strings, names or endpoint claims.
    This does not retry the assertion or turn late convergence into success.
    """
    known_ids = {node for node, _ in expected.values()}
    for index in range(3):
        report = {"unavailable": True}
        try:
            result = invoke(index, ["ls"])
            if result.returncode == 0:
                if len(result.stdout) > 64 * 1024:
                    raise ValueError("diagnostic membership byte limit")
                nodes = json.loads(result.stdout)
                if not isinstance(nodes, list) or len(nodes) > 16 or not all(isinstance(node, dict) for node in nodes):
                    raise ValueError("invalid diagnostic membership list")
                roles = {}
                for role, (identity, fingerprint) in expected.items():
                    matches = [node for node in nodes if node.get("nodeId") == identity]
                    roles[role] = {
                        "records": len(matches),
                        "states": sorted({node.get("liveness") if node.get("liveness") in
                                          ("alive", "suspected", "dead") else "invalid" for node in matches}),
                        "bindingMatches": bool(matches) and all(node.get("certFingerprint") == fingerprint for node in matches),
                    }
                report = {"nodes": len(nodes), "roles": roles,
                          "unexpectedRecords": sum(node.get("nodeId") not in known_ids for node in nodes)}
        except Exception:
            # Failure evidence must not replace the original assertion; the
            # production invoke still bounds each subprocess to three seconds.
            pass
        evidence.observe(index, verb, subprocess.CompletedProcess(
            [], 0, json.dumps(report).encode(), b""))


def capture_readmission_failure(invoke, evidence, expected, observe_late=False):
    """Preserve the initial failure, with one optional diagnostic-only later view.

    At most six public reads, each with invoke's three-second subprocess bound,
    and one twenty-second wait. Both three-worker projections fit the bounded
    evidence tail even when invoke records its own CLI responses. No mutation,
    retry of the failed assertion, or successful recovery result is produced.
    """
    capture_readmission_views(invoke, evidence, expected)
    if observe_late:
        time.sleep(20)
        capture_readmission_views(invoke, evidence, expected, verb="readmission-late-diagnostic")


def check_public_adoption(worker_binary, ctl_binary, inject_failure=False, lost_join_ack=False, admission_only=False, issuer_loss=False, handoff_only=False, source_loss=False, dead_assignment=False, removed_assignment=False, blocked_assignment=False, excluded_restart=False, peer_ejection=False, lost_departure=False, lost_leave_response=False, policy_partition=False, client_pressure=False, readmission_only=False, observability=False, issuer_ejection=False):
    """Exercise A admitting B, then B admitting C through public CLI routes."""
    blocked_assignment = blocked_assignment or excluded_restart
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith("ORISHU_")}
    with tempfile.TemporaryDirectory(prefix="orishu-adoption-") as directory, ExitStack() as relay_stack:
        root = Path(directory)
        os.chmod(root, 0o700)
        workers = []
        active = set()
        evidence = Evidence("three-worker-handoff-leave-crash-restart")
        relays = []
        diagnostics = []
        if observability:
            # Reserve distinct ports together. Any subsequent bind race fails
            # startup normally rather than redirecting a scrape to another port.
            with ExitStack() as reservations:
                for _ in range(3):
                    listener = reservations.enter_context(socket.socket())
                    listener.bind(("127.0.0.1", 0))
                    diagnostics.append(listener.getsockname()[1])
        if policy_partition:
            for _ in range(3):
                # Reserve an available loopback port; the worker must bind it
                # successfully or normal startup assertions fail, never retarget.
                with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as reservation:
                    reservation.bind(("127.0.0.1", 0))
                    target = reservation.getsockname()
                relays.append(relay_stack.enter_context(UdpRelay(target)))

        def worker_arguments(index):
            arguments = [str(worker_binary), "--state-dir", str(root / str(index)),
                    "--listen.clients", str(root / f"{index}.sock"),
                    "--listen.peers", "127.0.0.1:0", "--accepts.peers", "true",
                    "--name", "same-label", "--cluster-name", "same-label"]
            if (lost_join_ack or source_loss or dead_assignment or issuer_ejection) and index == 0:
                arguments.append("--test-lose-next-join-ack")
            if issuer_loss and index == 0 and not workers:
                arguments.append("--test-crash-after-join")
            if removed_assignment and index == 0:
                arguments.append("--test-remove-after-join")
            if blocked_assignment and index == 0:
                arguments.append("--test-block-after-join")
            if (peer_ejection and index == 0) or (issuer_ejection and index == 2):
                arguments.append("--test-eject-peer-on-signal")
            if lost_departure and index == 2:
                arguments.append("--test-drop-next-departure")
            if policy_partition:
                arguments[arguments.index("--listen.peers") + 1] = "%s:%s" % relays[index].target
                arguments += ["--advertise.peers", "%s:%s" % relays[index].address]
            if observability:
                arguments += ["--observability.enabled", "true", "--observability.bind",
                              f"127.0.0.1:{diagnostics[index]}"]
            return arguments

        def admission_metrics(index, activity=False, reliable=False, datagrams=False, outbound=False, deadlines=False, catchup=False, health=False):
            connection = http.client.HTTPConnection("127.0.0.1", diagnostics[index], timeout=2)
            try:
                connection.request("GET", "/metrics")
                response = connection.getresponse()
                body = response.read(32769)
                assert response.status == 200 and len(body) <= 32768
                assert response.getheader("Content-Type") == "text/plain; version=0.0.4; charset=utf-8"
                assert response.getheader("Cache-Control") == "no-store"
            finally:
                connection.close()
            assert (root / str(index) / "operator.token").read_bytes().strip() not in body
            assert material["token"].encode() not in body
            samples = dict(line.split() for line in body.decode("ascii").splitlines()
                           if line and not line.startswith("#"))
            if health:
                names = ("owner_responsive", "ready", "startup_complete")
                result = tuple(int(samples["orishu_worker_" + name]) for name in names)
                assert all(value in (0, 1) for value in result)
                evidence.observe(index, "health-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
                return result
            if catchup:
                names = ("started", "transfer_validated", "transfer_binding_rejected", "transfer_invalid",
                         "transfer_rejected", "transfer_unavailable", "transfer_timed_out", "transfer_cancelled",
                         "owner_adopted", "owner_not_adopted", "owner_fenced", "owner_abandoned")
                result = {name: int(samples["orishu_worker_catchup_" + name + "_total"]) for name in names}
                assert all(value >= 0 for value in result.values())
                evidence.observe(index, "catchup-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(result).encode(), b""))
                return result
            if deadlines:
                names = ("direct_probe_deadlines_total", "indirect_probe_deadlines_total",
                         "suspicion_deadlines_total", "anti_entropy_deadlines_total", "join_retry_due_total",
                         "stale_timer_inputs_total", "join_abandoned_total", "anti_entropy_abandoned_total")
                result = tuple(int(samples["orishu_worker_membership_" + name]) for name in names)
                assert all(value >= 0 for value in result)
                evidence.observe(index, "deadline-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
                return result
            if outbound:
                prefix = "orishu_worker_peer_outbound_"
                assert int(samples[prefix + "slots_capacity"]) == 4
                assert 0 <= int(samples[prefix + "slots_in_use"]) <= 4
                for stage in ("attempt", "tls"):
                    family = prefix + stage + "_duration_seconds"
                    bounds = ("0.001", "0.005", "0.025", "0.1", "0.5", "1", "5", "+Inf")
                    buckets = [int(samples[f'{family}_bucket{{le="{bound}"}}']) for bound in bounds]
                    assert buckets == sorted(buckets)
                    assert buckets[-1] == int(samples[family + "_count"])
                names = ("attempt_completed_total", "tls_completed_total",
                         "attempt_duration_seconds_count", "tls_duration_seconds_count")
                result = tuple(int(samples[prefix + name]) for name in names)
                evidence.observe(index, "outbound-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
                return result
            if datagrams:
                names = ("datagrams_submitted_total", "datagrams_received_total",
                         "datagram_bytes_submitted_total", "datagram_bytes_received_total")
                result = tuple(int(samples["orishu_worker_peer_" + name]) for name in names)
                assert all(value >= 0 for value in result)
                evidence.observe(index, "datagram-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
                return result
            if reliable:
                prefix = "orishu_worker_peer_reliable_"
                assert int(samples[prefix + "slots_capacity"]) == 64
                assert 0 <= int(samples[prefix + "slots_in_use"]) <= 64
                for role in ("request", "serve"):
                    family = prefix + role + "_duration_seconds"
                    bounds = ("0.001", "0.005", "0.025", "0.1", "0.5", "1", "5", "+Inf")
                    buckets = [int(samples[f'{family}_bucket{{le="{bound}"}}']) for bound in bounds]
                    assert buckets == sorted(buckets)
                    assert buckets[-1] == int(samples[family + "_count"])
                names = ("request_completed_total", "serve_completed_total", "bytes_sent_total",
                         "bytes_received_total", "request_duration_seconds_count", "serve_duration_seconds_count")
                result = tuple(int(samples[prefix + name]) for name in names)
                evidence.observe(index, "reliable-metrics", subprocess.CompletedProcess(
                    [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
                return result
            names = (("swim_packets_received", "anti_entropy_packets_received", "gossip_items_received")
                     if activity else ("admissions_accepted", "admissions_rejected", "admission_assignment_replays"))
            result = tuple(int(samples["orishu_worker_" + name + "_total"]) for name in names)
            assert all(value >= 0 for value in result)
            evidence.observe(index, "activity-metrics" if activity else "admission-metrics", subprocess.CompletedProcess(
                [], 0, json.dumps(dict(zip(names, result))).encode(), b""))
            return result

        def assert_probes(index, ready):
            for route, expected in [("livez", 200), ("readyz", 200 if ready else 503),
                                    ("startupz", 200)]:
                connection = http.client.HTTPConnection("127.0.0.1", diagnostics[index], timeout=2)
                try:
                    connection.request("GET", "/" + route)
                    response = connection.getresponse()
                    body = response.read(1025)
                    assert response.status == expected, f"worker {index} {route}: {response.status}"
                    assert len(body) <= 1024
                    assert body == (b"ok\n" if expected == 200 else b"local service not ready\n")
                    assert response.getheader("Cache-Control") == "no-store"
                    evidence.observe(index, route, subprocess.CompletedProcess(
                        [], 0, json.dumps({"status": response.status}).encode(), b""))
                finally:
                    connection.close()
            # These calls occur at stable lifecycle checkpoints, not during a
            # claimed atomic scrape/probe snapshot across a transition.
            assert admission_metrics(index, health=True) == (1, int(ready), 1)

        def invoke(index, command, authenticated=False, host=None):
            arguments = [str(ctl_binary), "--host", str(host or root / f"{index}.sock"),
                         "--output", "json", "--timeout", "2s"]
            if authenticated:
                arguments += ["--operator-token-file", str(root / str(index) / "operator.token")]
            result = subprocess.run(arguments + command, env=environment,
                                    capture_output=True, timeout=3)
            evidence.observe(index, command[0], result)
            return result

        def success(index, command, authenticated=False):
            result = invoke(index, command, authenticated)
            assert result.returncode == 0, f"worker {index}: {command[0]} failed"
            return json.loads(result.stdout)

        def inspection_command(operation):
            ref = operation["recoveryReference"]
            return ["admission-inspect", "--formation-id", operation["targetFormationId"],
                    "--attempt-id", ref["attemptId"],
                    "--applicant-fingerprint", ref["applicantFingerprint"],
                    "--introducer-node-id", ref["introducerNodeId"],
                    "--introducer-fingerprint", ref["introducerFingerprint"]]

        def rejected_inspection(body, expected, credential, extra_headers=None, suffix=""):
            connection = http.client.HTTPConnection("localhost", timeout=6)
            connection.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            connection.sock.settimeout(6)
            try:
                connection.sock.connect(str(root / "0.sock"))
                headers = {"Content-Type": "application/cbor", "Authorization": "Bearer " + credential}
                headers.update(extra_headers or {})
                assert_http_rejection(connection, "/api/v1/membership/admission-inspections" + suffix,
                                      body, headers, expected)
            finally:
                connection.close()

        def poll(read, predicate, label, timeout=10, interval=0.02):
            deadline = time.monotonic() + timeout
            while True:
                assert all(workers[index].poll() is None for index in active), "worker exited"
                value = read()
                if predicate(value):
                    return value
                if time.monotonic() >= deadline:
                    raise AssertionError(f"deadline: {label}")
                time.sleep(interval)

        def stop_workers():
            with ExitStack() as shutdown_clients:
                held = []
                if client_pressure:
                    credential = (root / "0" / "operator.token").read_text().strip()
                    held.append(hold_mutation_body(shutdown_clients, root / "0.sock", credential))
                    partial = shutdown_clients.enter_context(socket.socket(socket.AF_UNIX, socket.SOCK_STREAM))
                    partial.settimeout(1)
                    partial.connect(str(root / "0.sock"))
                    partial.sendall(b"POST /api/v1/membership/leaves HTTP/1.1\r\nHost: localhost\r\n")
                    held.append(partial)
                started = time.monotonic()
                for worker in workers:
                    worker.terminate()
                for worker in workers:
                    remaining = 3 - (time.monotonic() - started)
                    assert remaining > 0, "shared graceful shutdown deadline exceeded"
                    assert worker.wait(timeout=remaining) == 0, "graceful shutdown failed"
                # Keep sockets and incomplete requests alive until every process
                # has exited. Closing them must not be what unblocks shutdown.
                for connection in held:
                    received = 0
                    while True:
                        try:
                            chunk = connection.recv(4096)
                        except ConnectionResetError:
                            break
                        if not chunk:
                            break
                        received += len(chunk)
                        assert received <= 8192, "shutdown client response exceeded bound"
            assert not any((root / f"{index}.sock").exists() for index in range(3))

        try:
            for index in range(3):
                workers.append(evidence.start(
                    worker_arguments(index),
                    environment, root / str(index),
                ))
                active.add(index)
            initial = []
            for index in range(3):
                result = poll(lambda: invoke(index, ["cluster", "info"]),
                              lambda result: result.returncode == 0, "startup")
                initial.append(json.loads(result.stdout))
            assert len({view["formationId"] for view in initial}) == 3
            assert len({view["sourceNodeId"] for view in initial}) == 3
            assert all(view["introducerReady"] for view in initial)
            if inject_failure:
                raise AssertionError("injected harness failure after three-worker startup")
            material = success(0, ["token"], True)
            assert material["introducerReady"]
            path = root / "join.json"
            with path.open("x") as output:
                os.chmod(path, 0o600)
                json.dump(material, output)
            command = ["join", "--join-material-file", str(path),
                       "--formation-id", initial[1]["formationId"],
                       "--operation-id", "public-adoption"]
            assert invoke(1, command).returncode != 0
            receipt = success(1, command, True)
            assert receipt["state"]["phase"] == "connecting"
            assert receipt["schemaVersion"] == 2
            assert receipt["recoveryReference"] is None
            if issuer_ejection:
                marker = poll(lambda: evidence.processes[0][2].snapshot(),
                              lambda text: "FORMATION_TEST_LOST_ACK " in text,
                              "issuer insertion before ejection")
                original_id = next(line.split(" ", 1)[1] for line in marker.splitlines()
                                   if line.startswith("FORMATION_TEST_LOST_ACK "))
                # Freeze only the unresolved applicant while a third worker
                # joins normally and delivers the issuer tombstone over QUIC.
                workers[1].send_signal(signal.SIGSTOP)
                witness = ["join", "--join-material-file", str(path),
                           "--formation-id", initial[2]["formationId"],
                           "--operation-id", "issuer-ejection-witness"]
                success(2, witness, True)
                witness_status = poll(lambda: success(2, ["join-status", "issuer-ejection-witness"], True),
                                      lambda value: value["state"]["phase"] == "joined",
                                      "witness completes admission before issuer ejection")
                witness_id = witness_status["state"]["nodeId"]
                workers[2].send_signal(signal.SIGUSR1)
                poll(lambda: evidence.processes[2][2].snapshot(),
                     lambda text: "FORMATION_TEST_EJECTION_SENT " + initial[0]["sourceNodeId"] in text.splitlines(),
                     "witness sends original issuer tombstone")
                ejected = poll(lambda: success(0, ["cluster", "info"]),
                               lambda value: value["participation"] == "ejected",
                               "original issuer learns wire ejection")
                assert ejected["formationId"] == material["formationId"]
                assert ejected["sourceNodeId"] == material["introducerNodeId"]
                assert not ejected["introducerReady"]
                assert invoke(0, ["token"], True).returncode != 0
                workers[1].send_signal(signal.SIGCONT)
                pending = success(1, ["join-status", "public-adoption"], True)
                reference = pending["recoveryReference"]
                assert reference["introducerNodeId"] == material["introducerNodeId"]
                assert reference["introducerFingerprint"] == material["introducerFingerprint"]

                def unresolved_source():
                    operation = success(1, ["join-status", "public-adoption"], True)
                    assert operation["state"]["phase"] in {"admitting", "unresolved"}
                    assert operation["recoveryReference"] == reference
                    report = success(0, inspection_command(operation), True)
                    assert report["outcome"] == {"kind": "recordUnavailable"}
                    assert report["sourceFormationId"] == material["formationId"]
                    assert report["sourceNodeId"] == material["introducerNodeId"]
                    assert {node["nodeId"] for node in success(0, ["ls"])} == {original_id, witness_id}
                    return operation

                unresolved = poll(unresolved_source,
                                  lambda value: value["state"]["phase"] == "unresolved",
                                  "issuer ejection leaves original source unresolved", timeout=210, interval=1)
                assert success(1, command, True) == unresolved
                source = success(1, ["cluster", "info"])
                assert source["formationId"] == initial[1]["formationId"]
                assert source["sourceNodeId"] == initial[1]["sourceNodeId"] != original_id
                assert source["participation"] == "joinUnresolved"
                assert source["nodes"] == source["alive"] == 1
                new_join = command.copy()
                new_join[-1] = "unsafe-ejected-issuer-retry"
                assert invoke(1, new_join, True).returncode != 0
                assert invoke(1, ["leave", "--formation-id", source["formationId"],
                                  "--operation-id", "unsafe-ejected-issuer-leave"], True).returncode != 0
                assert success(1, ["cluster", "info"]) == source
                assert success(0, ["cluster", "info"]) == ejected
                stop_workers()
                return
            if source_loss or dead_assignment or removed_assignment or blocked_assignment:
                marker_prefix = "FORMATION_TEST_REMOVED_ACK " if removed_assignment else "FORMATION_TEST_LOST_ACK "
                if blocked_assignment:
                    marker_prefix = "FORMATION_TEST_BLOCKED_ACK "
                marker = poll(lambda: evidence.processes[0][2].snapshot(),
                              lambda text: marker_prefix in text,
                              "issuer retains insertion after lost acceptance")
                original_id = next(line.split(" ", 1)[1] for line in marker.splitlines()
                                   if line.startswith(marker_prefix))
                # Freeze the surviving issuer, not its ledger, while arranging
                # source loss. No shortened timers or private state mutation.
                workers[0].send_signal(signal.SIGSTOP)
                pending = success(1, ["join-status", "public-adoption"], True)
                assert pending["state"]["phase"] == "admitting", "fault missed pre-adoption window"
                reference = pending["recoveryReference"]
                assert reference["introducerNodeId"] == initial[0]["sourceNodeId"]
                if source_loss:
                    # The original issuer is alive but cannot serve requests.
                    # A failed lookup is not negative admission evidence and
                    # must not replace the source's still-pending attempt.
                    lookup_started = time.monotonic()
                    unavailable = invoke(0, inspection_command(pending), True)
                    assert unavailable.returncode != 0 and not unavailable.stdout.strip()
                    assert time.monotonic() - lookup_started >= 1.5, "inspection did not reach request timeout"
                    assert workers[0].poll() is None, "lookup must fail with the original issuer still alive"
                    after_lookup = success(1, ["join-status", "public-adoption"], True)
                    assert after_lookup["state"]["phase"] in {"admitting", "unresolved"}
                    assert after_lookup["recoveryReference"] == reference
                    assert after_lookup["sourceFormationId"] == pending["sourceFormationId"]
                    assert after_lookup["sourceNodeId"] == pending["sourceNodeId"]
                    assert after_lookup["targetFormationId"] == pending["targetFormationId"]
                    unchanged = success(1, ["cluster", "info"])
                    assert unchanged["formationId"] == initial[1]["formationId"]
                    assert unchanged["sourceNodeId"] == initial[1]["sourceNodeId"]
                    assert unchanged["nodes"] == unchanged["alive"] == 1
                if (dead_assignment or removed_assignment or blocked_assignment) and not excluded_restart:
                    # Keep the original source and its attempt alive, but pause
                    # peer progress until real issuer SWIM declares it Dead.
                    if dead_assignment:
                        workers[1].send_signal(signal.SIGSTOP)
                    workers[0].send_signal(signal.SIGCONT)
                    if dead_assignment:
                        poll(lambda: success(0, ["inspect", original_id]),
                             lambda value: value["liveness"] == "dead",
                             "accepted assignment becomes Dead before recovery", timeout=60)
                    retired = {"kind": "retiredOrRestricted", "nodeId": original_id}
                    assert success(0, inspection_command(pending), True)["outcome"] == retired
                    if dead_assignment:
                        workers[1].send_signal(signal.SIGCONT)

                    def recovering_source():
                        operation = success(1, ["join-status", "public-adoption"], True)
                        assert operation["state"]["phase"] in {"admitting", "unresolved"}
                        assert operation["recoveryReference"] == reference
                        if dead_assignment:
                            assert success(0, ["inspect", original_id])["liveness"] == "dead"
                        expected = {initial[0]["sourceNodeId"]}
                        if dead_assignment or blocked_assignment:
                            expected.add(original_id)
                        if blocked_assignment:
                            assert success(0, inspection_command(operation), True)["outcome"] == retired
                            assert success(0, ["inspect", original_id])["certFingerprint"] == reference["applicantFingerprint"]
                        assert {node["nodeId"] for node in success(0, ["ls"])} == expected
                        return operation

                    unresolved = poll(recovering_source,
                                      lambda value: value["state"]["phase"] == "unresolved",
                                      "retired replay reaches bounded stop", timeout=210, interval=1)
                    assert success(1, command, True) == unresolved, "exact retry reset exhausted admission"
                    source = success(1, ["cluster", "info"])
                    assert source["formationId"] == initial[1]["formationId"]
                    assert source["sourceNodeId"] == initial[1]["sourceNodeId"] != original_id
                    assert source["participation"] == "joinUnresolved"
                    assert source["nodes"] == source["alive"] == 1
                    new_join = command.copy()
                    new_join[-1] = "unsafe-retired-retry"
                    assert invoke(1, new_join, True).returncode != 0
                    assert invoke(1, ["leave", "--formation-id", source["formationId"],
                                      "--operation-id", "unsafe-retired-leave"], True).returncode != 0
                    assert success(0, inspection_command(unresolved), True)["outcome"] == retired
                    assert success(1, ["cluster", "info"]) == source
                    stop_workers()
                    return
                old_operator = (root / "1" / "operator.token").read_bytes()
                active.remove(1)
                workers[1].kill()
                assert workers[1].wait(timeout=3) == -signal.SIGKILL
                workers[1] = evidence.start(worker_arguments(1), environment, root / "1", replace=1)
                active.add(1)
                restarted = poll(lambda: invoke(1, ["cluster", "info"]),
                                 lambda result: result.returncode == 0, "source restart with surviving issuer")
                restarted = json.loads(restarted.stdout)
                assert restarted["participation"] == "standalone"
                assert restarted["nodes"] == restarted["alive"] == 1
                assert restarted["formationId"] not in {initial[1]["formationId"], material["formationId"]}
                assert restarted["sourceNodeId"] not in {initial[1]["sourceNodeId"], original_id}
                assert (root / "1" / "operator.token").read_bytes() == old_operator
                source_material = success(1, ["token"], True)
                assert source_material["introducerFingerprint"] == reference["applicantFingerprint"]
                missing = invoke(1, ["join-status", "public-adoption"], True)
                assert missing.returncode != 0 and b"UnknownOperation" in missing.stderr
                workers[0].send_signal(signal.SIGCONT)
                report = success(0, inspection_command(pending), True)
                assert report["sourceFormationId"] == material["formationId"]
                assert report["sourceNodeId"] == reference["introducerNodeId"]
                assert report["outcome"]["kind"] in {"currentMember", "retiredOrRestricted"}
                assert report["outcome"]["nodeId"] == original_id
                assert success(0, ["inspect", original_id])["certFingerprint"] == reference["applicantFingerprint"]
                assert {node["nodeId"] for node in success(0, ["ls"])} == {initial[0]["sourceNodeId"], original_id}
                assert success(1, ["cluster", "info"]) == restarted
                if excluded_restart:
                    # Deliberate negative admission test, not a recovery recipe.
                    # Retaining the certificate must retain its exclusion even
                    # though this process has fresh standalone identities.
                    retired = {"kind": "retiredOrRestricted", "nodeId": original_id}
                    assert report["outcome"] == retired
                    fresh_command = ["join", "--join-material-file", str(path),
                                     "--formation-id", restarted["formationId"],
                                     "--operation-id", "excluded-restart-admission"]
                    fresh_receipt = success(1, fresh_command, True)
                    assert fresh_receipt["state"]["phase"] == "connecting"

                    def excluded_source():
                        operation = success(1, ["join-status", "excluded-restart-admission"], True)
                        assert operation["state"]["phase"] in {"connecting", "admitting", "unresolved"}
                        assert operation["sourceFormationId"] == restarted["formationId"]
                        assert operation["sourceNodeId"] == restarted["sourceNodeId"]
                        assert operation["targetFormationId"] == material["formationId"]
                        assert success(0, inspection_command(pending), True)["outcome"] == retired
                        assert {node["nodeId"] for node in success(0, ["ls"])} == {initial[0]["sourceNodeId"], original_id}
                        return operation

                    refused = poll(excluded_source,
                                   lambda value: value["state"]["phase"] == "unresolved",
                                   "excluded certificate cannot rejoin after restart", timeout=210, interval=1)
                    fresh_reference = refused["recoveryReference"]
                    assert fresh_reference["applicantFingerprint"] == reference["applicantFingerprint"]
                    assert fresh_reference["attemptId"] != reference["attemptId"]
                    assert success(0, inspection_command(refused), True)["outcome"] == {"kind": "recordUnavailable"}
                    assert success(1, fresh_command, True) == refused
                    final_source = success(1, ["cluster", "info"])
                    assert final_source["formationId"] == restarted["formationId"]
                    assert final_source["sourceNodeId"] == restarted["sourceNodeId"]
                    assert final_source["participation"] == "joinUnresolved"
                    assert final_source["nodes"] == final_source["alive"] == 1
                    # A different certificate can still use the same issuer,
                    # material and transport: exclusion is not a dead listener
                    # or a formation-wide lock masquerading as rejection.
                    control_command = ["join", "--join-material-file", str(path),
                                       "--formation-id", initial[2]["formationId"],
                                       "--operation-id", "nonexcluded-control"]
                    success(2, control_command, True)
                    control_status = poll(lambda: success(2, ["join-status", "nonexcluded-control"], True),
                                          lambda value: value["state"]["phase"] == "joined",
                                          "nonexcluded certificate still admitted")
                    control_id = control_status["state"]["nodeId"]
                    assert control_id not in {original_id, restarted["sourceNodeId"]}
                    assert success(0, ["inspect", control_id])["certFingerprint"] != reference["applicantFingerprint"]
                    assert {node["nodeId"] for node in success(0, ["ls"])} == {initial[0]["sourceNodeId"], original_id, control_id}
                    assert success(0, inspection_command(pending), True)["outcome"] == retired
                    assert success(1, ["cluster", "info"]) == final_source
                    stop_workers()
                    return
                # Inspection supplies evidence only. Never resubmit or create
                # an operation after source history is lost.
                stop_workers()
                return
            if issuer_loss:
                active.remove(0)
                poll(lambda: workers[0].poll(), lambda code: code is not None, "issuer fault exit")
                assert workers[0].returncode == 86, "issuer must exit at the armed insertion boundary"
                marker = poll(lambda: evidence.processes[0][2].snapshot(),
                              lambda text: "FORMATION_TEST_ISSUER_EXIT " in text,
                              "confirmed insertion before issuer exit")
                original_id = next(line.split(" ", 1)[1] for line in marker.splitlines()
                                   if line.startswith("FORMATION_TEST_ISSUER_EXIT "))
                pending = success(1, ["join-status", "public-adoption"], True)
                assert pending["state"]["phase"] in {"admitting", "unresolved"}
                if observability:
                    assert_probes(1, False)
                reference = pending["recoveryReference"]
                assert reference["introducerNodeId"] == initial[0]["sourceNodeId"]
                assert invoke(0, inspection_command(pending), True).returncode != 0
                # Real default retry windows total 183 seconds; no clock hook
                # or reduced retry budget substitutes for process behavior.
                unresolved = poll(lambda: success(1, ["join-status", "public-adoption"], True),
                                  lambda value: value["state"]["phase"] == "unresolved",
                                  "source retains uncertainty after issuer loss", timeout=210, interval=1)
                assert unresolved["recoveryReference"] == reference
                assert success(1, command, True) == unresolved, "exact retry reset exhausted admission"
                source = success(1, ["cluster", "info"])
                assert source["formationId"] == initial[1]["formationId"]
                assert source["sourceNodeId"] == initial[1]["sourceNodeId"] != original_id
                assert source["participation"] == "joinUnresolved"
                assert source["nodes"] == source["alive"] == 1
                if observability:
                    assert_probes(1, False)
                    assert_probes(2, True)
                    assert admission_metrics(1) == (0, 0, 0)
                new_join = command.copy()
                new_join[-1] = "unsafe-new-attempt"
                assert invoke(1, new_join, True).returncode != 0
                assert invoke(1, ["leave", "--formation-id", source["formationId"],
                                  "--operation-id", "unsafe-leave"], True).returncode != 0
                # Restart only the dead issuer to inspect lost history. The
                # unresolved source is never restarted or submitted elsewhere.
                workers[0] = evidence.start(worker_arguments(0), environment, root / "0", replace=0)
                active.add(0)
                poll(lambda: invoke(0, ["cluster", "info"]),
                     lambda result: result.returncode == 0, "issuer restart for inspection")
                report = success(0, inspection_command(unresolved), True)
                assert report["outcome"] == {"kind": "wrongIssuer"}
                assert report["sourceFormationId"] != material["formationId"]
                assert success(1, ["join-status", "public-adoption"], True) == unresolved
                assert success(1, ["cluster", "info"]) == source
                # A second injected failure tests loss of source history. This
                # is not an operator recovery step: no new join is submitted.
                old_operator = (root / "1" / "operator.token").read_bytes()
                active.remove(1)
                workers[1].kill()
                assert workers[1].wait(timeout=3) == -signal.SIGKILL
                workers[1] = evidence.start(worker_arguments(1), environment, root / "1", replace=1)
                active.add(1)
                restarted = poll(lambda: invoke(1, ["cluster", "info"]),
                                 lambda result: result.returncode == 0, "unresolved source restart")
                restarted = json.loads(restarted.stdout)
                assert restarted["participation"] == "standalone"
                assert restarted["nodes"] == restarted["alive"] == 1
                assert restarted["formationId"] not in {source["formationId"], material["formationId"]}
                assert restarted["sourceNodeId"] not in {source["sourceNodeId"], original_id}
                assert (root / "1" / "operator.token").read_bytes() == old_operator
                source_material = success(1, ["token"], True)
                assert source_material["introducerFingerprint"] == reference["applicantFingerprint"]
                missing = invoke(1, ["join-status", "public-adoption"], True)
                assert missing.returncode != 0
                assert b"UnknownOperation" in missing.stderr, "must prove missing history, not transport/auth failure"
                assert success(0, inspection_command(unresolved), True)["outcome"] == {"kind": "wrongIssuer"}
                assert success(1, ["cluster", "info"]) == restarted
                stop_workers()
                return
            status = poll(lambda: success(1, ["join-status", "public-adoption"], True),
                          lambda value: value["state"]["phase"] == "joined", "B catch-up")
            assert status["sourceFormationId"] == initial[1]["formationId"]
            assert status["sourceNodeId"] == initial[1]["sourceNodeId"]
            assert status["targetFormationId"] == initial[0]["formationId"]
            adopted = success(1, ["cluster", "info"])
            assigned = status["state"]["nodeId"]
            if lost_join_ack:
                marker = "FORMATION_TEST_LOST_ACK " + assigned
                poll(lambda: evidence.processes[0][2].snapshot(),
                     lambda output: marker in output.splitlines(),
                     "post-insertion ACK-loss marker matches recovered assignment")
            assert adopted["formationId"] == initial[0]["formationId"]
            assert adopted["sourceNodeId"] == assigned != initial[1]["sourceNodeId"]
            assert adopted["participation"] == "joined"
            assert adopted["introducerReady"]
            assert success(1, command, True) == status, "replay changed accepted identity"
            views = [success(index, ["ls"]) for index in range(2)]
            expected_ids = {initial[0]["sourceNodeId"], assigned}
            assert all({node["nodeId"] for node in view} == expected_ids for view in views)
            inserted = success(0, ["inspect", assigned])
            local = success(1, ["inspect", assigned])
            assert inserted["certFingerprint"] == local["certFingerprint"]
            assert status["schemaVersion"] == 2
            reference = status["recoveryReference"]
            assert reference["introducerNodeId"] == material["introducerNodeId"]
            assert reference["introducerFingerprint"] == material["introducerFingerprint"]
            assert reference["applicantFingerprint"] == local["certFingerprint"]
            assert reference["attemptId"] not in {assigned, status["operationId"]}
            inspect_attempt = inspection_command(status)
            assert invoke(0, inspect_attempt).returncode != 0, "inspection needs operator authority"
            report = success(0, inspect_attempt, True)
            assert report["schemaVersion"] == 1
            assert report["request"]["reference"] == reference
            assert report["sourceFormationId"] == status["targetFormationId"]
            assert report["sourceNodeId"] == reference["introducerNodeId"]
            assert report["outcome"] == {"kind": "currentMember", "nodeId": assigned}
            assert success(1, inspect_attempt, True)["outcome"] == {"kind": "wrongIssuer"}
            unknown_attempt = inspect_attempt.copy()
            unknown_attempt[unknown_attempt.index("--attempt-id") + 1] = "not-recorded"
            assert success(0, unknown_attempt, True)["outcome"] == {"kind": "recordUnavailable"}
            assert success(0, inspect_attempt, True) == report, "inspection mutated retained evidence"
            operator = (root / "0" / "operator.token").read_text().strip()
            rejected_inspection(b"\xa0", 401, material["token"])
            rejected_inspection(b"\xa0", 400, operator)
            rejected_inspection(b"x" * 4097, 400, operator)
            rejected_inspection(b"\xa0", 415, operator, {"Content-Encoding": "gzip"})
            rejected_inspection(b"\xa0", 400, operator, {"If-Match": "unsupported"})
            rejected_inspection(b"\xa0", 400, operator, suffix="?unsupported=true")
            assert success(0, inspect_attempt, True) == report
            assert inserted["workerName"] == local["workerName"] == "same-label"
            assert material["token"] not in json.dumps([receipt, status, adopted, views])
            if peer_ejection:
                assert success(1, ["cluster", "info"])["participation"] == "joined"
                if observability:
                    for index in range(3):
                        assert_probes(index, True)
                workers[0].send_signal(signal.SIGUSR1)
                poll(lambda: evidence.processes[0][2].snapshot(),
                     lambda text: "FORMATION_TEST_EJECTION_SENT " + assigned in text.splitlines(),
                     "test peer sent identified self-removal gossip")
                ejected = poll(lambda: success(1, ["cluster", "info"]),
                               lambda value: value["participation"] == "ejected",
                               "joined worker learns wire self-removal")
                assert ejected["formationId"] == material["formationId"]
                assert ejected["sourceNodeId"] == assigned
                assert not ejected["introducerReady"]
                assert invoke(1, ["token"], True).returncode != 0
                assert success(1, ["join-status", "public-adoption"], True) == status
                refused_join = command.copy()
                refused_join[refused_join.index("--formation-id") + 1] = ejected["formationId"]
                refused_join[-1] = "ejected-cannot-join"
                assert invoke(1, refused_join, True).returncode != 0
                # A is a fixture sender, not an authoritative removal API.
                # Its old view must nevertheless detect B no longer serving
                # membership, rather than receiving successful live probes.
                poll(lambda: success(0, ["inspect", assigned]),
                     lambda value: value["liveness"] == "dead",
                     "ejected worker stops live peer participation", timeout=60)
                stable = success(1, ["cluster", "info"])
                assert stable["participation"] == "ejected"
                assert stable["sourceNodeId"] == assigned
                assert stable["formationId"] == material["formationId"]
                if observability:
                    assert_probes(1, False)
                    # Peer loss alone is not local process failure.
                    assert_probes(0, True)
                    assert admission_metrics(1) == (0, 0, 0)
                departure = success(1, ["leave", "--formation-id", material["formationId"],
                                        "--operation-id", "explicit-ejected-leave"], True)
                standalone = success(1, ["cluster", "info"])
                assert standalone["participation"] == "standalone"
                assert standalone["formationId"] != material["formationId"]
                assert standalone["sourceNodeId"] != assigned
                assert standalone["nodes"] == standalone["alive"] == 1
                assert material["token"] not in json.dumps([ejected, stable, departure, standalone])
                if observability:
                    assert_probes(1, True)
                stop_workers()
                return
            if admission_only:
                stop_workers()
                return
            handoff = success(1, ["token"], True)
            assert handoff["introducerReady"]
            assert handoff["introducerNodeId"] == assigned
            assert handoff["introducerFingerprint"] == local["certFingerprint"]
            assert handoff["formationId"] == material["formationId"]
            assert handoff["token"] == material["token"]
            handoff_path = root / "handoff.json"
            with handoff_path.open("x") as output:
                os.chmod(handoff_path, 0o600)
                json.dump(handoff, output)
            third_command = ["join", "--join-material-file", str(handoff_path),
                             "--formation-id", initial[2]["formationId"],
                             "--operation-id", "public-handoff"]
            success(2, third_command, True)
            third_status = poll(
                lambda: success(2, ["join-status", "public-handoff"], True),
                lambda value: value["state"]["phase"] == "joined", "C catch-up")
            assert third_status["sourceFormationId"] == initial[2]["formationId"]
            assert third_status["sourceNodeId"] == initial[2]["sourceNodeId"]
            assert third_status["targetFormationId"] == material["formationId"]
            third_id = third_status["state"]["nodeId"]
            third_inspection = inspection_command(third_status)
            assert success(1, third_inspection, True)["outcome"] == {"kind": "currentMember", "nodeId": third_id}
            assert third_id != initial[2]["sourceNodeId"]
            assert success(2, third_command, True) == third_status
            expected_ids.add(third_id)
            assert len(expected_ids) == 3
            all_views = poll(
                lambda: [success(index, ["ls"]) for index in range(3)],
                lambda lists: all(
                    {node["nodeId"] for node in nodes} == expected_ids
                    and all(node["liveness"] == "alive" for node in nodes)
                    for nodes in lists), "three-worker live identity convergence")
            expected_pins = {node["nodeId"]: node["certFingerprint"] for node in all_views[0]}
            for index, nodes in enumerate(all_views):
                assert {node["nodeId"]: node["certFingerprint"] for node in nodes} == expected_pins
                assert all(node["workerName"] == "same-label" for node in nodes)
                summary = success(index, ["cluster", "info"])
                assert summary["formationId"] == material["formationId"]
                assert summary["introducerReady"]
                assert summary["alive"] == summary["nodes"] == 3
            assert material["token"] not in json.dumps([third_status, all_views])
            if observability:
                measured = [admission_metrics(index) for index in range(3)]
                poll(lambda: [admission_metrics(index, activity=True) for index in range(3)],
                     lambda views: all(all(value > 0 for value in values) for values in views),
                     "all workers expose real SWIM, anti-entropy and gossip activity", timeout=15)
                assert [value[0] for value in measured] == [1, 1, 0], "count local insertion, not learned membership"
                assert [value[1] for value in measured] == [0, 0, 0]
                assert measured[1][2] == measured[2][2] == 0
                assert measured[0][2] >= int(lost_join_ack)
                reliable_before_leave = poll(
                    lambda: [admission_metrics(index, reliable=True) for index in range(3)],
                    lambda views: all(all(value > 0 for value in values) for values in views),
                    "all workers expose completed reliable exchanges, stream bytes and durations", timeout=15)
                datagrams_before_leave = poll(
                    lambda: [admission_metrics(index, datagrams=True) for index in range(3)],
                    lambda views: all(all(value > 0 for value in values) for values in views),
                    "all workers expose submitted/received datagrams and payload bytes", timeout=15)
                outbound_before_leave = [admission_metrics(index, outbound=True) for index in range(3)]
                deadlines_before_leave = [admission_metrics(index, deadlines=True) for index in range(3)]
                catchup_before_leave = [admission_metrics(index, catchup=True) for index in range(3)]
                assert not any(catchup_before_leave[0].values()), "initial introducer did not fetch a baseline"
                for outcomes in catchup_before_leave[1:]:
                    assert outcomes["started"] >= 1
                    assert outcomes["transfer_validated"] >= 1
                    assert outcomes["owner_adopted"] == 1, "one owner adoption, not one per page"
                    # Both joiners are now Joined and no longer start catch-up
                    # jobs. These are quiescent sums, not a scrape transaction.
                    for prefix in ("transfer_", "owner_"):
                        assert sum(value for name, value in outcomes.items() if name.startswith(prefix)) == outcomes["started"]
                assert all(value > 0 for row in outbound_before_leave[1:] for value in row), \
                    "both joining workers must expose completed outbound attempts and TLS candidates"
            if handoff_only:
                stop_workers()
                return
            if client_pressure:
                started = time.monotonic()
                with ExitStack() as pressure:
                    credential = (root / "0" / "operator.token").read_text().strip()
                    pending = []
                    for _ in range(16):
                        pending.append(hold_mutation_body(pressure, root / "0.sock", credential))
                    # The real body reader has started under each mutation permit.
                    # An extra malformed request must get overload, not body parsing.
                    overload = http.client.HTTPConnection("localhost", timeout=1)
                    overload.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    overload.sock.settimeout(1)
                    try:
                        overload.sock.connect(str(root / "0.sock"))
                        assert_http_rejection(overload, "/api/v1/membership/leaves", b"\xa0",
                                              {"Content-Type": "application/cbor", "Authorization": "Bearer " + credential}, 503)
                    finally:
                        overload.close()
                    command = ["cluster", "lock", "--formation-id", material["formationId"],
                               "--operation-id", "client-pressure-lock"]
                    assert success(1, command, True)["locked"]
                    poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                         lambda views: all(view["locked"] for view in views),
                         "peer policy reaches worker with saturated client mutations", timeout=2)
                    # Complete exactly one held malformed body. Waiting for its
                    # response proves release, rather than relying on disconnect.
                    pending[0].sendall(b"\x00" * 4095)
                    response = http.client.HTTPResponse(pending[0])
                    response.begin()
                    assert response.status == 400
                    assert len(response.read(4097)) <= 4096
                    response.close()
                    assert not success(0, ["cluster", "unlock", "--formation-id", material["formationId"],
                                           "--operation-id", "client-pressure-recovered"], True)["locked"]
                    assert time.monotonic() - started < 4, "pressure assertions reached body timeout instead of proving capacity recovery"
                    assert all(success(index, ["cluster", "info"])["nodes"] == 3 for index in range(3))
                poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                     lambda views: all(not view["locked"] for view in views), "pressure recovery convergence")
                with ExitStack() as header_pressure:
                    started = time.monotonic()
                    for _ in range(60):
                        connection = header_pressure.enter_context(socket.socket(socket.AF_UNIX, socket.SOCK_STREAM))
                        connection.settimeout(1)
                        connection.connect(str(root / "0.sock"))
                        connection.sendall(b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n\r\n")
                        response = http.client.HTTPResponse(connection)
                        response.begin()
                        assert response.status == 200
                        assert len(response.read(65537)) <= 65536
                        response.close()
                        connection.sendall(b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n")
                    # Sixty proven-accepted partial heads leave four of the
                    # listener's 64 slots for real operator reads. Full capacity
                    # and expiry are independently checked by executable tests.
                    assert success(1, ["cluster", "lock", "--formation-id", material["formationId"],
                                       "--operation-id", "header-pressure-lock"], True)["locked"]
                    poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                         lambda views: all(view["locked"] for view in views),
                         "peer policy progresses with unfinished headers", timeout=2)
                    assert time.monotonic() - started < 4, "header expiry masked peer progress"
            # No harness-created peer sessions: production maintenance must
            # reconnect the adopted worker before either direction converges.
            lock = ["cluster", "lock", "--formation-id", material["formationId"],
                    "--operation-id", "post-adoption-lock"]
            assert success(0, lock, True)["locked"]
            poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                 lambda values: all(value["locked"] for value in values), "lock convergence")
            if observability:
                for index in range(3):
                    assert_probes(index, True)  # A safely enforced lock is not local unreadiness.
            unlock = ["cluster", "unlock", "--formation-id", material["formationId"],
                      "--operation-id", "post-adoption-unlock"]
            assert not success(1, unlock, True)["locked"]
            poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                 lambda values: all(not value["locked"] for value in values), "unlock convergence")
            if policy_partition:
                started = time.monotonic()
                dropped_before = sum(relay.stats()["dropped"] for relay in relays)
                for relay in relays:
                    relay.partition(True)
                try:
                    def policy(index, locked, operation):
                        return success(index, ["cluster", "lock" if locked else "unlock",
                                               "--formation-id", material["formationId"],
                                               "--operation-id", operation], True)
                    left = policy(0, True, "partition-a-lock")
                    policy(1, True, "partition-b-lock")
                    winner = policy(1, False, "partition-b-unlock")
                    assert winner["policyVersion"]["counter"] > left["policyVersion"]["counter"]
                    while sum(relay.stats()["dropped"] for relay in relays) == dropped_before:
                        assert time.monotonic() - started < 2, "partition did not intercept peer traffic"
                        time.sleep(0.01)
                    views = [success(index, ["cluster", "info"]) for index in range(3)]
                    assert [view["locked"] for view in views] == [True, False, False]
                    # Keep the isolation shorter than retirement; no stopped
                    # processes or clock changes are used to arrange this fault.
                    assert time.monotonic() - started < 3, "partition commands exceeded fault budget"
                finally:
                    for relay in relays:
                        relay.partition(False)
                poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                     lambda values: all(not value["locked"] for value in values), "healed policy convergence")
                # No-op unlock receipts expose the current policy version without
                # creating a new one. Fresh IDs avoid replaying historical receipts.
                for attempt in range(10):
                    receipts = [policy(index, False, f"healed-version-{attempt}-{index}") for index in range(3)]
                    if all(receipt["policyVersion"] == winner["policyVersion"] for receipt in receipts):
                        break
                    time.sleep(0.05)
                else:
                    raise AssertionError("healed workers did not adopt the exact winning policy version")
                poll(lambda: [success(index, ["ls"]) for index in range(3)],
                     lambda lists: all({node["nodeId"]: node["certFingerprint"] for node in nodes} == expected_pins
                                      and all(node["liveness"] == "alive" for node in nodes)
                                      for nodes in lists), "partition preserves exact live identities")
                assert all(relay.stats()["routes"] <= relay.MAX_ROUTES for relay in relays)
            leave = ["leave", "--formation-id", material["formationId"],
                     "--operation-id", "public-departure"]
            assert invoke(2, leave).returncode != 0, "unauthenticated leave"
            if lost_leave_response:
                proxy_path = root / "lost-leave-response.sock"
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
                    listener.bind(str(proxy_path))
                    listener.listen(1)
                    with ThreadPoolExecutor(max_workers=1) as executor:
                        discarded = executor.submit(discard_one_http_response, listener, root / "2.sock")
                        interrupted = invoke(2, leave, True, host=proxy_path)
                        assert interrupted.returncode != 0, "CLI received success despite discarded response"
                        assert discarded.result(timeout=6) == 200
                before_replay = success(2, ["cluster", "info"])
                assert before_replay["participation"] == "standalone"
                assert before_replay["formationId"] != material["formationId"]
                assert before_replay["sourceNodeId"] != third_id
            departed = success(2, leave, True)
            if lost_leave_response:
                assert departed["current"]["formationId"] == before_replay["formationId"]
                assert departed["current"]["sourceNodeId"] == before_replay["sourceNodeId"]
                assert success(2, ["cluster", "info"]) == before_replay
            if lost_departure:
                poll(lambda: evidence.processes[2][2].snapshot(),
                     lambda text: f"FORMATION_TEST_DROPPED_DEPARTURE {third_id} 2" in text.splitlines(),
                     "both encoded departure announcements deliberately dropped")
            assert departed["changed"]
            assert departed["previousFormationId"] == material["formationId"]
            assert departed["previousNodeId"] == third_id
            replacement = departed["current"]
            assert replacement["formationId"] != material["formationId"]
            assert replacement["sourceNodeId"] not in expected_ids
            assert replacement["participation"] == "standalone"
            assert replacement["memberCount"] == replacement["aliveCount"] == 1
            if observability:
                retained = admission_metrics(2, reliable=True)
                assert all(after >= before for before, after in zip(reliable_before_leave[2], retained)), \
                    "formation change must not reset reliable-exchange counters"
                retained_datagrams = admission_metrics(2, datagrams=True)
                assert all(after >= before for before, after in zip(datagrams_before_leave[2], retained_datagrams)), \
                    "formation change must not reset datagram counters"
                retained_outbound = admission_metrics(2, outbound=True)
                assert all(after >= before for before, after in zip(outbound_before_leave[2], retained_outbound)), \
                    "formation change must not reset outbound observations"
                retained_deadlines = admission_metrics(2, deadlines=True)
                assert all(after >= before for before, after in zip(deadlines_before_leave[2], retained_deadlines)), \
                    "formation change must not reset membership deadline observations"
                assert admission_metrics(2, catchup=True) == catchup_before_leave[2], \
                    "leave retains completed catch-up outcomes without starting another transfer"
            assert success(2, leave, True) == departed, "leave replay changed identity"
            stale = leave.copy()
            stale[-1] = "stale-departure"
            assert invoke(2, stale, True).returncode != 0
            conflict = leave.copy()
            conflict[2] = replacement["formationId"]
            assert invoke(2, conflict, True).returncode != 0
            noop = ["leave", "--formation-id", replacement["formationId"],
                    "--operation-id", "standalone-noop"]
            assert not success(2, noop, True)["changed"]
            fresh_material = success(2, ["token"], True)
            assert fresh_material["token"] != material["token"]
            assert fresh_material["introducerFingerprint"] == expected_pins[third_id]
            poll(lambda: [success(index, ["inspect", third_id]) for index in range(2)],
                 lambda views: all(view["liveness"] == "dead" for view in views),
                 "survivors detect voluntary departure", timeout=60)
            assert success(1, third_inspection, True)["outcome"] == {"kind": "retiredOrRestricted", "nodeId": third_id}
            assert material["token"] not in json.dumps(departed)
            assert fresh_material["token"] not in json.dumps(departed)
            rejoin = ["join", "--join-material-file", str(handoff_path),
                      "--formation-id", replacement["formationId"],
                      "--operation-id", "public-readmission"]
            success(2, rejoin, True)
            readmitted = poll(
                lambda: success(2, ["join-status", "public-readmission"], True),
                lambda value: value["state"]["phase"] == "joined", "readmission after leave")
            new_id = readmitted["state"]["nodeId"]
            assert new_id not in expected_ids and new_id != replacement["sourceNodeId"]
            assert success(2, leave, True) == departed, "historical leave replay departed again"
            readmission_records = {identity: (pin, "dead" if identity == third_id else "alive")
                                   for identity, pin in expected_pins.items()}
            readmission_records[new_id] = (expected_pins[third_id], "alive")
            readmission_deadline = time.monotonic() + READMISSION_CONVERGENCE_SECONDS
            try:
                poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                     lambda views: all(view["nodes"] == 4 and view["alive"] == 3 for view in views),
                     "readmission retains dead history and three live identities",
                     timeout=READMISSION_CONVERGENCE_SECONDS)
                # Preserve the original summary assertion and use only its
                # remaining observation budget for the stronger identity check.
                poll(lambda: [success(index, ["ls"]) for index in range(3)],
                     lambda views: all(readmission_view_matches(nodes, readmission_records) for nodes in views),
                     "readmission exact identities, certificate bindings and liveness",
                     timeout=max(0, readmission_deadline - time.monotonic()))
            except AssertionError:
                capture_readmission_failure(invoke, evidence, {
                    "originalIntroducer": (initial[0]["sourceNodeId"], expected_pins[initial[0]["sourceNodeId"]]),
                    "secondIntroducer": (assigned, expected_pins[assigned]),
                    "departed": (third_id, expected_pins[third_id]),
                    "readmitted": (new_id, expected_pins[third_id]),
                }, observe_late=readmission_only)
                raise
            assert success(2, ["cluster", "info"])["sourceNodeId"] == new_id
            assert success(2, ["inspect", new_id])["certFingerprint"] == expected_pins[third_id]
            if readmission_only:
                stop_workers()
                return
            old_operator = (root / "1" / "operator.token").read_bytes()
            if observability:
                deadlines_before_crash = [admission_metrics(index, deadlines=True) for index in (0, 2)]
            active.remove(1)
            workers[1].kill()
            assert workers[1].wait(timeout=3) == -signal.SIGKILL
            observed = set()
            incident_checkpoints = set()

            def crashed_member_views():
                views = [success(index, ["inspect", assigned]) for index in (0, 2)]
                for index, source, view in zip((0, 2), (material["introducerNodeId"], new_id), views):
                    assert_incident_member(view, material["formationId"], source,
                                           assigned, expected_pins[assigned])
                observed.update(view["liveness"] for view in views)
                if observability:
                    states = {view["liveness"] for view in views} & {"suspected", "dead"}
                    if states - incident_checkpoints:
                        for index in (0, 2):
                            assert_probes(index, True)
                        incident_checkpoints.update(states)
                return views

            poll(crashed_member_views,
                 lambda views: all(view["liveness"] == "dead" for view in views),
                 "survivors detect killed worker", timeout=60)
            assert "suspected" in observed, "crash bypassed observable SWIM suspicion"
            if observability:
                assert incident_checkpoints == {"suspected", "dead"}, "missing peer-loss health checkpoints"
                deadlines_after_crash = [admission_metrics(index, deadlines=True) for index in (0, 2)]
                for field in (0, 2):
                    assert sum(row[field] for row in deadlines_after_crash) > sum(row[field] for row in deadlines_before_crash), \
                        "surviving processes must expose consumed probe and suspicion deadlines"
            workers[1] = evidence.start(worker_arguments(1), environment, root / "1", replace=1)
            active.add(1)
            restarted = poll(lambda: invoke(1, ["cluster", "info"]),
                             lambda result: result.returncode == 0, "restart startup")
            restarted = json.loads(restarted.stdout)
            assert restarted["participation"] == "standalone"
            assert restarted["nodes"] == restarted["alive"] == 1
            if observability:
                assert admission_metrics(1) == (0, 0, 0), "process restart resets issuer counters"
                assert not any(admission_metrics(1, catchup=True).values()), "process restart resets catch-up counters"
                assert admission_metrics(1, activity=True) == (0, 0, 0), "fresh standalone restart resets received activity"
                assert admission_metrics(1, reliable=True) == (0, 0, 0, 0, 0, 0), \
                    "fresh standalone restart resets reliable-exchange observations"
                assert admission_metrics(1, datagrams=True) == (0, 0, 0, 0), \
                    "fresh standalone restart resets datagram observations"
                assert admission_metrics(1, outbound=True) == (0, 0, 0, 0), \
                    "fresh standalone restart resets outbound observations"
                assert admission_metrics(1, deadlines=True) == (0,) * 8, \
                    "fresh standalone restart resets membership deadline observations"
            assert restarted["formationId"] not in {material["formationId"], initial[1]["formationId"]}
            assert restarted["sourceNodeId"] not in expected_ids | {new_id, initial[1]["sourceNodeId"]}
            assert (root / "1" / "operator.token").read_bytes() == old_operator
            assert invoke(1, ["inspect", assigned]).returncode != 0, "restart restored old membership"
            assert success(1, third_inspection, True)["outcome"] == {"kind": "wrongIssuer"}, "restart must not claim old issuer history"
            restart_material = success(1, ["token"], True)
            assert restart_material["introducerFingerprint"] == expected_pins[assigned]
            assert restart_material["token"] != material["token"]
            restart_join = ["join", "--join-material-file", str(path),
                            "--formation-id", restarted["formationId"],
                            "--operation-id", "explicit-restart-admission"]
            success(1, restart_join, True)
            restart_status = poll(
                lambda: success(1, ["join-status", "explicit-restart-admission"], True),
                lambda value: value["state"]["phase"] == "joined", "restart readmission")
            restarted_id = restart_status["state"]["nodeId"]
            assert restarted_id not in expected_ids | {new_id, restarted["sourceNodeId"]}
            poll(lambda: [success(index, ["cluster", "info"]) for index in range(3)],
                 lambda views: all(view["nodes"] == 5 and view["alive"] == 3
                                   and view["formationId"] == material["formationId"] for view in views),
                 "restart readmission convergence", timeout=60)
            expected_restart = {
                node: (pin, "dead" if node in {assigned, third_id} else "alive")
                for node, pin in (expected_pins | {new_id: expected_pins[third_id],
                                                  restarted_id: expected_pins[assigned]}).items()
            }
            poll(lambda: [success(index, ["ls"]) for index in range(3)],
                 lambda views: all({node["nodeId"]: (node["certFingerprint"], node["liveness"])
                                    for node in view} == expected_restart for view in views),
                 "exact restart membership and liveness convergence")
            for index in range(3):
                assert success(index, ["inspect", assigned])["liveness"] == "dead"
                assert success(index, ["inspect", restarted_id])["certFingerprint"] == expected_pins[assigned]
            stop_workers()
        except BaseException as error:
            try:
                print(f"formation failure evidence: {evidence.save_failure(error)}")
            except OSError:
                print("formation failure evidence could not be written")
            raise
        finally:
            for worker in workers:
                if worker.poll() is None:
                    worker.kill()
            for worker in workers:
                worker.wait(timeout=3)
            evidence.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, default=Path("target/debug/orishu-worker"))
    parser.add_argument("--ctl", type=Path, default=Path("target/debug/orishuctl"))
    parser.add_argument("--inject-failure-after-startup", action="store_true",
                        help="Test failure artifacts/cleanup after three-worker startup; not a worker fault mode")
    parser.add_argument("--lost-join-ack", action="store_true",
                        help="Require a formation-fault-test worker; discard A's first accepted JoinAck")
    parser.add_argument("--admission-only", action="store_true",
                        help="Diagnostic subset: stop after A admits B and issuer inspection; omits handoff/churn")
    parser.add_argument("--issuer-loss", action="store_true",
                        help="Require fault build: issuer exits after insertion; wait for real retry exhaustion and assert stop outcome")
    parser.add_argument("--issuer-ejection", action="store_true",
                        help="Require fault build: wire-eject original issuer after insertion and verify unavailable-history stop")
    parser.add_argument("--source-loss", action="store_true",
                        help="Require fault build: source crashes before adoption; surviving issuer retains assignment")
    parser.add_argument("--dead-assignment", action="store_true",
                        help="Require fault build: pause source after ACK loss until issuer SWIM retires its assignment")
    parser.add_argument("--removed-assignment", action="store_true",
                        help="Require fault build: remove accepted assignment before its ACK; original source must stop")
    parser.add_argument("--blocked-assignment", action="store_true",
                        help="Require fault build: block accepted certificate before its ACK; original source must stop")
    parser.add_argument("--excluded-restart", action="store_true",
                        help="Require fault build: restart blocked source and prove fresh admission cannot bypass exclusion")
    parser.add_argument("--peer-ejection", action="store_true",
                        help="Require Unix fault build: signal test peer to send valid self-removal gossip after adoption")
    parser.add_argument("--lost-departure", action="store_true",
                        help="Require fault build: drop C's leave announcements; full journey must detect departure and readmit")
    parser.add_argument("--lost-leave-response", action="store_true",
                        help="Discard accepted public leave response through a local proxy; recover the exact receipt")
    parser.add_argument("--handoff-only", action="store_true",
                        help="Diagnostic subset: stop after three-worker handoff/convergence; omit leave/crash churn")
    parser.add_argument("--readmission-only", action="store_true",
                        help="Diagnostic subset: handoff, leave and readmission; omit standalone checks and later crash/restart")
    parser.add_argument("--policy-partition", action="store_true",
                        help="Partition encrypted peer traffic with loopback relays, heal competing policy updates, then run full churn")
    parser.add_argument("--client-pressure", action="store_true",
                        help="Hold authenticated client bodies while real formation policy converges; verify overload and recovery")

    parser.add_argument("--observability", action="store_true",
                        help="Require observability build: full admission scrapes, optionally --lost-join-ack; --peer-ejection or --issuer-loss select probe lifecycle journeys")
    args = parser.parse_args()
    if args.issuer_ejection and any(value for name, value in vars(args).items()
                                   if name not in {"worker", "ctl", "issuer_ejection"}):
        parser.error("--issuer-ejection is a separate fault scenario")
    if args.observability and any(value for name, value in vars(args).items()
                                 if name not in {"worker", "ctl", "observability", "lost_join_ack", "peer_ejection", "issuer_loss"}):
        parser.error("--observability supports ordinary/lost-ACK formation or ejection/issuer-loss probe journeys")
    if args.readmission_only and any(value for name, value in vars(args).items()
                                    if name not in {"worker", "ctl", "readmission_only"}):
        parser.error("--readmission-only is a separate diagnostic subset")
    if args.client_pressure and any(value for name, value in vars(args).items()
                                   if name not in {"worker", "ctl", "client_pressure"}):
        parser.error("--client-pressure is a separate fault scenario")
    if args.policy_partition and any(value for name, value in vars(args).items()
                                     if name not in {"worker", "ctl", "policy_partition"}):
        parser.error("--policy-partition is a separate fault scenario")
    if args.lost_leave_response and (args.lost_departure or args.peer_ejection or args.excluded_restart or args.blocked_assignment or args.removed_assignment or args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--lost-leave-response is a separate fault scenario")
    if args.lost_departure and (args.peer_ejection or args.excluded_restart or args.blocked_assignment or args.removed_assignment or args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--lost-departure is a separate fault scenario")
    if args.peer_ejection and (args.excluded_restart or args.blocked_assignment or args.removed_assignment or args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--peer-ejection is a separate fault scenario")
    if args.excluded_restart and (args.blocked_assignment or args.removed_assignment or args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--excluded-restart is a separate fault scenario")
    if args.blocked_assignment and (args.removed_assignment or args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--blocked-assignment is a separate fault scenario")
    if args.removed_assignment and (args.dead_assignment or args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--removed-assignment is a separate fault scenario")
    if args.dead_assignment and (args.source_loss or args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--dead-assignment is a separate fault scenario")
    if args.source_loss and (args.issuer_loss or args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--source-loss is a separate fault scenario")
    if args.issuer_loss and (args.lost_join_ack or args.admission_only or args.handoff_only or args.inject_failure_after_startup):
        parser.error("--issuer-loss is a separate fault scenario")
    if args.admission_only and args.handoff_only:
        parser.error("select only one diagnostic subset")
    with tempfile.TemporaryDirectory(prefix="orishu-formation-binaries-") as directory:
        root = Path(directory)
        worker_binary, ctl_binary = root / "orishu-worker", root / "orishuctl"
        snapshot_executable(args.worker.resolve(), worker_binary)
        snapshot_executable(args.ctl.resolve(), ctl_binary)
        run_selected(args, worker_binary, ctl_binary)


def run_selected(args, worker_binary, ctl_binary):
    if args.readmission_only:
        check_public_adoption(worker_binary, ctl_binary, readmission_only=True)
        print("handoff, leave and readmission diagnostic passed (standalone/crash/restart/full conformance not exercised)")
        return

    with tempfile.TemporaryDirectory(prefix="orishu-cli-") as directory:
        root = Path(directory)
        os.chmod(root, 0o700)
        socket, state = root / "worker.sock", root / "state"
        worker_environment = {key: value for key, value in os.environ.items()
                              if not key.startswith("ORISHU_")}
        evidence = Evidence("standalone-cli-authorization-replay")
        worker = evidence.start(
            [str(worker_binary), "--state-dir", str(state), "--listen.clients", str(socket),
             "--name", "cli-test", "--cluster-name", "cli-test", "--listen.peers", "127.0.0.1:0"],
            worker_environment, state,
        )

        def invoke(command, authenticated=False):
            environment = os.environ.copy()
            environment.pop("ORISHU_OPERATOR_TOKEN_FILE", None)
            arguments = [str(ctl_binary), "--host", str(socket), "--output", "json", "--timeout", "2s"]
            if authenticated:
                arguments += ["--operator-token-file", str(state / "operator.token")]
            result = subprocess.run(arguments + command, env=environment, capture_output=True, timeout=3)
            evidence.observe(0, command[0], result)
            return result

        def success(command, authenticated=False):
            result = invoke(command, authenticated)
            if result.returncode != 0:
                raise AssertionError(f"CLI command failed: {command[0:2]} (exit {result.returncode})")
            return json.loads(result.stdout)

        try:
            deadline = time.monotonic() + 10
            while True:
                if worker.poll() is not None:
                    raise AssertionError(f"worker exited during startup ({worker.returncode})")
                result = invoke(["cluster", "info"])
                if result.returncode == 0:
                    initial = json.loads(result.stdout)
                    break
                if time.monotonic() >= deadline:
                    raise AssertionError("worker startup deadline")
                time.sleep(0.02)

            def intent(verb, operation):
                return ["cluster", verb, "--formation-id", initial["formationId"], "--operation-id", operation]

            inspected = success(["inspect", initial["sourceNodeId"]])
            assert inspected["formationId"] == initial["formationId"]
            assert inspected["nodeId"] == initial["sourceNodeId"]
            assert inspected["sourceNodeId"] == initial["sourceNodeId"]
            assert inspected["workerName"] == "cli-test"
            assert inspected["liveness"] == "alive"
            assert invoke(["token"]).returncode != 0
            material = success(["token"], True)
            assert material["formationId"] == initial["formationId"]
            assert material["introducerNodeId"] == initial["sourceNodeId"]
            assert material["introducerFingerprint"] == inspected["certFingerprint"]
            assert material["peerEndpoints"] == inspected["peerEndpoints"]
            assert not material["peerEndpoints"][0].endswith(":0")
            assert material["introducerReady"] is False
            assert len(material["token"]) == 64
            assert material["token"] not in json.dumps(inspected)
            material_path = root / "join.json"
            with material_path.open("x") as output:
                os.chmod(material_path, 0o600)
                json.dump(material | {"formationId": "join-input-target", "introducerFingerprint": "f" * 64}, output)
            join_command = ["join", "--join-material-file", str(material_path),
                            "--formation-id", initial["formationId"], "--operation-id", "join-input-test"]
            assert invoke(join_command).returncode != 0
            join = invoke(join_command, True)
            assert join.returncode == 0
            assert json.loads(join.stdout)["state"]["phase"] == "connecting"
            assert material["token"].encode() not in join.stderr + join.stdout
            assert invoke(["join-status", "join-input-test"]).returncode != 0
            deadline = time.monotonic() + 5
            while True:
                status = success(["join-status", "join-input-test"], True)
                if status["state"]["phase"] == "failedBeforeAdmission":
                    break
                if time.monotonic() >= deadline:
                    raise AssertionError("join status deadline")
                time.sleep(0.02)
            assert success(join_command, True) == status
            assert invoke(["join-status", "unknown-operation"], True).returncode != 0
            stale_join = join_command.copy()
            stale_join[4], stale_join[6] = "stale-source", "stale-attempt"
            assert invoke(stale_join, True).returncode != 0
            with material_path.open("w") as output:
                json.dump(material | {"formationId": "join-input-target", "introducerFingerprint": "f" * 64, "token": "b" * 64}, output)
            assert invoke(join_command, True).returncode != 0
            assert status["sourceFormationId"] == initial["formationId"]
            assert status["targetFormationId"] == "join-input-target"
            assert success(["cluster", "info"])["formationId"] == initial["formationId"]
            assert invoke(["token", "--rotate"], True).returncode != 0
            listed = success(["ls"])
            assert len(listed) == 1 and listed[0] == inspected
            assert success(["ls", "--name", "absent-worker"]) == []
            assert invoke(["inspect", initial["sourceNodeId"], "--source", "direct"]).returncode != 0

            lock_id, unlock_id = uuid.uuid4().hex, uuid.uuid4().hex
            lock = intent("lock", lock_id)
            assert invoke(lock).returncode != 0, "local unauthenticated mutation succeeded"
            assert success(["cluster", "info"])["locked"] is False
            receipt = success(lock, True)
            assert receipt["operationId"] == lock_id and receipt["locked"] is True
            assert receipt["formationId"] == initial["formationId"]
            assert receipt["sourceNodeId"] == initial["sourceNodeId"]
            assert success(["cluster", "info"])["locked"] is True
            assert success(intent("unlock", unlock_id), True)["locked"] is False
            assert success(lock, True) == receipt, "retry changed historical receipt"
            assert success(["cluster", "info"])["locked"] is False, "retry reapplied old lock"
            assert invoke(intent("unlock", lock_id), True).returncode != 0, "conflicting ID reused"
            stale = ["cluster", "lock", "--formation-id", "wrong-formation", "--operation-id", uuid.uuid4().hex]
            assert invoke(stale, True).returncode != 0, "stale formation accepted"
            assert success(["cluster", "info"])["locked"] is False
            worker.terminate()
            assert worker.wait(timeout=3) == 0, "graceful shutdown failed"
            assert not socket.exists(), "worker left its socket behind"
        except BaseException as error:
            try:
                print(f"formation failure evidence: {evidence.save_failure(error)}")
            except OSError:
                print("formation failure evidence could not be written")
            raise
        finally:
            if worker.poll() is None:
                worker.kill()
            worker.wait(timeout=3)
            evidence.close()

    check_public_adoption(worker_binary, ctl_binary, args.inject_failure_after_startup, args.lost_join_ack, args.admission_only, args.issuer_loss, args.handoff_only, args.source_loss, args.dead_assignment, args.removed_assignment, args.blocked_assignment, args.excluded_restart, args.peer_ejection, args.lost_departure, args.lost_leave_response, args.policy_partition, args.client_pressure, observability=args.observability, issuer_ejection=args.issuer_ejection)
    if args.issuer_ejection:
        print("Wire-ejected original issuer lost accepted history without changing identity; original source required bounded operator stop")
        return
    if args.observability and args.peer_ejection:
        print("HTTP probes tracked joined, ejected and explicitly returned standalone state; startup remained latched and peer loss did not fail survivor health")
    elif args.observability and args.issuer_loss:
        print("HTTP probes kept pending/unresolved admission live but unready with startup latched through real retry exhaustion")
    elif args.observability:
        print("Three-worker admission/activity/reliable/datagram/outbound/deadline/catch-up scrapes, locked and peer-loss health checkpoints, crash expiry, leave retention and process-restart counter reset passed with full formation churn")
    if args.lost_join_ack:
        print("deliberate post-insertion JoinAck loss recovered the original assignment across worker processes")
    if args.client_pressure:
        print("Slow client bodies saturated mutation capacity; reads and peer policy progressed, released capacity restored control; full churn and shutdown with unfinished clients passed")
    elif args.policy_partition:
        print("Peer partition preserved operator access; competing policy versions converged after healing; full churn passed")
    elif args.lost_leave_response:
        print("Accepted leave response was discarded; exact CLI retry recovered the receipt without a second identity change; full churn passed")
    elif args.lost_departure:
        print("Both departure announcements were dropped; SWIM detected departure and same-certificate readmission plus full churn passed")
    elif args.peer_ejection:
        print("Joined worker learned self-removal over real peer gossip, stopped participation and required explicit leave to become standalone")
    elif args.excluded_restart:
        print("Restarted source retained its blocked certificate; fresh identities and a fresh admission attempt did not bypass exclusion")
    elif args.blocked_assignment:
        print("Blocked accepted certificate refused recovery without replacement identity; original source exhausted retries and required operator stop")
    elif args.removed_assignment:
        print("Removed accepted assignment refused recovery without readmission; original source exhausted retries and required operator stop")
    elif args.dead_assignment:
        print("Dead accepted assignment refused recovery without revival or duplicate identity; original source exhausted retries and required operator stop")
    elif args.source_loss:
        print("source crash lost operation history while surviving issuer retained original assignment; inspection did not authorize readmission")
    elif args.issuer_loss:
        print("issuer loss retained uncertainty through retry exhaustion; subsequent source crash lost history without restoring assignment (operator stop required)")
    elif args.handoff_only:
        print("three-worker handoff diagnostic passed (leave/crash churn not exercised)")
    elif args.admission_only:
        print("admission-only diagnostic journey passed (handoff/churn/full conformance not exercised)")
    else:
        print("formation CLI handoff, leave, crash/restart and readmission passed (full fault conformance pending)")


if __name__ == "__main__":
    main()
