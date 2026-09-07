"""Bounded loopback-only UDP fault relay for real formation process tests.

Payloads remain opaque (including QUIC/TLS); no credentials or packet bytes
are retained as evidence. One backend socket per source preserves reply routing.
This helper is not shipped in workers and requires no network-admin privileges.
"""

import ipaddress
import selectors
import socket
import threading


class UdpRelay:
    """Relay one advertised loopback address to one fixed worker endpoint.

    At most eight source routes and one datagram per ready socket are processed
    per iteration. Partition changes synchronize with forwarding: once the
    setter returns, no subsequent receive is forwarded until healing. Already
    forwarded packets can still be in flight. Packets dropped during partition
    are never queued for replay. Backend sockets retain their source ports across
    healing so the relay itself does not manufacture a QUIC migration.
    """

    MAX_ROUTES = 8

    def __init__(self, target):
        if (not ipaddress.ip_address(target[0]).is_loopback
                or ipaddress.ip_address(target[0]).version != 4
                or not 0 < target[1] <= 65535):
            raise ValueError("relay target must be an IPv4 loopback endpoint")
        self.target = target
        self.front = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.front.bind(("127.0.0.1", 0))
        self.front.setblocking(False)
        self.address = self.front.getsockname()
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.front, selectors.EVENT_READ, None)
        self.routes = {}
        self.lock = threading.Lock()
        self.stopping = threading.Event()
        self.partitioned = False
        self.forwarded = 0
        self.dropped = 0
        self.error = None
        self.thread = threading.Thread(target=self._run, name="formation-udp-relay", daemon=True)
        self.thread.start()

    def partition(self, enabled):
        with self.lock:
            self._check()
            self.partitioned = enabled

    def stats(self):
        with self.lock:
            self._check()
            return {"routes": len(self.routes), "forwarded": self.forwarded,
                    "dropped": self.dropped}

    def _check(self):
        if self.error is not None:
            raise RuntimeError("UDP relay failed") from self.error

    def _run(self):
        try:
            while not self.stopping.is_set():
                for key, _ in self.selector.select(timeout=0.02):
                    with self.lock:
                        try:
                            payload, source = key.fileobj.recvfrom(65535)
                        except BlockingIOError:
                            continue
                        except ConnectionRefusedError:
                            # A worker may be down during the process journey.
                            # ICMP refusal is transport loss, not relay failure.
                            self.dropped += 1
                            continue
                        if self.partitioned:
                            self.dropped += 1
                            continue
                        if key.fileobj is self.front:
                            backend = self.routes.get(source)
                            if backend is None:
                                if len(self.routes) >= self.MAX_ROUTES:
                                    self.dropped += 1
                                    continue
                                backend = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                                backend.bind(("127.0.0.1", 0))
                                backend.connect(self.target)
                                backend.setblocking(False)
                                self.routes[source] = backend
                                self.selector.register(backend, selectors.EVENT_READ, source)
                            destination, address = backend, self.target
                        else:
                            destination, address = self.front, key.data
                        try:
                            destination.sendto(payload, address)
                            self.forwarded += 1
                        except (BlockingIOError, ConnectionRefusedError):
                            self.dropped += 1
        except Exception as error:
            with self.lock:
                self.error = error

    def close(self):
        self.stopping.set()
        self.thread.join(timeout=1)
        if self.thread.is_alive():
            raise RuntimeError("UDP relay did not stop within one second")
        self.selector.close()
        self.front.close()
        for backend in self.routes.values():
            backend.close()
        self._check()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
