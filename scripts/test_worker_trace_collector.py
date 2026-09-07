"""Real-socket checks of the bounded collector used by the scrape harness."""
import http.client
import socket
import time
import unittest
from urllib.parse import urlsplit

from worker_trace_collector import trace_collector


class CollectorTests(unittest.TestCase):
    def test_failed_then_recovered_response(self):
        with trace_collector() as collector:
            address = urlsplit(collector.endpoint)
            for expected in (503, 200):
                connection = http.client.HTTPConnection(address.hostname, address.port, timeout=1)
                try:
                    body = b"\x0a\x00" if expected == 503 else bytes(1024)
                    connection.request("POST", address.path, body,
                                       {"Content-Type": "application/x-protobuf"})
                    response = connection.getresponse()
                    self.assertEqual(response.status, expected)
                    self.assertEqual(response.getheader("Content-Type"), "application/x-protobuf")
                    self.assertEqual(response.read(1), b"")
                finally:
                    connection.close()
                collector.recover()
                collector.check()

    def test_oversized_body_is_refused_before_read(self):
        with self.assertRaisesRegex(AssertionError, "bounded collector fixture failed"):
            with trace_collector() as collector:
                address = urlsplit(collector.endpoint)
                with socket.create_connection((address.hostname, address.port), timeout=2) as stream:
                    stream.sendall(b"POST /v1/traces HTTP/1.1\r\nContent-Type: application/x-protobuf\r\n"
                                   b"Content-Length: 1025\r\n\r\n")
                    self.assertEqual(stream.recv(1), b"")

    def test_silent_connection_expires_and_cleanup_is_bounded(self):
        started = time.monotonic()
        with self.assertRaisesRegex(AssertionError, "bounded collector fixture failed"):
            with trace_collector() as collector:
                address = urlsplit(collector.endpoint)
                with socket.create_connection((address.hostname, address.port), timeout=2) as stream:
                    self.assertEqual(stream.recv(1), b"")
        self.assertLess(time.monotonic() - started, 3)


if __name__ == "__main__":
    unittest.main()
