"""Socket-level regression evidence for the process harness's UDP partition."""

import socket
import time
import unittest
from contextlib import ExitStack

from formation_udp_relay import UdpRelay


def udp(stack):
    result = stack.enter_context(socket.socket(socket.AF_INET, socket.SOCK_DGRAM))
    result.bind(("127.0.0.1", 0))
    result.settimeout(1)
    return result


def wait_stats(relay, predicate):
    deadline = time.monotonic() + 1
    while time.monotonic() < deadline:
        stats = relay.stats()
        if predicate(stats):
            return stats
        time.sleep(0.005)
    raise AssertionError(f"relay did not reach expected state: {relay.stats()}")


class RelayTests(unittest.TestCase):
    def test_distinct_sources_keep_replies_and_ports_across_partition(self):
        with ExitStack() as stack:
            worker = udp(stack)
            clients = [udp(stack), udp(stack)]
            relay = stack.enter_context(UdpRelay(worker.getsockname()))
            routes = []
            for index, client in enumerate(clients):
                packet = bytes([index]) + b"\x00opaque\xff"
                client.sendto(packet, relay.address)
                received, route = worker.recvfrom(65535)
                self.assertEqual(received, packet)
                routes.append(route)
                worker.sendto(packet, route)
                self.assertEqual(client.recvfrom(65535), (packet, relay.address))
            self.assertNotEqual(*routes)
            baseline = relay.stats()
            relay.partition(True)
            clients[0].sendto(b"drop-forward", relay.address)
            worker.sendto(b"drop-reverse", routes[1])
            stats = wait_stats(relay, lambda state: state["dropped"] == 2)
            self.assertEqual(stats["forwarded"], baseline["forwarded"])
            # Healing must not replay either dropped packet.
            relay.partition(False)
            clients[0].sendto(b"healed", relay.address)
            self.assertEqual(worker.recvfrom(65535), (b"healed", routes[0]))
            worker.sendto(b"healed-reply", routes[1])
            self.assertEqual(clients[1].recvfrom(65535), (b"healed-reply", relay.address))
        self.assertFalse(relay.thread.is_alive())

    def test_route_capacity_refuses_without_allocating_another_socket(self):
        with ExitStack() as stack:
            worker = udp(stack)
            relay = stack.enter_context(UdpRelay(worker.getsockname()))
            clients = [udp(stack) for _ in range(relay.MAX_ROUTES + 1)]
            for client in clients[:-1]:
                client.sendto(b"route", relay.address)
                self.assertEqual(worker.recvfrom(65535)[0], b"route")
            clients[-1].sendto(b"over-capacity", relay.address)
            stats = wait_stats(relay, lambda state: state["dropped"] == 1)
            self.assertEqual(stats["routes"], relay.MAX_ROUTES)
            clients[0].sendto(b"existing", relay.address)
            self.assertEqual(worker.recvfrom(65535)[0], b"existing")

    def test_reject_non_loopback_and_unspecified_targets(self):
        for target in [("0.0.0.0", 1000), ("192.0.2.1", 1000), ("127.0.0.1", 0)]:
            with self.assertRaises(ValueError):
                UdpRelay(target)


if __name__ == "__main__":
    unittest.main()
