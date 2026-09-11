"""Node-owned, bounded formation smoke session. Loaded over verified SSH.

Only private, newly created run directories are written. No service management,
arbitrary shell, existing worker control, or performance acceptance operation.
"""
import hashlib
import http.client
import ipaddress
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import signal
import subprocess
import sys
import threading
import time

from pi_lab_node import artifact, bounded_command, check, require, root_path, write_new_json

CAP = 65536
LIFETIME = 180


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def decode(raw):
    require(len(raw) <= CAP, 'JSON byte cap')
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: require(False, 'non-finite JSON'))


def identifier(value):
    require(isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', value),
            'invalid identifier')
    return value


def line(stream, deadline):
    """No buffered read-ahead; a partial line cannot extend the deadline."""
    result = bytearray()
    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_READ)
        while len(result) <= CAP:
            remaining = deadline - time.monotonic()
            require(remaining > 0 and selector.select(remaining), 'session IO deadline')
            value = os.read(stream.fileno(), 1)
            if not value:
                raise EOFError('session disconnected')
            if value == b'\n':
                return bytes(result)
            result.extend(value)
    raise ValueError('session line cap')


def emit(value):
    raw = json.dumps(value, allow_nan=False).encode() + b'\n'
    require(len(raw) <= CAP, 'response byte cap')
    sys.stdout.buffer.write(raw)
    sys.stdout.buffer.flush()


class Drain:
    """Continuously consume logs with constant memory; export only size/hash."""
    def __init__(self, stream):
        self.stream, self.bytes, self.digest = stream, 0, hashlib.sha256()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        while raw := self.stream.read(4096):
            self.bytes += len(raw)
            self.digest.update(raw)

    def finish(self):
        self.thread.join(timeout=2)
        require(not self.thread.is_alive(), 'log drain deadline')
        self.stream.close()
        return {'bytes': self.bytes, 'sha256': self.digest.hexdigest()}


class Session:
    def __init__(self, config, *, peer_port=9000, lifetime=LIFETIME):
        require(type(peer_port) is int and 9000 <= peer_port <= 9003
                and type(lifetime) is int and 180 <= lifetime <= 600, 'internal session bounds')
        self.peer_port = peer_port
        require(isinstance(config, dict) and set(config) ==
                {'root', 'interface', 'peer_address', 'name', 'run_id', 'mode'}, 'session config fields')
        self.root = root_path(config['root'])
        self.name, self.run_id = identifier(config['name']), identifier(config['run_id'])
        self.mode = config['mode']
        self.interface = config['interface']
        require(self.mode in ('omitted', 'compiled_off', 'metrics'), 'smoke mode not supported')
        self.address = str(ipaddress.ip_address(config['peer_address']))
        self.readiness = check(self.root, config['interface'])
        require(self.readiness['infrastructure_ready'] and self.readiness['artifacts_staged'], 'node not prepared')
        require(self.address in self.readiness['addresses'], 'peer address not assigned')
        require(shutil.which('timeout') is not None, 'timeout watchdog required')
        self.base = self.root / 'runs' / self.run_id
        # Unix sockets have a small platform path limit; fail before any writes.
        require(len(os.fsencode(self.base / 'api.sock')) <= 100, 'Unix socket path too long')
        self.base.mkdir(mode=0o700, exist_ok=False)
        (self.base / 'bin').mkdir(mode=0o700)
        self.worker, self.drain = None, None
        self.measurement = None
        self.deadline = time.monotonic() + lifetime
        self.operations = set()
        self.hashes = {}
        for name in ('orishu-worker' if self.mode != 'omitted' else 'orishu-worker-omitted', 'orishuctl'):
            source, target = self.root / 'bin' / name, self.base / 'bin' / name
            expected = self.readiness['artifacts'][name]['sha256']
            with source.open('rb') as incoming, target.open('xb') as outgoing:
                # Artifact sizes were bounded by readiness. Recheck copied bytes/hash.
                remaining = 256 * 1024 * 1024 + 1
                while raw := incoming.read(min(1048576, remaining)):
                    remaining -= len(raw)
                    require(remaining > 0, 'snapshot artifact byte cap')
                    outgoing.write(raw)
            target.chmod(0o700)
            require(artifact(target)['sha256'] == expected == artifact(source)['sha256'], 'artifact changed during snapshot')
            self.hashes[name] = expected
        write_new_json(self.base / 'manifest.json', {
            'schema_version': 1, 'kind': 'pi-formation-smoke-node', 'run_id': self.run_id,
            'mode': self.mode, 'artifacts': self.hashes, 'lifetime_seconds': lifetime,
            'peer_port': peer_port})

    def start(self):
        require(self.worker is None, 'worker already started')
        binary = 'orishu-worker-omitted' if self.mode == 'omitted' else 'orishu-worker'
        port = getattr(self, 'peer_port', 9000)
        address = f'[{self.address}]:{port}' if ':' in self.address else f'{self.address}:{port}'
        environment = {k: v for k, v in os.environ.items()
                       if not k.startswith(('ORISHU_', 'OTEL_')) and k != 'TOKIO_WORKER_THREADS'}
        command = [str(self.base / 'bin' / binary), '--state-dir', str(self.base / 'state'),
                   '--listen.clients', str(self.base / 'api.sock'), '--listen.peers', address,
                   '--accepts.peers', 'true', '--name', self.name,
                   '--observability.enabled', str(self.mode == 'metrics').lower(),
                   '--observability.bind', '127.0.0.1:9168',
                   '--tracing.enabled', 'false', '--logging.enabled', 'false']
        # Set only by the reviewed private network-capacity adapter. Existing
        # smoke/colocated profiles retain exactly their local-only listener.
        if getattr(self, 'client_tls', None) is not None:
            endpoint, certificate, key = self.client_tls
            command += ['--listen.clients', endpoint, '--tls-cert', str(certificate), '--tls-key', str(key)]
        # Independent watchdog survives an abrupt SSH/supervisor death. It owns
        # its worker child; no persisted PID is later used to control a process.
        lifetime = max(1, int(self.deadline - time.monotonic()))
        self.worker = subprocess.Popen(['timeout', '--preserve-status', '--signal=TERM', '--kill-after=5s',
                                        f'{lifetime}s', *command], stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                       start_new_session=True, env=environment, bufsize=0)
        self.drain = Drain(self.worker.stdout)
        deadline = min(self.deadline, time.monotonic() + 15)
        while not (self.base / 'api.sock').exists():
            require(self.worker.poll() is None, 'worker exited during startup')
            require(time.monotonic() < deadline, 'worker socket deadline')
            time.sleep(0.05)
        return {'started': True, 'artifacts': self.hashes}

    def cli(self, args):
        require(self.worker is not None and self.worker.poll() is None, 'worker is not running')
        command = [str(self.base / 'bin' / 'orishuctl'), '--host', str(self.base / 'api.sock'),
                   '--output', 'json', '--timeout', '2s', '--operator-token-file',
                   str(self.base / 'state' / 'operator.token'), *args]
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, 'session expired')
        response = bounded_command(command, timeout=min(5, remaining), cap=CAP)
        require(response['exit'] == 0, 'operator command failed: ' + args[0])
        return decode(response['stdout'])

    def request(self, request):
        require(isinstance(request, dict) and set(request) == {'operation', 'arguments'}, 'request fields')
        op, args = request['operation'], request['arguments']
        require(isinstance(args, dict), 'operation arguments')
        if op == 'environment':
            require(not args, 'environment fields')
            from pi_lab_environment import snapshot
            return snapshot(self.run_id, self.interface)
        if op == 'clock':
            require(set(args) == {'nonce'}, 'clock fields')
            from pi_lab_clock import receipt
            return receipt(self.run_id, args['nonce'])
        if op == 'prepare-load':
            require(set(args) == {'formation', 'node', 'workers', 'role', 'probe_sha256'}, 'prepare-load fields')
            require(self.mode in ('omitted', 'compiled_off'), 'timed metrics scraping is not implemented yet')
            require(self.measurement is None, 'load already prepared')
            require(isinstance(args['probe_sha256'], str) and re.fullmatch(r'[0-9a-f]{64}', args['probe_sha256']), 'probe digest')
            view = self.cli(['cluster', 'info'])
            require(view['formationId'] == args['formation'] and view['sourceNodeId'] == args['node']
                    and view['nodes'] == view['alive'] == args['workers'] and view['introducerReady']
                    and not view['locked'], 'pre-load formation mismatch')
            from pi_lab_measurement import Measurement
            binary = 'orishu-worker-omitted' if self.mode == 'omitted' else 'orishu-worker'
            config = {'schema_version': 2, 'workers': args['workers'], 'role': args['role'],
                      'target': {'socket': str(self.base / 'api.sock'), 'formation': args['formation'], 'node': args['node']}}
            self.measurement = Measurement(self.base, self.worker, self.base / 'bin' / binary,
                                           self.root / 'bin' / 'formation-telemetry-probe-node-v2', args['probe_sha256'],
                                           config, self.deadline, line, Drain, interface=self.interface)
            return {'prepared': True, 'probe_sha256': args['probe_sha256'], 'workers': args['workers'], 'role': args['role']}
        if op == 'measure':
            require(set(args) == {'start_unix_ns'} and self.measurement is not None, 'measurement is not prepared')
            return self.measurement.run(args['start_unix_ns'])
        if op == 'stop-load':
            require(not args and self.measurement is not None, 'load is not prepared')
            result = self.measurement.close()
            self.measurement = None
            return result
        if op in ('start', 'info', 'members', 'material', 'stop', 'diagnostics'):
            require(not args, 'unexpected arguments')
            if op == 'start':
                return self.start()
            if op == 'stop':
                return self.stop()
            if op == 'diagnostics':
                require(self.mode == 'metrics', 'diagnostics not enabled in this mode')
                result = {}
                for path in ('/livez', '/readyz', '/startupz', '/metrics'):
                    connection = http.client.HTTPConnection('127.0.0.1', 9168, timeout=2)
                    try:
                        connection.request('GET', path)
                        response = connection.getresponse()
                        body = response.read(32769)
                        require(len(body) <= 32768, 'diagnostics response cap')
                        result[path] = {'status': response.status, 'bytes': len(body),
                                        'sha256': hashlib.sha256(body).hexdigest()}
                        if path == '/metrics':
                            # Keep only the number of series. This is endpoint
                            # qualification, not a counter/overhead acceptance run.
                            result[path]['series'] = sum(bool(line) and not line.startswith(b'#')
                                                         for line in body.splitlines())
                    finally:
                        connection.close()
                return result
            return self.cli({'info': ['cluster', 'info'], 'members': ['ls'], 'material': ['token']}[op])
        if op == 'join-status':
            require(set(args) == {'operation_id'}, 'join-status fields')
            return self.cli(['join-status', identifier(args['operation_id'])])
        require(op in ('join', 'lock', 'unlock', 'leave'), 'operation not allowed')
        require(set(args) == ({'formation', 'operation_id', 'material'} if op == 'join'
                             else {'formation', 'operation_id'}), 'mutation fields')
        formation, operation = identifier(args['formation']), identifier(args['operation_id'])
        require(operation not in self.operations, 'mutation already submitted; inspect its outcome')
        self.operations.add(operation)
        suffix = ['--formation-id', formation, '--operation-id', operation]
        if op == 'join':
            require(isinstance(args['material'], dict), 'join material shape')
            path = self.base / (operation + '.join.json')
            write_new_json(path, args['material'])
            try:
                return self.cli(['join', '--join-material-file', str(path), *suffix])
            finally:
                path.unlink()  # Only the just-created transient credential file.
        return self.cli((['cluster', op] if op in ('lock', 'unlock') else [op]) + suffix)

    def stop(self):
        measurement = None
        if getattr(self, 'measurement', None) is not None:
            try:
                measurement = self.measurement.close()
            except Exception as error:
                # A probe cleanup failure must not skip worker cleanup.
                measurement = {'forced': True, 'error_type': type(error).__name__}
            self.measurement = None
        if self.worker is None:
            return {'clean': True, 'started': False, 'measurement': measurement}
        child, self.worker = self.worker, None
        forced = False
        was_running = child.poll() is None
        if was_running:
            # timeout forwards TERM to its owned process group and applies
            # kill-after if shutdown stalls. Do not signal a reaped PID.
            child.terminate()
            try:
                child.wait(timeout=7)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=2)
                forced = True
        logs = self.drain.finish()
        result = {'clean': was_running and not forced and child.returncode in (0, -signal.SIGTERM, 143),
                  'exit': child.returncode, 'forced': forced, 'logs': logs,
                  'socket_removed': not (self.base / 'api.sock').exists()}
        result['clean'] &= result['socket_removed']
        if measurement is not None:
            result['measurement'] = measurement
            result['clean'] &= not measurement.get('forced', False)
        write_new_json(self.base / 'cleanup.json', result)
        return result


def clock_only(config):
    """Read-only clock session: no root, artifacts, worker, or subprocess API."""
    from pi_lab_clock import receipt
    require(set(config) == {'run_id', 'mode'} and config['mode'] == 'clock_only', 'clock-only config')
    run_id = identifier(config['run_id'])
    deadline = time.monotonic() + 20
    emit({'ok': True, 'ready': True, 'run_id': run_id, 'clock_only': True})
    for _ in range(16):
        try:
            request = decode(line(sys.stdin.buffer, deadline))
        except EOFError:
            return
        require(isinstance(request, dict) and set(request) == {'operation', 'arguments'}
                and request['operation'] == 'clock' and isinstance(request['arguments'], dict)
                and set(request['arguments']) == {'nonce'}, 'clock-only operation')
        emit({'ok': True, 'result': receipt(run_id, request['arguments']['nonce'])})
    raise ValueError('clock-only command cap')


def main():
    os.umask(0o077)
    session = None
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt()
    for value in (signal.SIGHUP, signal.SIGTERM, signal.SIGINT):
        signal.signal(value, interrupted)
    try:
        config = decode(line(sys.stdin.buffer, time.monotonic() + 15))
        if isinstance(config, dict) and config.get('mode') == 'clock_only':
            clock_only(config)
            return
        session = Session(config)
        emit({'ok': True, 'ready': True, 'run_id': session.run_id, 'artifacts': session.hashes,
              'readiness': session.readiness})
        for _ in range(1024):
            request = decode(line(sys.stdin.buffer, session.deadline))
            result = session.request(request)
            emit({'ok': True, 'result': result})
            if request['operation'] == 'stop':
                return
        raise ValueError('session command count cap')
    except (Exception, KeyboardInterrupt) as error:
        # Never serialize arbitrary request material, CLI stderr, paths or secrets.
        emit({'ok': False, 'error_type': type(error).__name__})
    finally:
        if session is not None:
            session.stop()


if __name__ == '__main__':
    main()
