"""Four colocated workers, one bounded probe; private lab diagnostic only.

The coordinator owns experiment choices. This shell owns its children and
independent deadlines; it never operates existing workers or changes host policy.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

from pi_lab_node import artifact, require, write_new_json, bounded_command
from pi_lab_session import Session, Drain, decode, emit, line, experiment_options
from pi_lab_measurement import Process, watchdog_child, resource_delta
from pi_lab_environment import KernelSeries, snapshot


def scheduler_snapshot(process):
    """Read-only per-thread brackets; unavailable/changing tasks stay explicit."""
    began = time.monotonic_ns()
    process.verify()
    rows = {}
    tasks = sorted(Path(f'/proc/{process.pid}/task').iterdir())
    require(len(tasks) <= 128, 'scheduler task cap')
    for task in tasks:
        try:
            with (task / 'schedstat').open() as source:
                fields = source.read(257).split()
            require(len(fields) == 3 and all(n.isdecimal() for n in fields), 'scheduler counter shape')
            with (task / 'status').open() as source:
                raw = source.read(65537)
            require(len(raw) <= 65536, 'thread status cap')
            switches = {line.split(':')[0]: int(line.split()[1]) for line in raw.splitlines()
                        if line.startswith(('voluntary_ctxt_switches:', 'nonvoluntary_ctxt_switches:'))}
            rows[task.name] = {'schedstat': [int(n) for n in fields], **switches}
        except OSError:
            rows[task.name] = {'unavailable': True}
    process.verify()
    return {'read_started_monotonic_ns': began, 'monotonic_ns': time.monotonic_ns(), 'threads': rows}


def placement_snapshot(processes, interface):
    """Private kernel socket evidence, plus all device byte counters, not packet attribution."""
    response = bounded_command(['ss', '-H', '-n', '-a', '-t', '-u', '-p'], timeout=3, cap=131072)
    require(response['exit'] == 0, 'socket inspection unavailable')
    rows = []
    for process in processes:
        process.verify()
        lines = [line for line in response['stdout'].splitlines() if f'pid={process.pid},' in line]
        require(len(lines) <= 256, 'socket evidence cap')
        rows.append({'pid': process.pid, 'lines': lines,
                     'selected_device_visible': any('%' + interface + ':' in line for line in lines)})
    counters = {}
    devices = sorted(Path('/sys/class/net').iterdir())
    require(len(devices) <= 64, 'device inventory cap')
    for device in devices:
        counters[device.name] = {key: int((device / 'statistics' / key).read_text())
                                for key in ('rx_bytes', 'tx_bytes')}
    return {'monotonic_ns': time.monotonic_ns(), 'sockets': rows, 'device_bytes': counters}


class ProbeDrain(Drain):
    """Retain only a bounded prefix in memory; export a finite classification."""
    def __init__(self, stream):
        self.prefix = bytearray()
        super().__init__(stream)

    def run(self):
        while raw := self.stream.read(4096):
            self.bytes += len(raw)
            self.digest.update(raw)
            self.prefix.extend(raw[:max(0, 4096 - len(self.prefix))])

    def finish(self):
        result = super().finish()
        text = bytes(self.prefix).lower()
        if not text:
            classification = 'none'
        elif b'failed to fill whole buffer' in text:
            classification = 'control_eof'
        elif b'load response changed formation' in text:
            classification = 'membership_or_identity_changed'
        elif b'timeout' in text or b'timed out' in text or b'deadline' in text:
            classification = 'timeout'
        elif b'connection' in text or b'transporterror' in text:
            classification = 'transport'
        else:
            classification = 'unclassified'
        result['classification'] = classification
        return result


def profile(args):
    require(isinstance(args, dict) and set(args) - {'global_workers'} == {'cell', 'first_role', 'rate_per_worker',
            'clients_per_worker', 'formation', 'nodes', 'probe_sha256'}, 'capacity profile fields')
    total = args.get('global_workers', 16)
    local = len(args['nodes']) if isinstance(args['nodes'], list) else 0
    require(type(total) is int and local in (1, 4) and total % local == 0
            and 3 <= total // local <= 5, 'capacity topology bounds')
    require('global_workers' in args or local == 4, 'legacy capacity topology')
    require(type(args['cell']) is int and 0 <= args['cell'] < 12
            and type(args['first_role']) is int and args['first_role'] in range(0, total, local)
            and type(args['clients_per_worker']) is int and args['clients_per_worker'] in (2, 8, 32, 64)
            and (args['rate_per_worker'] is None or type(args['rate_per_worker']) is int
                 and 100 <= args['rate_per_worker'] <= 100000), 'capacity profile bounds')
    require(isinstance(args['nodes'], list) and len(args['nodes']) == len(set(args['nodes'])) == local
            and all(isinstance(n, str) for n in args['nodes'])
            and isinstance(args['formation'], str)
            and isinstance(args['probe_sha256'], str)
            and re.fullmatch('[0-9a-f]{64}', args['probe_sha256']), 'capacity identities/digest')
    return args


class CapacitySession:
    def __init__(self, config):
        self.sessions, self.child, self.stderr, self.cells = [], None, None, set()
        self.progress = None
        self.deadline = time.monotonic() + 570
        self.experiment = experiment_options(config.get('experiment'))
        self.global_workers = self.experiment['hosts'] * self.experiment['workers_per_host']
        self.versioned = 'experiment' in config
        require(config.get('mode') == 'compiled_off', 'capacity telemetry mode')
        try:
            for slot in range(self.experiment['workers_per_host']):
                child_config = config | {'name': config['name'] + '-' + str(slot),
                                         'run_id': config['run_id'] + '-' + str(slot)}
                self.sessions.append(Session(child_config, peer_port=9000 + slot, lifetime=600))
            self.base = self.sessions[0].base
            self.run_id, self.interface = config['run_id'], config['interface']
            self.hashes = self.sessions[0].hashes
            require(all(s.hashes == self.hashes for s in self.sessions), 'local artifacts differ')
            self.probe_path, self.probe_digest = None, None
        except BaseException:
            self.stop()
            raise

    def stop_probe(self):
        if self.child is None:
            return {'started': False, 'clean': True}
        child, self.child = self.child, None
        if not child.stdin.closed:
            child.stdin.close()
        forced = False
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=7)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=2)
                forced = True
        child.stdout.close()
        logs = self.stderr.finish()
        return {'started': True, 'exit': child.returncode, 'forced': forced,
                'clean': not forced, 'successful_exit': child.returncode == 0, 'stderr': logs}

    def stop(self):
        try:
            probe = self.stop_probe()
        except Exception as error:
            probe = {'clean': False, 'error_type': type(error).__name__}
        def stop_one(session):
            try:
                return session.stop()
            except Exception as error:
                return {'clean': False, 'error_type': type(error).__name__}
        with ThreadPoolExecutor(max_workers=4) as pool:
            workers = list(pool.map(stop_one, self.sessions))
        return {'clean': probe['clean'] and all(w['clean'] for w in workers),
                'probe': probe, 'workers': workers}

    def prepare(self, args):
        self.progress = {'cell': args.get('cell'), 'stage': 'profile'}
        profile(args)
        require(args.get('global_workers', 16) == self.global_workers
                and len(args['nodes']) == len(self.sessions), 'session topology mismatch')
        require(self.child is None and args['cell'] not in self.cells and len(self.cells) < 12,
                'cell already attempted or probe still present')
        self.cells.add(args['cell'])
        require(time.monotonic() + 60 < self.deadline, 'capacity deadline')
        targets = []
        self.progress['stage'] = 'identities'
        for slot, session in enumerate(self.sessions):
            view = session.cli(['cluster', 'info'])
            require(view['formationId'] == args['formation'] and view['sourceNodeId'] == args['nodes'][slot]
                    and view['nodes'] == view['alive'] == self.global_workers and view['introducerReady']
                    and not view['locked'], 'capacity pre-load identity')
            targets.append({'socket': str(session.base / 'api.sock'), 'formation': args['formation'],
                            'node': args['nodes'][slot]})
        if self.probe_path is None:
            self.progress['stage'] = 'artifact-copy'
            probe_name = 'formation-telemetry-probe-capacity-v3' if self.versioned else 'formation-telemetry-probe-capacity-v2'
            source = self.sessions[0].root / 'bin' / probe_name
            metadata = artifact(source)
            require(metadata['sha256'] == args['probe_sha256'] and metadata['elf64_aarch64']
                    and metadata['executable'], 'capacity probe artifact')
            destination = self.base / 'bin' / 'capacity-probe'
            digest, total = hashlib.sha256(), 0
            with source.open('rb') as incoming, destination.open('xb') as outgoing:
                while raw := incoming.read(1048576):
                    total += len(raw)
                    require(total <= 256 * 1024 * 1024, 'probe copy cap')
                    digest.update(raw)
                    outgoing.write(raw)
            require(total == metadata['bytes'] and digest.hexdigest() == args['probe_sha256'], 'probe copy identity')
            destination.chmod(0o700)
            self.probe_path, self.probe_digest = destination, digest.hexdigest()
        require(args['probe_sha256'] == self.probe_digest, 'probe changed between cells')
        self.config = {'schema_version': 1, 'first_role': args['first_role'], 'targets': targets,
                       'clients_per_worker': args['clients_per_worker'], 'rate_per_worker': args['rate_per_worker']}
        if self.versioned:
            self.config.update(schema_version=2, global_workers=self.global_workers)
        self.cell = args['cell']
        self.progress['stage'] = 'config-write'
        config_path = self.base / f'capacity-{self.cell}.json'
        write_new_json(config_path, self.config)
        self.child = subprocess.Popen(['timeout', '--preserve-status', '--signal=TERM', '--kill-after=5s',
                                       '60s', str(self.probe_path), 'capacity', str(config_path)],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      bufsize=0, start_new_session=True)
        self.stderr = ProbeDrain(self.child.stderr)
        self.progress = {'cell': self.cell, 'stage': 'warmup'}
        require(line(self.child.stdout, min(self.deadline, time.monotonic() + 20)) == b'READY', 'capacity READY')
        self.progress['stage'] = 'process-identities'
        self.processes = [watchdog_child(s.worker, s.base / 'bin' / 'orishu-worker') for s in self.sessions]
        self.processes += [watchdog_child(self.child, self.probe_path), Process(os.getpid())]
        self.threads = [len(list(Path(f'/proc/{p.pid}/task').iterdir())) for p in self.processes]
        self.progress['stage'] = 'warmup'
        return {'prepared': True, 'cell': self.cell, 'config': self.config, 'probe_sha256': self.probe_digest,
                'threads': self.threads}

    def measure(self, args):
        external = getattr(self, 'external_generator', False)
        require(set(args) == {'start_unix_ns'} and type(args['start_unix_ns']) is int
                and (self.child is not None or external) and self.progress['stage'] == 'warmup', 'capacity measurement state')
        mono, wall = time.monotonic_ns(), time.time_ns()
        wait = args['start_unix_ns'] - wall
        require(2e9 <= wait <= 20e9 and time.monotonic() + wait / 1e9 + 20 < self.deadline, 'capacity future start')
        self.progress['stage'] = 'waiting'
        # Diagnostic counters cover their own wider brackets. Collecting every
        # thread at the scheduled start delays resource sampling under load.
        scheduler_before = [scheduler_snapshot(p) for p in self.processes]
        require(time.monotonic_ns() < mono + wait, 'scheduler collection overran start lead')
        while time.monotonic_ns() < mono + wait:
            time.sleep(min(.05, max(0, mono + wait - time.monotonic_ns()) / 1e9))
        before = [p.sample() for p in self.processes]
        peaks, swaps = [p['rss_kib'] for p in before], [p['swap_kib'] for p in before]
        actual_mono, actual_wall = time.monotonic_ns(), time.time_ns()
        env = KernelSeries(self.run_id, self.interface, actual_mono)
        if not external:
            self.child.stdin.write(b'G')
        self.progress['stage'] = 'sampling'
        end, next_sample, samples, missed = actual_mono + 10_000_000_000, actual_mono, 0, 0
        try:
            while time.monotonic_ns() < end:
                env.sample_if_due()
                if env.samples:
                    latest = env.samples[-1].get('snapshot', {})
                    temperature = latest.get('temperature_millicelsius')
                    require(temperature is not None and temperature < 75000, 'sampled thermal safety stop')
                for index, process in enumerate(self.processes):
                    current = process.sample()
                    require(current['swap_kib'] == 0, 'sampled process swap safety stop')
                    peaks[index] = max(peaks[index], current['rss_kib'])
                    swaps[index] = max(swaps[index], current['swap_kib'])
                samples += 1
                next_sample += 100_000_000
                now = time.monotonic_ns()
                if next_sample < now:
                    skipped = (now - next_sample) // 100_000_000 + 1
                    missed += skipped
                    next_sample += skipped * 100_000_000
                time.sleep(max(0, min(end, next_sample) - time.monotonic_ns()) / 1e9)
            if not external:
                require(line(self.child.stdout, min(self.deadline, time.monotonic() + 3)) == b'END', 'capacity END')
            after = [p.sample() for p in self.processes]
            scheduler_after = [scheduler_snapshot(p) for p in self.processes]
            end_mono, end_wall = time.monotonic_ns(), time.time_ns()
            report = None
            if not external:
                self.child.stdin.write(b'A')
                report = decode(line(self.child.stdout, min(self.deadline, time.monotonic() + 8)))
                self.child.stdin.close()
                require(self.child.wait(timeout=2) == 0, 'capacity probe exit')
            result = {'schema_version': 2 if external else 1,
                      'kind': 'pi-network-capacity-measurement' if external else 'pi-capacity-measurement', 'run_id': self.run_id,
                      'cell': self.cell, 'config': self.config, 'load': report,
                      'resources': [resource_delta(a, b, p, s) for a, b, p, s in zip(before, after, peaks, swaps)],
                      'threads': self.threads, 'samples': samples, 'missed_samples': missed,
                      'scheduler_before': scheduler_before, 'scheduler_after': scheduler_after,
                      'scheduler_scope': 'pre-start-lead-through-post-window',
                      'environment_series': env.finish(), 'start_unix_ns': actual_wall, 'end_unix_ns': end_wall,
                      'control_bracket_ns': end_mono - actual_mono,
                      'local_start_lateness_ns': actual_mono - (mono + wait),
                      'window_wall_change_ns': end_wall - actual_wall - (end_mono - actual_mono),
                      'acceptance_run': False}
            result['probe_cleanup'] = self.stop_probe()
            if external and self.experiment['placement']:
                result['placement_after'] = placement_snapshot(self.processes[:-1], self.interface)
            write_new_json(self.base / f'capacity-result-{self.cell}.json', result)
            return result
        finally:
            self.progress.update(samples=samples, missed_samples=missed, environment_series=env.finish())
            write_new_json(self.base / f'capacity-progress-{self.cell}.json', self.progress)

    def request(self, request):
        require(isinstance(request, dict) and set(request) == {'operation', 'arguments'}, 'capacity request')
        op, args = request['operation'], request['arguments']
        require(isinstance(args, dict), 'capacity arguments')
        if op == 'worker':
            require(set(args) == {'slot', 'operation', 'arguments'} and type(args['slot']) is int
                    and 0 <= args['slot'] < len(self.sessions) and args['operation'] in
                    ('start', 'info', 'members', 'material', 'join', 'join-status'), 'capacity worker operation')
            return self.sessions[args['slot']].request({k: args[k] for k in ('operation', 'arguments')})
        if op == 'prepare-capacity':
            return self.prepare(args)
        if op == 'measure-capacity':
            return self.measure(args)
        if op == 'clock':
            from pi_lab_clock import receipt
            require(set(args) == {'nonce'}, 'clock fields')
            return receipt(self.run_id, args['nonce'])
        require(op in ('stop', 'environment') and not args, 'capacity operation not allowed')
        return self.stop() if op == 'stop' else snapshot(self.run_id, self.interface)


def main(session_factory=CapacitySession):
    os.umask(0o077)
    session = None
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt()
    for value in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(value, interrupted)
    try:
        session = session_factory(decode(line(sys.stdin.buffer, time.monotonic() + 15)))
        emit({'ok': True, 'ready': True, 'run_id': session.run_id, 'artifacts': session.hashes,
              'readiness': session.sessions[0].readiness})
        for _ in range(2048):
            request = decode(line(sys.stdin.buffer, session.deadline))
            result = session.request(request)
            emit({'ok': True, 'result': result})
            if request['operation'] == 'stop':
                return
    except (Exception, KeyboardInterrupt) as error:
        failure = {'ok': False, 'error_type': type(error).__name__,
                   'stage': session.progress.get('stage') if session and session.progress else 'session'}
        if session is not None:
            # Capture public local membership after a failed warmup, before
            # stopping workers. No token, peer credential or arbitrary stderr.
            views = []
            for worker in session.sessions:
                try:
                    views.append(worker.cli(['cluster', 'info']))
                except Exception as view_error:
                    views.append({'error_type': type(view_error).__name__})
            cleanup = session.stop()
            write_new_json(session.base / 'capacity-failure.json', failure | {'views': views, 'cleanup': cleanup})
        emit(failure)
    finally:
        if session is not None:
            session.stop()


if __name__ == '__main__':
    main()
