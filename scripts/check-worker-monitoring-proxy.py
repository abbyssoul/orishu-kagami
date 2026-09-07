#!/usr/bin/env python3
"""Exercise the checked-in monitoring mTLS proxy with real worker and Prometheus."""
import argparse
import contextlib
from concurrent.futures import ThreadPoolExecutor
import http.client
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent


def run(command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, timeout=15, **kwargs)


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


@contextlib.contextmanager
def process(command, environment, stderr=subprocess.DEVNULL):
    child = subprocess.Popen(command, env=environment, stdout=subprocess.DEVNULL,
                             stderr=stderr)
    try:
        yield child
    finally:
        if child.poll() is None:
            child.terminate()
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=3)
            raise AssertionError("process exceeded shutdown budget")
        assert child.returncode == 0, "fixture process exited unsuccessfully"


def get(address, path, context=None, method="GET", headers=None):
    connection = (http.client.HTTPSConnection("127.0.0.1", address, context=context, timeout=2)
                  if context else http.client.HTTPConnection("127.0.0.1", address, timeout=2))
    try:
        connection.request(method, path, headers=headers or {})
        response = connection.getresponse()
        body = response.read(32769)
        assert len(body) <= 32768, "response exceeded diagnostic byte budget"
        return response.status, dict(response.getheaders()), body
    finally:
        connection.close()


def poll(check, children):
    deadline = time.monotonic() + 15
    last = "no result"
    while time.monotonic() < deadline:
        assert all(child.poll() is None for child in children), "fixture process exited"
        try:
            result = check()
            if result:
                return result
            last = "condition false"
        except (OSError, http.client.HTTPException) as error:
            last = str(error)
        time.sleep(0.05)
    raise AssertionError("observation deadline expired: " + last)


def certificates(root):
    certs = root / "certs"
    certs.mkdir(mode=0o700)
    for name in ("monitoring-ca", "rogue-ca"):
        run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
             "-addext", "basicConstraints=critical,CA:TRUE",
             "-addext", "keyUsage=critical,keyCertSign,cRLSign",
             "-subj", f"/CN={name}", "-keyout", str(certs / f"{name}.key"),
             "-out", str(certs / f"{name}.crt")])
    for name, ca, usage in (("server", "monitoring-ca", "serverAuth"),
                            ("monitor", "monitoring-ca", "clientAuth"),
                            ("rogue", "rogue-ca", "clientAuth")):
        run(["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", f"/CN={name}",
             "-keyout", str(certs / f"{name}.key"), "-out", str(certs / f"{name}.csr")])
        extensions = certs / f"{name}.ext"
        extensions.write_text("basicConstraints=critical,CA:FALSE\n"
                              "keyUsage=critical,digitalSignature,keyEncipherment\n"
                              f"extendedKeyUsage={usage}\n"
                              "subjectAltName=DNS:worker-mgmt.example,IP:127.0.0.1\n")
        run(["openssl", "x509", "-req", "-in", str(certs / f"{name}.csr"),
             "-CA", str(certs / f"{ca}.crt"), "-CAkey", str(certs / f"{ca}.key"),
             "-CAcreateserial", "-days", "1", "-extfile", str(extensions),
             "-out", str(certs / f"{name}.crt")])
    shutil.copyfile(certs / "monitoring-ca.crt", certs / "server-ca.crt")
    for key in certs.glob("*.key"):
        key.chmod(0o600)
    return certs


def client_hello(context):
    """Generate a real TLS ClientHello without completing client authentication."""
    incoming, outgoing = ssl.MemoryBIO(), ssl.MemoryBIO()
    client = context.wrap_bio(incoming, outgoing, server_hostname="worker-mgmt.example")
    try:
        client.do_handshake()
    except ssl.SSLWantReadError:
        pass
    else:
        raise AssertionError("initial TLS handshake did not require server input")
    hello = outgoing.read()
    assert 5 < len(hello) <= 16384 and hello[0] == 22, "bounded TLS handshake record required"
    return hello


def read_until_closed(connection, deadline, limit):
    """Bound bytes and total observation time; timeout is never closure evidence."""
    received = bytearray()
    while True:
        remaining = deadline - time.monotonic()
        assert remaining > 0, "peer closure exceeded shared observation deadline"
        connection.settimeout(remaining)
        try:
            chunk = connection.recv(limit + 1 - len(received))
        except ConnectionResetError:
            break
        if not chunk:
            break
        received.extend(chunk)
        assert len(received) <= limit, "peer response exceeded fixture byte bound"
    return bytes(received)


class ProxyWriteEvidence:
    """Consume debug output without retaining headers, credentials or raw logs.

    The pinned Nginx build logs SSL_write followed by SSL_get_error. Only
    WANT_WRITE immediately following a failed write on that same connection
    proves downstream write pressure; handshake/read WANT_WRITE does not.
    """

    def __init__(self):
        self.lock = threading.Lock()
        self.connections = {}
        self.bytes = 0
        self.failure = None

    def consume(self, line, now):
        with self.lock:
            self.bytes += len(line)
            if len(line) > 8192 or self.bytes > 8 * 1024 * 1024:
                self.failure = "proxy debug evidence exceeded byte budget"
            if self.failure:
                return
            match = re.search(rb"\*(\d+) (.*)", line)
            if not match:
                return
            identity, message = int(match[1]), match[2].rstrip()
            accepted = re.fullmatch(rb"accept: 127\.0\.0\.1:(\d+) fd:\d+", message)
            if accepted:
                if len(self.connections) >= 64 or identity in self.connections:
                    self.failure = "proxy debug connection inventory exceeded bound or duplicated"
                    return
                self.connections[identity] = dict(port=int(accepted[1]), requests=0,
                                                 written=0, failed_write=False,
                                                 blocked=None, expired=None, closed=None)
            state = self.connections.get(identity)
            if state is None:
                return
            previous_failed = state["failed_write"]
            state["failed_write"] = message == b"SSL_write: -1"
            if message == b'http request line: "GET /metrics HTTP/1.1"':
                state["requests"] += 1
            written = re.fullmatch(rb"SSL_write: (\d+)", message)
            if written:
                state["written"] += int(written[1])
            if (previous_failed and message == b"SSL_get_error: 3"
                    and state["requests"] > 0 and state["blocked"] is None):
                state["blocked"] = now
            if (b"client timed out" in message and b"while sending to client" in message
                    and state["blocked"] is not None):
                state["expired"] = now
            if message.startswith(b"close http connection:"):
                state["closed"] = now

    def drain(self, stream):
        try:
            while line := stream.readline(8193):
                self.consume(line, time.monotonic())
        except Exception:
            with self.lock:
                self.failure = "proxy debug reader failed"

    def snapshot(self, client_port):
        with self.lock:
            assert self.failure is None, self.failure
            found = [dict(state) for state in self.connections.values()
                     if state["port"] == client_port]
            assert len(found) <= 1, "ambiguous proxy client correlation"
            return found[0] if found else None


def downstream_pressure(root, config, nginx, environment, trusted, worker_port,
                        proxy_port, worker_process, control, capacity=16):
    """Actual TLS write pressure with a finite pipeline and an unread client."""
    fixture = root / "downstream.conf"
    # Keep production timeouts, buffering and upstream. A second, explicitly
    # reduced request-capacity variant proves reuse of the sole occupied slot.
    assert capacity in (1, 16)
    fixture.write_text(config.replace("error_log stderr warn;", "error_log stderr debug;")
                       .replace("limit_conn monitoring 16;", f"limit_conn monitoring {capacity};"))
    evidence = ProxyWriteEvidence()
    reader = None
    try:
        with contextlib.ExitStack() as cleanup, process(
                nginx[:-1] + [str(fixture)], environment, stderr=subprocess.PIPE) as proxy:
            reader = threading.Thread(target=evidence.drain, args=(proxy.stderr,), daemon=True)
            reader.start()
            poll(lambda: get(proxy_port, "/readyz", trusted)[0] == 200,
                 [worker_process, proxy])
            request = b"GET /metrics HTTP/1.1\r\nHost: worker-mgmt.example\r\n\r\n"

            def connect():
                raw = cleanup.enter_context(socket.socket())
                # Client-only TCP settings create a small advertised receive
                # window/segment size; no proxy production limit is changed.
                raw.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
                raw.setsockopt(socket.IPPROTO_TCP, socket.TCP_MAXSEG, 512)
                raw.settimeout(2)
                raw.connect(("127.0.0.1", proxy_port))
                return cleanup.enter_context(trusted.wrap_socket(
                    raw, server_hostname="worker-mgmt.example"))

            # Same small-window TLS configuration succeeds when the client
            # actually reads. Keep this connection separate from stalled-write
            # evidence: transient WANT_WRITE with a healthy reader is expected.
            with connect() as healthy:
                for _ in range(3):
                    healthy.sendall(request)
                    with http.client.HTTPResponse(healthy) as response:
                        response.begin()
                        body = response.read(32769)
                        assert response.status == 200 and len(body) <= 32768
                        assert b"orishu_worker_ready 1" in body
            client = connect()
            client_port = client.getsockname()[1]
            started = time.monotonic()
            client.sendall(request * 100)

            def await_state(condition, deadline):
                while time.monotonic() < deadline:
                    assert proxy.poll() is None and worker_process.poll() is None
                    state = evidence.snapshot(client_port)
                    if state and condition(state):
                        return state
                    time.sleep(0.01)
                raise AssertionError("downstream evidence deadline: " +
                                     repr(evidence.snapshot(client_port)))

            blocked = await_state(lambda state: state["blocked"] is not None, started + 2)
            assert 0 < blocked["requests"] < 100 and 0 < blocked["written"] < 1024 * 1024
            time.sleep(0.1)
            stable = evidence.snapshot(client_port)
            assert (stable["requests"], stable["written"]) == (blocked["requests"], blocked["written"])
            assert stable["closed"] is None
            if capacity == 16:
                assert get(proxy_port, "/readyz", trusted)[0] == 200
                assert b"orishu_worker_ready 1" in get(proxy_port, "/metrics", trusted)[2]
            else:
                for path in ("/readyz", "/metrics"):
                    status, headers, body = get(proxy_port, path, trusted)
                    assert status == 429 and headers["Cache-Control"] == "no-store"
                    assert b"orishu_worker_" not in body
            assert get(worker_port, "/readyz")[0] == 200
            control()
            assert time.monotonic() - started < 4, "control must precede downstream expiry"
            closed = await_state(lambda state: state["closed"] is not None, started + 7)
            assert closed["expired"] is not None, "closure without timeout is not expiry evidence"
            assert 4 <= closed["expired"] - blocked["blocked"] < 7
            assert (closed["requests"], closed["written"]) == (stable["requests"], stable["written"])
            # The original client is still open and has never read any response.
            assert get(proxy_port, "/readyz", trusted)[0] == 200
            assert b"orishu_worker_ready 1" in get(proxy_port, "/metrics", trusted)[2]
            assert get(worker_port, "/readyz")[0] == 200
            print("downstream evidence: capacity=%d requests=%d written=%d expiry=%.3fs" % (
                capacity, closed["requests"], closed["written"],
                closed["expired"] - blocked["blocked"]))
    finally:
        if reader is not None:
            reader.join(timeout=3)
            assert not reader.is_alive(), "proxy debug reader exceeded shutdown budget"
            proxy.stderr.close()
            # Late recovery/shutdown output is subject to the same bounds.
            evidence.snapshot(None)


def pre_http_pressure(nginx, environment, trusted, anonymous, worker_port, proxy_port,
                      worker_process, control):
    """Observe pre-auth TLS and post-handshake incomplete-header expiry."""
    with contextlib.ExitStack() as cleanup, process(nginx, environment) as proxy:
        poll(lambda: get(proxy_port, "/readyz", trusted)[0] == 200,
             [worker_process, proxy])
        held = []
        started = time.monotonic()
        trickling = []
        for kind in ("tls", "headers"):
            for index in range(16):
                opened = time.monotonic()
                raw = socket.create_connection(("127.0.0.1", proxy_port), timeout=2)
                cleanup.callback(raw.close)
                if kind == "tls":
                    raw.sendall(client_hello(anonymous))
                    # A server handshake record proves this connection left
                    # the listen backlog. Never send the remaining client flight.
                    response = raw.recv(16384)
                    assert response and response[0] == 22, "expected real server TLS handshake"
                    connection = raw
                    remaining_bytes = 32768 - len(response)
                else:
                    connection = trusted.wrap_socket(raw, server_hostname="worker-mgmt.example")
                    cleanup.callback(connection.close)
                    assert connection.version() is not None
                    connection.sendall(b"GET /metrics HTTP/1.1\r\nHost: worker-mgmt.example\r\nX-Held: ")
                    if index >= 8:
                        trickling.append(connection)
                    remaining_bytes = 8192
                held.append((kind, connection, opened, remaining_bytes))
        assert time.monotonic() - started < 2, "pre-HTTP setup must precede expiry"

        # Partial header progress must not turn the whole-header budget into
        # an idle timeout. Do not read and write one SSL object concurrently.
        for offset in (2, 2.5, 3):
            time.sleep(max(0, started + offset - time.monotonic()))
            for connection in trickling:
                remaining = started + 4 - time.monotonic()
                assert remaining > 0, "header trickle exceeded pre-expiry budget"
                connection.settimeout(min(1, remaining))
                connection.sendall(b"x")

        def expired(kind, connection, opened, remaining_bytes):
            data = read_until_closed(connection, started + 7, remaining_bytes)
            assert time.monotonic() - opened >= 4, "early refusal is not expiry evidence"
            if kind == "headers":
                assert not data or data.startswith(b"HTTP/1.1 408 "), "unexpected partial-header response"
                assert b"orishu_worker_" not in data

        # Observe all EOFs concurrently so early refusal of one connection
        # cannot be hidden by waiting five seconds on another connection first.
        with ThreadPoolExecutor(max_workers=32) as readers:
            results = [readers.submit(expired, kind, connection, opened, remaining_bytes)
                       for kind, connection, opened, remaining_bytes in held]
            assert get(proxy_port, "/readyz", trusted)[0] == 200
            assert b"orishu_worker_ready 1" in get(proxy_port, "/metrics", trusted)[2]
            assert get(worker_port, "/readyz")[0] == 200
            control()
            assert time.monotonic() - started < 4, "control must finish before head/handshake expiry"
            for result in results:
                result.result()
        assert time.monotonic() - started < 7, "mixed pre-HTTP expiry budget"
        assert all(child.poll() is None for child in (worker_process, proxy))
        # Original client-side sockets remain open. Only Nginx closed the
        # expired sessions; neither fixture cleanup nor proxy restart assists.
        assert get(proxy_port, "/readyz", trusted)[0] == 200
        assert b"orishu_worker_ready 1" in get(proxy_port, "/metrics", trusted)[2]
        assert get(worker_port, "/readyz")[0] == 200


def pressure(root, config, nginx, environment, trusted, worker_port, proxy_port,
             worker_process, control, expire=False):
    """Hold real upstream requests until explicit release or proxy-driven expiry."""
    with contextlib.ExitStack() as cleanup:
        relay = cleanup.enter_context(socket.socket())
        relay.bind(("127.0.0.1", 0))
        relay.listen(32)
        relay.settimeout(2)
        relay_port = relay.getsockname()[1]
        fixture = root / "pressure.conf"
        fixture.write_text(config.replace(f"127.0.0.1:{worker_port}",
                                          f"127.0.0.1:{relay_port}"))
        with process(nginx[:-1] + [str(fixture)], environment) as proxy:
            # Use a rejected route for startup observation: it cannot occupy
            # the held upstream or manufacture a successful health response.
            poll(lambda: get(proxy_port, "/not-a-route", trusted)[0] == 404,
                 [worker_process, proxy])
            held = []
            started = time.monotonic()
            for _ in range(16):
                client = http.client.HTTPSConnection("127.0.0.1", proxy_port,
                                                     context=trusted, timeout=2)
                cleanup.callback(client.close)
                client.request("GET", "/metrics")
                incoming, _ = relay.accept()
                cleanup.callback(incoming.close)
                incoming.settimeout(2)
                request = bytearray()
                while not request.endswith(b"\r\n\r\n"):
                    chunk = incoming.recv(4097 - len(request))
                    assert chunk, "upstream closed before complete request"
                    request.extend(chunk)
                    assert len(request) <= 4096, "relay request exceeded bound"
                assert request.startswith(b"GET /metrics HTTP/1.1\r\n")
                held.append((client, incoming, request))
            # All sixteen requests reached a real upstream socket and remain
            # unanswered. This is active-request capacity, not listen backlog.
            status, headers, body = get(proxy_port, "/metrics", trusted)
            assert status == 429 and headers["Cache-Control"] == "no-store"
            assert b"orishu_worker_" not in body
            assert get(worker_port, "/readyz")[0] == 200
            control()
            assert time.monotonic() - started < 4, "must reach capacity before proxy read expiry"
            for _, incoming, request in (() if expire else held):
                with socket.create_connection(("127.0.0.1", worker_port), timeout=2) as upstream:
                    upstream.sendall(request)
                    response = bytearray()
                    # 32 KiB catalogue budget plus bounded header headroom.
                    while True:
                        chunk = upstream.recv(33793 - len(response))
                        if not chunk:
                            break
                        response.extend(chunk)
                        assert len(response) <= 33792, "relay response exceeded bound"
                    incoming.sendall(response)
                    incoming.shutdown(socket.SHUT_WR)
            for client, _, _ in held:
                if expire:
                    remaining = started + 7 - time.monotonic()
                    assert remaining > 0, "whole expiry observation deadline"
                    client.sock.settimeout(remaining)
                response = client.getresponse()
                body = response.read(32769)
                assert len(body) <= 32768
                if expire:
                    assert response.status == 504
                    assert response.getheader("Cache-Control") == "no-store"
                    assert b"orishu_worker_" not in body
                else:
                    assert response.status == 200
                    assert b"orishu_worker_ready 1" in body
            if expire:
                assert 4 <= time.monotonic() - started < 7, "observe timeout, not setup refusal"
                # Keep all fixture-side sockets open. EOF comes from Nginx;
                # cleanup must not manufacture the capacity release assertion.
                for _, incoming, _ in held:
                    remaining = started + 7 - time.monotonic()
                    assert remaining > 0, "upstream close exceeded shared expiry deadline"
                    incoming.settimeout(remaining)
                    assert incoming.recv(1) == b"", "expired upstream must be closed by proxy"
            else:
                for client, incoming, _ in held:
                    client.close()
                    incoming.close()
            # A new request must reach the relay and get a real worker reply.
            fresh = http.client.HTTPSConnection("127.0.0.1", proxy_port,
                                                context=trusted, timeout=2)
            cleanup.callback(fresh.close)
            fresh.request("GET", "/readyz")
            incoming, _ = relay.accept()
            cleanup.callback(incoming.close)
            incoming.settimeout(2)
            request = bytearray()
            while not request.endswith(b"\r\n\r\n"):
                chunk = incoming.recv(4097 - len(request))
                assert chunk, "fresh request ended before complete headers"
                request.extend(chunk)
                assert len(request) <= 4096
            assert request.startswith(b"GET /readyz HTTP/1.1\r\n")
            # No invented healthy reply: forward the actual worker response.
            with socket.create_connection(("127.0.0.1", worker_port), timeout=2) as upstream:
                upstream.sendall(request)
                response = bytearray()
                while True:
                    chunk = upstream.recv(4097 - len(response))
                    if not chunk:
                        break
                    response.extend(chunk)
                    assert len(response) <= 4096
                incoming.sendall(response)
                incoming.shutdown(socket.SHUT_WR)
            response = fresh.getresponse()
            assert response.status == 200 and len(response.read(1025)) <= 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("nginx", "worker", "ctl", "promtool", "prometheus"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    binaries = {name: str(Path(shutil.which(str(value)) or value).resolve())
                for name, value in vars(args).items()}
    version = run([binaries["nginx"], "-v"])
    assert b"nginx/1.28.0" in version.stderr, "use the documented proxy test version"
    assert b"--with-debug" in run([binaries["nginx"], "-V"]).stderr, "write-pressure evidence requires Nginx debug support"
    for name in ("promtool", "prometheus"):
        version = run([binaries[name], "--version"])
        assert b"version 3.5.0 " in version.stdout + version.stderr
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith("ORISHU_")}
    with tempfile.TemporaryDirectory(prefix="orishu-monitoring-proxy-") as directory:
        root = Path(directory)
        # Keep every CLI invocation on the same build as the running worker,
        # even if another task subsequently rebuilds the source target paths.
        for name in ("worker", "ctl"):
            executable = root / name
            shutil.copyfile(binaries[name], executable)
            executable.chmod(0o700)
            binaries[name] = str(executable)
        certs = certificates(root)
        ports = set()
        for _ in range(16):
            ports.add(port())
            if len(ports) == 3:
                break
        assert len(ports) == 3, "could not select distinct local ports"
        worker_port, proxy_port, prom_port = sorted(ports)
        config = (ROOT / "etc/nginx-worker-monitoring.conf.example").read_text()
        config = config.replace("192.0.2.10:9443", f"127.0.0.1:{proxy_port}")
        config = config.replace("127.0.0.1:9168", f"127.0.0.1:{worker_port}")
        (root / "nginx.conf").write_text(config)
        nginx = [binaries["nginx"], "-e", "stderr", "-p", str(root) + "/", "-c", str(root / "nginx.conf")]
        validation = subprocess.run(nginx + ["-t"], capture_output=True, timeout=10)
        assert validation.returncode == 0, validation.stderr.decode()
        worker = [binaries["worker"], "--state-dir", str(root / "state"),
                  "--listen.clients", str(root / "api.sock"),
                  "--observability.enabled", "true", "--observability.bind",
                  f"127.0.0.1:{worker_port}"]
        trusted = ssl.create_default_context(cafile=str(certs / "server-ca.crt"))
        trusted.load_cert_chain(certs / "monitor.crt", certs / "monitor.key")
        anonymous = ssl.create_default_context(cafile=str(certs / "server-ca.crt"))
        rogue = ssl.create_default_context(cafile=str(certs / "server-ca.crt"))
        rogue.load_cert_chain(certs / "rogue.crt", certs / "rogue.key")
        with process(worker, environment) as worker_process:
            poll(lambda: get(worker_port, "/readyz")[0] == 200, [worker_process])
            with process(nginx, environment) as proxy:
                children = [worker_process, proxy]
                def ready():
                    status, _, body = get(proxy_port, "/readyz", trusted)
                    assert status == 200, (status, body)
                    return True
                poll(ready, children)
                # Monitoring clients must verify the proxy too. An unexpected
                # server CA or hostname fails TLS, not just application auth.
                wrong_server_ca = ssl.create_default_context(cafile=str(certs / "rogue-ca.crt"))
                wrong_server_ca.load_cert_chain(certs / "monitor.crt", certs / "monitor.key")
                try:
                    get(proxy_port, "/metrics", wrong_server_ca)
                except ssl.SSLCertVerificationError:
                    pass
                else:
                    raise AssertionError("scraper accepted an untrusted server certificate")
                with socket.create_connection(("127.0.0.1", proxy_port), timeout=2) as raw:
                    try:
                        with trusted.wrap_socket(raw, server_hostname="wrong-worker.example"):
                            raise AssertionError("scraper accepted a mismatched server hostname")
                    except ssl.SSLCertVerificationError:
                        pass
                for path in ("/metrics", "/livez", "/readyz", "/startupz"):
                    status, headers, body = get(proxy_port, path, trusted)
                    assert status == 200 and headers["Cache-Control"] == "no-store"
                    if path == "/metrics":
                        run([binaries["promtool"], "check", "metrics"], input=body)
                        assert b"orishu_worker_ready 1" in body
                    for context in (anonymous, rogue):
                        status, _, body = get(proxy_port, path, context)
                        assert status in (400, 403), "untrusted monitoring client was admitted"
                        assert b"orishu_worker_" not in body
                for path in ("/", "/cluster", "/membership", "/metrics?x=1", "/%6detrics", "/METRICS"):
                    assert get(proxy_port, path, trusted)[0] == 404
                for method in ("POST", "PUT", "DELETE", "HEAD"):
                    assert get(proxy_port, "/metrics", trusted, method)[0] == 405
                # A monitoring credential is not an operator credential. The
                # proxy has no mutation route, and the actual client API must
                # independently refuse its fingerprint as a bearer token.
                ctl = [binaries["ctl"], "--host", str(root / "api.sock"), "--output", "json"]
                before = json.loads(run(ctl + ["cluster", "info"], env=environment).stdout)
                def mutation(action, operation):
                    return ["cluster", action, "--formation-id", before["formationId"],
                            "--operation-id", operation]
                fingerprint = hashlib.sha256(ssl.PEM_cert_to_DER_cert(
                    (certs / "monitor.crt").read_text())).hexdigest()
                (root / "monitor.token").write_text(fingerprint)
                (root / "monitor.token").chmod(0o600)
                refused = subprocess.run(ctl + ["--operator-token-file", str(root / "monitor.token")]
                                         + mutation("lock", "monitor-refused"), env=environment,
                                         capture_output=True, timeout=5)
                assert refused.returncode == 1, "require command refusal, not argument parsing failure"
                after = json.loads(run(ctl + ["cluster", "info"], env=environment).stdout)
                assert before == after and after["locked"] is False
                token = (root / "state/operator.token").read_text().strip()
                assert get(proxy_port, "/metrics", anonymous,
                           headers={"Authorization": "Bearer " + token})[0] in (400, 403)
                administrator = ctl + ["--operator-token-file", str(root / "state/operator.token")]
                run(administrator + mutation("lock", "monitor-admin-lock"), env=environment)
                assert json.loads(run(ctl + ["cluster", "info"], env=environment).stdout)["locked"] is True
                run(administrator + mutation("unlock", "monitor-admin-unlock"), env=environment)
                assert json.loads(run(ctl + ["cluster", "info"], env=environment).stdout) == before
                assert token.encode() not in get(proxy_port, "/metrics", trusted)[2]
                scrape = (ROOT / "etc/prometheus-worker-mtls.yml").read_text()
                scrape = scrape.replace("worker-mgmt.example:9443", f"127.0.0.1:{proxy_port}")
                scrape = scrape.replace("certs/", str(certs) + "/")
                (root / "prometheus.yml").write_text(scrape)
                run([binaries["promtool"], "check", "config", str(root / "prometheus.yml")])
                command = [binaries["prometheus"], "--config.file", str(root / "prometheus.yml"),
                           "--web.listen-address", f"127.0.0.1:{prom_port}",
                           "--storage.tsdb.path", str(root / "tsdb")]
                with process(command, environment) as prometheus:
                    query = urllib.parse.urlencode({"query": 'orishu_worker_ready{job="orishu-worker-mtls"}'})
                    def scraped():
                        status, _, body = get(prom_port, "/api/v1/query?" + query)
                        return status == 200 and any(
                            item["value"][1] == "1"
                            for item in json.loads(body)["data"]["result"])
                    poll(scraped, children + [prometheus])
            # Losing the proxy does not change worker readiness or identity.
            assert get(worker_port, "/readyz")[0] == 200
            assert json.loads(run(ctl + ["cluster", "info"], env=environment).stdout) == before
            def control_under_pressure(phase):
                for action, locked in (("lock", True), ("unlock", False)):
                    run(administrator + mutation(action, "proxy-" + phase + "-" + action), env=environment)
                    assert json.loads(run(ctl + ["cluster", "info"], env=environment).stdout)["locked"] is locked
            for phase in ("release", "expire"):
                pressure(root, config, nginx, environment, trusted, worker_port, proxy_port,
                         worker_process, lambda: control_under_pressure(phase), expire=phase == "expire")
            pre_http_pressure(nginx, environment, trusted, anonymous, worker_port, proxy_port,
                              worker_process, lambda: control_under_pressure("pre-http"))
            for capacity in (16, 1):
                downstream_pressure(root, config, nginx, environment, trusted, worker_port, proxy_port,
                                    worker_process, lambda: control_under_pressure(f"downstream-{capacity}"),
                                    capacity=capacity)
            assert json.loads(run(ctl + ["cluster", "info"], env=environment).stdout) == before
        # The worker has now exited normally. A fresh proxy must not synthesize
        # healthy probes, cached metrics or an alternative upstream on failure.
        with process(nginx, environment) as proxy:
            poll(lambda: get(proxy_port, "/readyz", trusted)[0] == 502, [proxy])
            for path in ("/metrics", "/livez", "/readyz", "/startupz"):
                status, headers, body = get(proxy_port, path, trusted)
                assert status == 502 and headers["Cache-Control"] == "no-store"
                assert b"orishu_worker_" not in body
                assert get(proxy_port, path, anonymous)[0] in (400, 403)
            assert get(proxy_port, "/membership", trusted)[0] == 404
    print("PASS: monitoring mutual trust, isolation, scrape, request capacity/recovery, pre-HTTP/downstream expiry and proxy/upstream outage")


if __name__ == "__main__":
    main()
