"""Deterministic real-socket early-rejection regression for the CLI harness."""

import http.client
import importlib.util
import io
import json
import subprocess
import socket
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("formation_cli", Path(__file__).with_name("check-formation-cli.py"))
formation_cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(formation_cli)


class EarlyRejectSocket:
    """Schedule the server's early response before the client's body write."""

    def __init__(self, client, server, response):
        self.client, self.server, self.response = client, server, response
        self.sent_headers = False

    def sendall(self, data):
        self.client.sendall(data)
        if not self.sent_headers:
            self.sent_headers = True
            self.server.recv(4096)
            self.server.sendall(self.response)
            self.server.shutdown(socket.SHUT_RD)
            if not self.response:
                self.server.shutdown(socket.SHUT_WR)

    def __getattr__(self, name):
        return getattr(self.client, name)


class ExecutableSnapshotTests(unittest.TestCase):
    def test_replacing_build_outputs_cannot_change_restart_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "worker"
            source.write_bytes(b"original feature build")
            source.chmod(0o700)
            copied = root / "snapshot"
            formation_cli.snapshot_executable(source, copied)
            replacement = root / "replacement"
            replacement.write_bytes(b"different feature build")
            replacement.replace(source)
            self.assertEqual(copied.read_bytes(), b"original feature build")
            self.assertEqual(copied.stat().st_mode & 0o777, 0o700)


class EarlyRejectionTests(unittest.TestCase):
    def check_response(self, response):
        client, server = socket.socketpair()
        client.settimeout(1)
        server.settimeout(1)
        connection = http.client.HTTPConnection("localhost", timeout=1)
        connection.sock = EarlyRejectSocket(client, server, response)
        try:
            formation_cli.assert_http_rejection(connection, "/early", b"body",
                                                {"Content-Type": "application/cbor"}, 400)
        finally:
            connection.close()
            server.close()

    def test_early_response_survives_broken_request_body_pipe(self):
        self.check_response(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")

    def test_wrong_http_status_still_fails(self):
        with self.assertRaises(AssertionError):
            self.check_response(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")

    def test_transport_close_without_response_is_not_rejection(self):
        with self.assertRaises(http.client.RemoteDisconnected):
            self.check_response(b"")

    def test_oversized_response_still_fails(self):
        with self.assertRaises(AssertionError):
            self.check_response(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 5000\r\nConnection: close\r\n\r\n" + b"x" * 5000)


class ResponseLossProxyTests(unittest.TestCase):
    def exercise(self, status):
        request = b"POST /leave HTTP/1.1\r\nContent-Length: 4\r\nAuthorization: Bearer fixture\r\n\r\nbody"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener, \
                    socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as upstream:
                listener.bind(str(root / "proxy.sock"))
                listener.listen(1)
                upstream.bind(str(root / "worker.sock"))
                upstream.listen(1)
                upstream.settimeout(2)

                def worker():
                    connection, _ = upstream.accept()
                    with connection:
                        connection.settimeout(2)
                        received = b""
                        while len(received) < len(request):
                            chunk = connection.recv(len(request) - len(received))
                            self.assertTrue(chunk)
                            received += chunk
                        self.assertEqual(received, request)
                        connection.sendall(b"HTTP/1.1 " + status + b" Fixture\r\nContent-Length: 7\r\n\r\nreceipt")

                with ThreadPoolExecutor(max_workers=2) as executor:
                    served = executor.submit(worker)
                    discarded = executor.submit(formation_cli.discard_one_http_response, listener, root / "worker.sock")
                    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                        client.settimeout(2)
                        client.connect(str(root / "proxy.sock"))
                        client.sendall(request)
                        self.assertEqual(client.recv(1), b"", "proxy leaked response bytes")
                    served.result(timeout=3)
                    return discarded.result(timeout=3)

    def test_accepted_request_is_forwarded_but_response_is_lost(self):
        self.assertEqual(self.exercise(b"200"), 200)

    def test_rejected_request_cannot_count_as_lost_acceptance(self):
        with self.assertRaises(AssertionError):
            self.exercise(b"401")


class IncompleteBodyTests(unittest.TestCase):
    def exercise(self, response, accepted):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            path = Path(directory) / "worker.sock"
            listener = stack.enter_context(socket.socket(socket.AF_UNIX, socket.SOCK_STREAM))
            listener.settimeout(2)
            listener.bind(str(path))
            listener.listen(1)

            def worker():
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(1)
                    headers = bytearray()
                    while not headers.endswith(b"\r\n\r\n"):
                        chunk = connection.recv(1)
                        self.assertTrue(chunk)
                        headers.extend(chunk)
                        self.assertLess(len(headers), 4096)
                    self.assertIn(b"Content-Length: 4096\r\n", headers)
                    self.assertIn(b"Expect: 100-continue\r\n", headers)
                    connection.sendall(response)
                    if accepted:
                        self.assertEqual(connection.recv(1), b"\xa0")
                        # No remaining body bytes are supplied by the helper.
                        connection.settimeout(0.05)
                        with self.assertRaises(socket.timeout):
                            connection.recv(1)

            with ThreadPoolExecutor(max_workers=1) as executor:
                served = executor.submit(worker)
                if accepted:
                    formation_cli.hold_mutation_body(stack, path, "fixture")
                else:
                    with self.assertRaises(AssertionError):
                        formation_cli.hold_mutation_body(stack, path, "fixture")
                served.result(timeout=3)

    def test_continue_leaves_body_incomplete(self):
        self.exercise(b"HTTP/1.1 100 Continue\r\n\r\n", True)

    def test_rejection_is_not_admitted_body(self):
        self.exercise(b"HTTP/1.1 503 Unavailable\r\n\r\n", False)

    def test_close_is_not_admitted_body(self):
        self.exercise(b"", False)


class IncidentInspectionTests(unittest.TestCase):
    def setUp(self):
        self.view = {"schemaVersion": 1, "formationId": "formation", "sourceNodeId": "source",
                     "nodeId": "target", "certFingerprint": "pin", "view": "localAtRequest",
                     "workerName": "same-label", "liveness": "alive"}

    def check_view(self, view):
        formation_cli.assert_incident_member(view, "formation", "source", "target", "pin")

    def test_liveness_changes_preserve_exact_identity_and_local_source(self):
        for state in ("alive", "suspected", "dead"):
            self.check_view(dict(self.view, liveness=state))

    def test_correct_label_cannot_hide_wrong_identity_schema_or_source(self):
        for field in ("schemaVersion", "formationId", "sourceNodeId", "nodeId", "certFingerprint", "view", "liveness"):
            with self.subTest(field=field), self.assertRaises(AssertionError):
                self.check_view(dict(self.view, **{field: "wrong"}))

    def test_missing_and_non_resource_inspections_fail(self):
        for value in (None, [], {}, {key: value for key, value in self.view.items() if key != "nodeId"}):
            with self.subTest(value=value), self.assertRaises(AssertionError):
                self.check_view(value)


class ReadmissionViewTests(unittest.TestCase):
    def setUp(self):
        self.expected = {"a": ("pin-a", "alive"), "b": ("pin-b", "alive"),
                         "departed": ("pin-c", "dead"), "readmitted": ("pin-c", "alive")}
        self.nodes = [{"nodeId": identity, "certFingerprint": pin, "liveness": state,
                       "workerName": "same-label"}
                      for identity, (pin, state) in self.expected.items()]

    def test_exact_records_are_order_independent(self):
        self.assertTrue(formation_cli.readmission_view_matches(self.nodes[::-1], self.expected))

    def test_correct_counts_do_not_hide_identity_binding_or_liveness_errors(self):
        variants = []
        for field, value in [("nodeId", "unknown"), ("nodeId", "a"),
                             ("certFingerprint", "wrong")]:
            changed = [dict(node) for node in self.nodes]
            changed[-1][field] = value
            variants.append(changed)
        changed = [dict(node) for node in self.nodes]
        changed[0]["liveness"], changed[2]["liveness"] = "dead", "alive"
        variants.append(changed)
        for nodes in variants:
            with self.subTest(nodes=nodes):
                self.assertEqual(len(nodes), 4)
                self.assertEqual(sum(node["liveness"] == "alive" for node in nodes), 3)
                self.assertFalse(formation_cli.readmission_view_matches(nodes, self.expected))

    def test_malformed_missing_and_extra_records_do_not_match(self):
        for nodes in [None, {}, self.nodes[:-1], self.nodes + self.nodes[:1],
                      [None] * 4, [{}] * 4, [{"nodeId": []}] * 4]:
            with self.subTest(nodes=nodes):
                self.assertFalse(formation_cli.readmission_view_matches(nodes, self.expected))


class ReadmissionEvidenceTests(unittest.TestCase):
    def test_late_observation_preserves_missing_and_later_present_evidence(self):
        evidence = formation_cli.Evidence("fixture")
        calls = []
        node = {"nodeId": "new-node", "certFingerprint": "expected-pin", "liveness": "alive"}

        def read(index, command):
            calls.append((index, command))
            nodes = [] if len(calls) <= 3 else [node]
            result = subprocess.CompletedProcess([], 0, json.dumps(nodes).encode(), b"")
            evidence.observe(index, "ls", result)
            return result

        with patch.object(formation_cli.time, "sleep") as sleep:
            formation_cli.capture_readmission_failure(
                read, evidence, {"readmitted": ("new-node", "expected-pin")}, observe_late=True)
        sleep.assert_called_once_with(20)
        self.assertEqual(calls, [(index, ["ls"]) for index in range(3)] * 2)
        self.assertEqual(len(evidence.recent), 12, "both captures fit with actual CLI evidence")
        projections = [item for item in evidence.recent if item["verb"] != "ls"]
        self.assertEqual([item["verb"] for item in projections],
                         ["readmission-diagnostic"] * 3 + ["readmission-late-diagnostic"] * 3)
        reports = [json.loads(item["stdout"]) for item in projections]
        self.assertTrue(all(report["roles"]["readmitted"]["records"] == 0 for report in reports[:3]))
        self.assertTrue(all(report["roles"]["readmitted"] ==
                            {"records": 1, "states": ["alive"], "bindingMatches": True}
                            for report in reports[3:]))

    def test_normal_failure_capture_does_not_wait_or_repeat(self):
        evidence = formation_cli.Evidence("fixture")
        with patch.object(formation_cli.time, "sleep") as sleep, \
                patch.object(formation_cli, "capture_readmission_views") as capture:
            formation_cli.capture_readmission_failure(None, evidence, {})
        sleep.assert_not_called()
        capture.assert_called_once_with(None, evidence, {})

    def test_late_observation_lookup_errors_remain_unavailable_and_bounded(self):
        evidence = formation_cli.Evidence("fixture")
        calls = []

        def read(index, command):
            calls.append((index, command))
            raise subprocess.TimeoutExpired("fixture", 3)

        with patch.object(formation_cli.time, "sleep") as sleep:
            formation_cli.capture_readmission_failure(read, evidence, {}, observe_late=True)
        sleep.assert_called_once_with(20)
        self.assertEqual(len(calls), 6)
        self.assertEqual([json.loads(item["stdout"]) for item in evidence.recent],
                         [{"unavailable": True}] * 6)

    def test_roles_survive_redaction_without_exporting_identities(self):
        expected = {"departed": ("a" * 64, "b" * 64),
                    "readmitted": ("c" * 64, "b" * 64)}
        nodes = [{"nodeId": "a" * 64, "certFingerprint": "b" * 64, "liveness": "dead"},
                 {"nodeId": "c" * 64, "certFingerprint": "wrong", "liveness": "suspected"},
                 {"nodeId": "extra", "name": "do-not-export", "liveness": "untrusted text"}]
        evidence = formation_cli.Evidence("fixture")
        read = lambda index, command: subprocess.CompletedProcess([], 0, json.dumps(nodes).encode(), b"")
        formation_cli.capture_readmission_views(read, evidence, expected)
        self.assertEqual(len(evidence.recent), 3)
        for item in evidence.recent:
            report = json.loads(item["stdout"])
            self.assertEqual(report["nodes"], 3)
            self.assertEqual(report["unexpectedRecords"], 1)
            self.assertEqual(report["roles"]["departed"],
                             {"records": 1, "states": ["dead"], "bindingMatches": True})
            self.assertEqual(report["roles"]["readmitted"],
                             {"records": 1, "states": ["suspected"], "bindingMatches": False})
            for forbidden in ["a" * 64, "b" * 64, "c" * 64, "wrong", "do-not-export", "untrusted text"]:
                self.assertNotIn(forbidden, item["stdout"])

    def test_missing_identity_is_distinct_from_failed_read(self):
        evidence = formation_cli.Evidence("fixture")
        calls = []
        def read(index, command):
            calls.append((index, command))
            if index == 0:
                raise subprocess.TimeoutExpired("fixture", 3)
            return subprocess.CompletedProcess([], index == 1, b"[]", b"")
        formation_cli.capture_readmission_views(read, evidence, {"readmitted": ("node", "pin")})
        self.assertEqual(calls, [(index, ["ls"]) for index in range(3)])
        reports = [json.loads(item["stdout"]) for item in evidence.recent]
        self.assertEqual(reports[:2], [{"unavailable": True}] * 2)
        self.assertEqual(reports[2]["roles"]["readmitted"],
                         {"records": 0, "states": [], "bindingMatches": False})

    def test_malformed_or_oversized_capture_is_unavailable(self):
        for payload in [b"not-json", b"{}", b"[null]", json.dumps([{}] * 17).encode(), b" " * 65537]:
            with self.subTest(payload=payload):
                evidence = formation_cli.Evidence("fixture")
                read = lambda index, command: subprocess.CompletedProcess([], 0, payload, b"")
                formation_cli.capture_readmission_views(read, evidence, {"readmitted": ("node", "pin")})
                self.assertEqual([json.loads(item["stdout"]) for item in evidence.recent],
                                 [{"unavailable": True}] * 3)


class DiagnosticSubsetTests(unittest.TestCase):
    def test_readmission_subset_routes_without_claiming_full_conformance(self):
        output = io.StringIO()
        with patch("sys.argv", ["formation", "--readmission-only"]), \
                patch.object(formation_cli, "check_public_adoption") as journey, \
                redirect_stdout(output):
            formation_cli.main()
        self.assertEqual(journey.call_count, 1)
        self.assertEqual(journey.call_args.kwargs, {"readmission_only": True})
        self.assertIn("standalone/crash/restart/full conformance not exercised", output.getvalue())

    def test_readmission_subset_rejects_other_scenarios_before_startup(self):
        for option in ["--handoff-only", "--admission-only", "--lost-join-ack",
                       "--client-pressure", "--policy-partition", "--lost-leave-response"]:
            with self.subTest(option=option), \
                    patch("sys.argv", ["formation", "--readmission-only", option]), \
                    patch.object(formation_cli, "check_public_adoption") as journey, \
                    redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as failure:
                formation_cli.main()
            self.assertEqual(failure.exception.code, 2)
            journey.assert_not_called()

    def test_observability_rejects_partial_or_unmapped_scenarios_before_startup(self):
        for option in ["--handoff-only", "--readmission-only", "--admission-only",
                       "--source-loss", "--client-pressure", "--policy-partition"]:
            with self.subTest(option=option), \
                    patch("sys.argv", ["formation", "--observability", option]), \
                    patch.object(formation_cli, "check_public_adoption") as journey, \
                    redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as failure:
                formation_cli.main()
            self.assertEqual(failure.exception.code, 2)
            journey.assert_not_called()

    def test_observability_cannot_combine_ejection_and_lost_ack(self):
        with patch("sys.argv", ["formation", "--observability", "--peer-ejection", "--lost-join-ack"]), \
                patch.object(formation_cli, "check_public_adoption") as journey, \
                redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as failure:
            formation_cli.main()
        self.assertEqual(failure.exception.code, 2)
        journey.assert_not_called()


if __name__ == "__main__":
    unittest.main()
