"""Off-Pi HTTPS capacity adapter. Separate generator and worker receipts.

Only new private lab children/files are owned. No forwarding, insecure TLS,
production worker changes, credential argv, or automatic failed-cell retries.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import ipaddress
import os
from pathlib import Path
import re
import shutil
import socket
import ssl
import subprocess
import time

from pi_lab_experiment import Remote
from pi_lab_capacity_node import ProbeDrain, scheduler_snapshot
from pi_lab_measurement import watchdog_child, resource_delta
from pi_lab_node import require, write_new_json
from pi_lab_session import decode, line


def bundle():
    root = Path(__file__).parent
    code = 'import sys,types\n'
    for name in ('pi_lab_node', 'pi_lab_clock', 'pi_lab_environment', 'pi_lab_measurement',
                 'pi_lab_session', 'pi_lab_capacity_node', 'pi_lab_network_node'):
        source = (root / (name + '.py')).read_text()
        code += (f'module=types.ModuleType({name!r});sys.modules[{name!r}]=module\n'
                 f'exec(compile({source!r},{name!r},"exec"),module.__dict__)\n')
    code += 'sys.modules["pi_lab_capacity_node"].main(sys.modules["pi_lab_network_node"].NetworkSession)\n'
    raw = code.encode()
    require(len(raw) <= 131072, 'network bundle cap')
    return raw


def endpoint(address, slot):
    require(type(slot) is int and 0 <= slot < 4, 'network endpoint slot')
    address = str(ipaddress.ip_address(address))
    return f'[{address}]:{9440 + slot}' if ':' in address else f'{address}:{9440 + slot}'


def generator_environment():
    """Bounded read-only host counters; not per-worker attribution."""
    with open('/proc/stat') as source:
        raw = source.read(65537)
    require(len(raw) <= 65536, 'host CPU counters cap')
    counters = {line.split()[0]: [int(v) for v in line.split()[1:]] for line in raw.splitlines()
                if line.startswith(('cpu ', 'ctxt '))}
    temperatures = {}
    for hwmon in sorted(Path('/sys/class/hwmon').glob('hwmon*'))[:64]:
        try:
            if (hwmon / 'name').read_text().strip() != 'coretemp':
                continue
            for sensor in sorted(hwmon.glob('temp*_input'))[:64]:
                temperatures[str(sensor)] = int(sensor.read_text())
        except OSError:
            continue
    return {'monotonic_ns': time.monotonic_ns(), 'cpu_counters': counters,
            'temperatures_millicelsius': temperatures, 'logical_cpus': os.cpu_count()}


def qualify(address, port, certificate):
    """Actual TLS/auth failures, not disabled verification or inferred trust."""
    context = ssl.create_default_context(cafile=str(certificate))
    codes = []
    for headers in ({}, {'Authorization': 'Bearer ' + '0' * 64}):
        connection = http.client.HTTPSConnection(address, port, context=context, timeout=2)
        try:
            connection.request('GET', '/api/v1/cluster', headers=headers)
            response = connection.getresponse()
            codes.append(response.status)
            require(len(response.read(8193)) <= 8192, 'qualification response cap')
        finally:
            connection.close()
    require(codes == [401, 401], 'network authorization qualification')
    def refused(ctx, name):
        try:
            with socket.create_connection((address, port), timeout=2) as raw:
                with ctx.wrap_socket(raw, server_hostname=name):
                    pass
        except ssl.SSLCertVerificationError:
            return True
        return False
    require(refused(ssl.create_default_context(), address), 'untrusted certificate accepted')
    require(refused(context, 'invalid.orishu.test'), 'wrong server identity accepted')
    return {'missing_token_status': codes[0], 'wrong_token_status': codes[1],
            'untrusted_certificate_rejected': True, 'wrong_server_identity_rejected': True}


class NetworkBackend:
    def __init__(self, probe, digest, *, direct_ip=False):
        self.probe = Path(probe).resolve(strict=True)
        require(self.probe.is_file() and os.access(self.probe, os.X_OK)
                and self.probe.stat().st_size < 256 * 1024 * 1024, 'local probe bounds')
        require(hashlib.sha256(self.probe.read_bytes()).hexdigest() == digest, 'local probe checksum')
        self.digest, self.output = digest, None
        require(type(direct_ip) is bool, 'direct SSH selection')
        self.direct_ip = direct_ip

    def remote(self, node, run_id, deadline):
        return NetworkRemote(node, run_id, deadline, self)


class NetworkRemote(Remote):
    def __init__(self, node, run_id, deadline, backend):
        self.backend, self.probe, self.logs = backend, None, None
        self.secrets, self.credentials, self.security = [], None, []
        super().__init__(node, run_id, 'compiled_off', deadline, bundle_factory=bundle,
                         direct_ip=backend.direct_ip)
        try:
            self.base = backend.output / ('generator-' + node['name'])
            self.base.mkdir(mode=0o700)
            self.binary = self.base / 'probe'
            with backend.probe.open('rb') as src, self.binary.open('xb') as dst:
                shutil.copyfileobj(src, dst)
            self.binary.chmod(0o700)
            require(hashlib.sha256(self.binary.read_bytes()).hexdigest() == backend.digest, 'snapshot probe checksum')
        except BaseException:
            super().stop()
            raise

    def worker(self, slot, operation, **arguments):
        return self.call('worker', payload={'slot': slot, 'operation': operation, 'arguments': arguments})

    def rpc(self, operation, arguments):
        self.write({'operation': operation, 'arguments': arguments})
        reply = self.read(45 if operation == 'measure-capacity' else 30)
        if isinstance(reply, dict) and reply.get('ok') is False:
            self.last_node_error = {key: reply.get(key) for key in ('error_type', 'stage')}
        require(isinstance(reply, dict) and reply.get('ok') is True and 'result' in reply, 'network RPC failed')
        return reply['result']

    def load_credentials(self):
        if self.credentials is not None:
            return
        raw = self.rpc('network-credentials', {})
        local = self.node.get('experiment', {}).get('workers_per_host', 4)
        require(isinstance(raw, list) and len(raw) == local, 'network credential count')
        self.credentials = []
        for slot, item in enumerate(raw):
            require(isinstance(item, dict) and set(item) == {'endpoint', 'certificate', 'token', 'certificate_sha256'}
                    and item['endpoint'] == endpoint(self.node['peer_address'], slot)
                    and isinstance(item['token'], str) and re.fullmatch('[0-9a-f]{64}', item['token'])
                    and isinstance(item['certificate'], str) and len(item['certificate']) < 16384
                    and hashlib.sha256(item['certificate'].encode()).hexdigest() == item['certificate_sha256'],
                    'network credential identity')
            cert, token = self.base / f'{slot}.crt', self.base / f'{slot}.token'
            for path, content in ((cert, item['certificate']), (token, item['token'])):
                with open(path, 'x', opener=lambda p, flags: os.open(p, flags, 0o600)) as out:
                    out.write(content)
                self.secrets.append(path)
            self.credentials.append({'certificate': str(cert), 'token_file': str(token)})
            self.security.append({'endpoint': item['endpoint'], 'certificate_sha256': item['certificate_sha256'],
                                  **qualify(self.node['peer_address'], 9440 + slot, cert)})
        # Never return the raw credential envelope to the generic coordinator.

    def _call(self, operation, **arguments):
        if operation == 'worker':
            require(set(arguments) == {'payload'}, 'worker RPC envelope')
            return self.rpc(operation, arguments['payload'])
        if operation == 'prepare-capacity':
            self.load_credentials()
            require(self.probe is None, 'previous generator still present')
            result = self.rpc(operation, arguments)
            public = result['config']
            expected = [{'endpoint': endpoint(self.node['peer_address'], slot), 'formation': arguments['formation'],
                         'node': arguments['nodes'][slot]} for slot in range(len(arguments['nodes']))]
            version = 4 if 'global_workers' in arguments else 2
            require(public['targets'] == expected and public['schema_version'] == version
                    and public.get('global_workers', 16) == arguments.get('global_workers', 16),
                    'network targets changed')
            private = public | {'targets': [t | c for t, c in zip(expected, self.credentials)]}
            config = self.base / f'cell-{arguments["cell"]}.json'
            write_new_json(config, private)
            self.probe = subprocess.Popen(['timeout', '--preserve-status', '--signal=TERM', '--kill-after=5s',
                                          '60s', str(self.binary), 'capacity-network', str(config)],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0, start_new_session=True)
            self.logs = ProbeDrain(self.probe.stderr)
            require(line(self.probe.stdout, min(self.deadline, time.monotonic() + 20)) == b'READY', 'network probe READY')
            self.process = watchdog_child(self.probe, self.binary)
            result['security'] = self.security
            return result
        if operation == 'measure-capacity':
            # Remote and generator sampling use independently owned clocks. The
            # coordinator verifies both actual starts; it never relabels one.
            with ThreadPoolExecutor(max_workers=1) as pool:
                remote = pool.submit(self.rpc, operation, arguments)
                local = self.measure_local()
                result = remote.result()
            require(result['load'] is None, 'unexpected Pi load generator')
            result['load'] = local['load']
            result['generator'] = local
            return result
        return self.rpc(operation, arguments)

    def measure_local(self):
        mono, wall = time.monotonic_ns(), time.time_ns()
        target = mono + self.local_start_ns - wall
        require(2e9 <= target - mono <= 20e9, 'local future start')
        scheduler_before = scheduler_snapshot(self.process)
        require(time.monotonic_ns() < target, 'scheduler collection overran start lead')
        while time.monotonic_ns() < target:
            time.sleep(min(.02, max(0, target - time.monotonic_ns()) / 1e9))
        before = self.process.sample()
        environment = [generator_environment()]
        peak, swap, samples = before['rss_kib'], before['swap_kib'], 0
        self.probe.stdin.write(b'G')
        end = time.monotonic() + 10
        while time.monotonic() < end:
            current = self.process.sample()
            peak, swap = max(peak, current['rss_kib']), max(swap, current['swap_kib'])
            require(swap == 0, 'generator swap safety stop')
            samples += 1
            if samples % 10 == 0:
                environment.append(generator_environment())
            time.sleep(.1)
        require(line(self.probe.stdout, min(self.deadline, time.monotonic() + 3)) == b'END', 'network probe END')
        after = self.process.sample()
        scheduler_after = scheduler_snapshot(self.process)
        environment.append(generator_environment())
        self.probe.stdin.write(b'A')
        report = decode(line(self.probe.stdout, min(self.deadline, time.monotonic() + 8)))
        require(self.probe.wait(timeout=2) == 0, 'network probe exit')
        cleanup = self.stop_probe()
        return {'load': report, 'resources': resource_delta(before, after, peak, swap),
                'samples': samples, 'cleanup': cleanup, 'location': 'coordinator', 'executor_threads': 2,
                'scheduler_before': scheduler_before, 'scheduler_after': scheduler_after,
                'scheduler_scope': 'pre-start-lead-through-post-window',
                'environment': environment}

    def stop_probe(self):
        if self.probe is None:
            return {'clean': True, 'started': False}
        child, self.probe = self.probe, None
        child.stdin.close()
        forced = False
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                import signal
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=2)
                forced = True
        child.stdout.close()
        return {'clean': not forced, 'started': True, 'exit': child.returncode,
                'forced': forced, 'stderr': self.logs.finish()}

    def stop(self):
        try:
            local = self.stop_probe()
        except Exception as error:
            local = {'clean': False, 'error_type': type(error).__name__}
        try:
            remote = super().stop()
            return remote | {'generator_cleanup': local, 'clean': remote['clean'] and local['clean']}
        finally:
            # Only exact files created by this adapter; these disposable tokens
            # authorize now-stopped test workers, never existing deployments.
            for path in self.secrets:
                path.unlink(missing_ok=True)
