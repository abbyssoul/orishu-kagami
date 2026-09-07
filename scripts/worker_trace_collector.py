"""Bounded loopback HTTP responder for the Prometheus trace-counter fixture.

This checks HTTP framing, not OTLP decoding or collector deployment. Worker
Rust tests separately verify the received protobuf and trace identities.
"""
import contextlib
import socket
import threading
import time


class Collector:
    def __init__(self, listener):
        self.endpoint = f"http://127.0.0.1:{listener.getsockname()[1]}/v1/traces"
        self.recovered = threading.Event()
        self.stopped = threading.Event()
        self.failed = threading.Event()

    def recover(self):
        self.recovered.set()

    def check(self):
        assert not self.failed.is_set(), "bounded collector fixture failed"

    def serve(self, listener):
        attempts = 0
        try:
            while not self.stopped.is_set():
                try:
                    stream, _ = listener.accept()
                except socket.timeout:
                    continue
                with stream:
                    attempts += 1
                    assert attempts <= 64, "unexpected collector request volume"
                    deadline = time.monotonic() + 1

                    def read(count):
                        remaining = deadline - time.monotonic()
                        assert remaining > 0, "collector input deadline"
                        stream.settimeout(remaining)
                        data = stream.recv(count)
                        assert data, "truncated collector input"
                        return data

                    head = bytearray()
                    while not head.endswith(b"\r\n\r\n"):
                        assert len(head) < 4096, "collector header bound"
                        head.extend(read(1))
                    lines = head.decode("ascii").lower().split("\r\n")
                    assert lines[0] == "post /v1/traces http/1.1"
                    headers = {}
                    for line in lines[1:-2]:
                        key, value = line.split(":", 1)
                        assert key not in headers
                        headers[key] = value.strip()
                    assert headers["content-type"] == "application/x-protobuf"
                    assert not {"authorization", "transfer-encoding", "content-encoding"} & headers.keys()
                    remaining = int(headers["content-length"])
                    assert 0 < remaining <= 1024
                    while remaining:
                        remaining -= len(read(remaining))
                    status = b"200 OK" if self.recovered.is_set() else b"503 Unavailable"
                    stream.sendall(b"HTTP/1.1 " + status +
                                   b"\r\nContent-Type: application/x-protobuf\r\n"
                                   b"Content-Length: 0\r\nConnection: close\r\n\r\n")
        except (AssertionError, OSError, ValueError, KeyError, UnicodeError):
            self.failed.set()


@contextlib.contextmanager
def trace_collector():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        listener.settimeout(0.1)
        collector = Collector(listener)
        thread = threading.Thread(target=collector.serve, args=(listener,), daemon=True)
        thread.start()
        try:
            yield collector
        finally:
            collector.stopped.set()
            thread.join(timeout=2)
            assert not thread.is_alive(), "collector cleanup deadline"
            collector.check()
