"""Bounded real-socket controls for the monitoring proxy evidence helper."""
import importlib.util
from pathlib import Path
import socket
import ssl
import time
import unittest

spec = importlib.util.spec_from_file_location(
    "monitoring_proxy", Path(__file__).with_name("check-worker-monitoring-proxy.py"))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class ProxyEvidenceTests(unittest.TestCase):
    @staticmethod
    def event(evidence, message, now=1, identity=7):
        evidence.consume(b"2026/09/07 00:00:00 [debug] 1#1: *" + str(identity).encode()
                         + b" " + message + b"\n", now)

    def test_only_correlated_response_write_want_write_proves_pressure(self):
        evidence = check.ProxyWriteEvidence()
        self.event(evidence, b"accept: 127.0.0.1:1234 fd:9")
        self.event(evidence, b"SSL_write: -1")
        self.event(evidence, b"SSL_get_error: 3")
        self.assertIsNone(evidence.snapshot(1234)["blocked"])
        self.event(evidence, b'http request line: "GET /metrics HTTP/1.1"')
        self.event(evidence, b"SSL_write: 1024")
        self.event(evidence, b"SSL_read: -1")
        self.event(evidence, b"SSL_get_error: 3")
        self.assertIsNone(evidence.snapshot(1234)["blocked"])
        self.event(evidence, b"SSL_write: -1")
        self.event(evidence, b"SSL_get_error: 3", identity=8)
        self.assertIsNone(evidence.snapshot(1234)["blocked"])
        self.event(evidence, b"SSL_get_error: 2")
        self.assertIsNone(evidence.snapshot(1234)["blocked"])
        self.event(evidence, b"SSL_write: -1")
        self.event(evidence, b"SSL_get_error: 3", now=2)
        self.event(evidence, b"client timed out while reading client request", now=6)
        self.assertIsNone(evidence.snapshot(1234)["expired"])
        self.event(evidence, b"client timed out (110: Connection timed out) while sending to client", now=7)
        self.event(evidence, b"close http connection: 9", now=7.1)
        state = evidence.snapshot(1234)
        self.assertEqual((state["requests"], state["written"]), (1, 1024))
        self.assertEqual((state["blocked"], state["expired"], state["closed"]), (2, 7, 7.1))

    def test_close_alone_does_not_establish_write_expiry(self):
        evidence = check.ProxyWriteEvidence()
        self.event(evidence, b"accept: 127.0.0.1:1234 fd:9")
        self.event(evidence, b"close http connection: 9")
        self.assertIsNone(evidence.snapshot(1234)["blocked"])
        self.assertIsNone(evidence.snapshot(1234)["expired"])

    def test_debug_observer_rejects_overlong_lines_and_connection_inventory(self):
        evidence = check.ProxyWriteEvidence()
        evidence.consume(b"x" * 8193, 1)
        with self.assertRaisesRegex(AssertionError, "byte budget"):
            evidence.snapshot(1234)
        evidence = check.ProxyWriteEvidence()
        for identity in range(65):
            self.event(evidence, b"accept: 127.0.0.1:1234 fd:9", identity=identity)
        with self.assertRaisesRegex(AssertionError, "inventory"):
            evidence.snapshot(1234)

    def test_debug_observer_bounds_total_input_and_discards_raw_fields(self):
        evidence = check.ProxyWriteEvidence()
        self.event(evidence, b"accept: 127.0.0.1:1234 fd:9")
        self.event(evidence, b'http header: "Authorization: Bearer seeded-secret"')
        self.assertNotIn("seeded-secret", repr(evidence.__dict__))
        for _ in range(1025):
            evidence.consume(b"x" * 8192, 1)
        with self.assertRaisesRegex(AssertionError, "byte budget"):
            evidence.snapshot(1234)

    def test_client_hello_requires_a_server_flight(self):
        hello = check.client_hello(ssl.create_default_context())
        self.assertEqual(hello[0], 22)
        self.assertLessEqual(len(hello), 16384)

    def test_received_bytes_are_retained_until_server_eof(self):
        client, server = socket.socketpair()
        with client, server:
            server.sendall(b"bounded response")
            server.shutdown(socket.SHUT_WR)
            self.assertEqual(check.read_until_closed(client, time.monotonic() + 1, 16),
                             b"bounded response")
            self.assertGreaterEqual(server.fileno(), 0)

    def test_no_eof_is_not_a_passing_timeout(self):
        client, server = socket.socketpair()
        with client, server:
            with self.assertRaises(TimeoutError):
                check.read_until_closed(client, time.monotonic() + 0.05, 16)
            self.assertGreaterEqual(server.fileno(), 0)

    def test_expired_deadline_cannot_accept_ready_eof(self):
        client, server = socket.socketpair()
        with client, server:
            server.shutdown(socket.SHUT_WR)
            with self.assertRaisesRegex(AssertionError, "deadline"):
                check.read_until_closed(client, time.monotonic() - 1, 16)

    def test_oversized_response_is_not_closure_evidence(self):
        client, server = socket.socketpair()
        with client, server:
            server.sendall(b"x" * 17)
            server.shutdown(socket.SHUT_WR)
            with self.assertRaisesRegex(AssertionError, "byte bound"):
                check.read_until_closed(client, time.monotonic() + 1, 16)


if __name__ == "__main__":
    unittest.main()
