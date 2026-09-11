"""Bounded node-local pilot measurement; no cluster or SSH authority.

The caller owns formation checks, the source freeze, clock qualification and
the reviewed experiment budget. This module records observations, not passes.
"""
import ctypes
import functools
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import time

from pi_lab_node import artifact, require, write_new_json
from pi_lab_environment import KernelSeries

WINDOW_NS = 10_000_000_000
PERIOD_NS = 50_000_000


def stat(pid):
    """Read Linux process identity/accounting with a bounded parser."""
    with open(f'/proc/{pid}/stat', 'rb') as source:
        raw = source.read(4097)
    require(len(raw) <= 4096 and b')' in raw, 'process stat bound/shape')
    fields = raw.rsplit(b')', 1)[1].split()
    require(len(fields) >= 22, 'process stat fields')
    return {'parent': int(fields[1]), 'start_ticks': int(fields[19]),
            'ticks': int(fields[11]) + int(fields[12])}


@functools.lru_cache(maxsize=1)
def clock_function():
    function = ctypes.CDLL(None).clock_getcpuclockid
    function.argtypes = (ctypes.c_int, ctypes.POINTER(ctypes.c_int))
    function.restype = ctypes.c_int
    return function


class Process:
    """Pinned live process, never a persisted PID used to signal a later run."""
    def __init__(self, pid, executable=None, parent=None):
        require(type(pid) is int and pid > 0, 'positive process ID required')
        self.pid, self.initial = pid, stat(pid)
        self.executable = Path(executable) if executable is not None else None
        require(parent is None or self.initial['parent'] == parent, 'unexpected process owner')
        self.verify()

    def verify(self):
        current = stat(self.pid)
        require(current['start_ticks'] == self.initial['start_ticks']
                and current['parent'] == self.initial['parent'], 'process identity changed')
        if self.executable is not None:
            require(Path(f'/proc/{self.pid}/exe').resolve(strict=True) == self.executable,
                    'process executable changed')
        return current

    def sample(self):
        self.verify()
        clock = ctypes.c_int()
        error = clock_function()(self.pid, ctypes.byref(clock))
        if error:
            raise OSError(error, 'process CPU clock unavailable')
        cpu = time.clock_gettime_ns(clock.value)
        observed = time.monotonic_ns()
        with open(f'/proc/{self.pid}/status', 'rb') as source:
            raw = source.read(65537)
        require(len(raw) <= 65536, 'process status cap')
        values = {line.split(b':', 1)[0]: int(line.split()[1]) for line in raw.splitlines()
                  if line.startswith((b'VmRSS:', b'VmHWM:', b'VmSwap:'))}
        self.verify()  # Reject exit/reuse across the CPU and memory reads too.
        return {'cpu_ns': cpu, 'observed_monotonic_ns': observed, 'rss_kib': values[b'VmRSS'],
                'hwm_kib': values[b'VmHWM'], 'swap_kib': values[b'VmSwap']}


def watchdog_child(watchdog, executable):
    require(watchdog.poll() is None, 'watchdog already exited')
    with open(f'/proc/{watchdog.pid}/task/{watchdog.pid}/children', 'rb') as source:
        raw = source.read(129)
    require(len(raw) <= 128 and len(raw.split()) == 1, 'watchdog must own exactly one child')
    return Process(int(raw), executable, watchdog.pid)


def resource_delta(before, after, peak, swap):
    require(after['cpu_ns'] >= before['cpu_ns'], 'process CPU regressed')
    return {'cpu_nanoseconds': after['cpu_ns'] - before['cpu_ns'],
            'cpu_seconds': (after['cpu_ns'] - before['cpu_ns']) / 1e9,
            'cpu_bracket_ns': after['observed_monotonic_ns'] - before['observed_monotonic_ns'],
            'rss_peak_kib': max(peak, after['rss_kib']),
            'swap_peak_kib': max(swap, after['swap_kib']),
            'setup_hwm_kib': before['hwm_kib'], 'lifetime_hwm_kib': after['hwm_kib']}


def probe_clock_bounds(timing):
    """Retain probe-owned clock intervals; no wall-step or overlap guarantee."""
    require(isinstance(timing, dict) and set(timing) == {'window_ns', 'start', 'end_marker', 'end_marker_elapsed_ns'},
            'probe timing fields')
    require(type(timing['window_ns']) is int and timing['window_ns'] == WINDOW_NS
            and type(timing['end_marker_elapsed_ns']) is int
            and WINDOW_NS <= timing['end_marker_elapsed_ns'] < 2**63, 'probe window duration')
    bounds = []
    for key in ('start', 'end_marker'):
        mark = timing[key]
        require(isinstance(mark, dict) and set(mark) == {'unix_ns', 'read_bracket_ns'}
                and all(type(n) is int and 0 <= n < 2**63 for n in mark.values())
                and mark['unix_ns'] + mark['read_bracket_ns'] < 2**63, 'probe clock mark')
        bounds.append([mark['unix_ns'], mark['unix_ns'] + mark['read_bracket_ns']])
    start, end = bounds
    elapsed = timing['end_marker_elapsed_ns']
    return {'start_wall_interval_ns': start, 'end_marker_wall_interval_ns': end,
            'wall_change_interval_ns': [end[0] - start[1] - elapsed, end[1] - start[0] - elapsed],
            'end_marker_elapsed_ns': elapsed, 'clock_uncertainty_qualified': False}


def load_issues(report, workers, role):
    """Validate the exact physical profile; return unmet rate/cap gates."""
    require(isinstance(report, dict) and set(report) == {'schema_version', 'arrival_profile', 'global_workers', 'seconds', 'workers', 'timing'}
            and type(report['schema_version']) is int and report.get('schema_version') == 5
            and report.get('arrival_profile') == 'fixed_500_per_worker_physical_pilot_v2'
            and type(report['global_workers']) is int and report.get('global_workers') == workers
            and type(report['seconds']) is int and report.get('seconds') == 10,
            'physical load report profile mismatch')
    probe_clock_bounds(report['timing'])
    rows = report.get('workers')
    require(isinstance(rows, list) and len(rows) == 1 and isinstance(rows[0], dict) and rows[0].get('role') == role,
            'physical load report role mismatch')
    row = rows[0]
    require(set(row) == {'role', 'latency', 'capped', 'tail_requests', 'scheduled_arrivals',
                         'skipped_arrivals', 'scheduled_latency', 'scheduling_delay', 'activity'}
            and type(row['role']) is int and type(row['capped']) is bool, 'load row fields/types')
    for name in ('latency', 'scheduled_latency', 'scheduling_delay'):
        require(isinstance(row[name], dict) and set(row[name]) == {'requests', 'median_us', 'p95_us'},
                'latency fields')
    counts = [row['scheduled_arrivals'], row['latency']['requests'], row['skipped_arrivals'], row['tail_requests']]
    require(all(type(n) is int and n >= 0 for n in counts), 'arrival count type/bound')
    scheduled, completed, skipped, tail = counts
    require(scheduled == 5000 and completed + skipped + tail == scheduled, 'arrival accounting mismatch')
    activity = row['activity']
    require(isinstance(activity, list) and len(activity) == 2, 'two physical client activity receipts required')
    timed, tail_only = 0, 0
    for index, client in enumerate(activity):
        require(isinstance(client, dict) and set(client) == {'client_index', 'timed_requests', 'first_request_offset_ns',
                                                           'last_timed_completion_offset_ns'}
                and type(client['client_index']) is int and client['client_index'] == index
                and type(client['timed_requests']) is int and 0 <= client['timed_requests'] <= 2500, 'client activity identity/count')
        first, last = client['first_request_offset_ns'], client['last_timed_completion_offset_ns']
        require(first is None or type(first) is int and 0 <= first < 2**63, 'first request offset')
        if client['timed_requests']:
            require(first is not None and type(last) is int and first <= last <= WINDOW_NS, 'timed activity bounds')
        else:
            require(last is None, 'completion without timed requests')
            tail_only += first is not None
        timed += client['timed_requests']
    require(timed == completed and tail_only <= tail, 'client activity/load count mismatch')
    require(row['scheduled_latency']['requests'] == row['scheduling_delay']['requests'] == completed,
            'physical latency sample count mismatch')
    for name in ('latency', 'scheduled_latency', 'scheduling_delay'):
        value = row[name]
        require(isinstance(value, dict) and set(value) == {'requests', 'median_us', 'p95_us'}
                and type(value['requests']) is int, 'latency fields')
        for key in ('median_us', 'p95_us'):
            number = value[key]
            require(number is None if completed == 0 else
                    type(number) in (float, int) and math.isfinite(number) and number >= 0, 'latency value')
    return (['offered_rate_not_sustained'] if completed < 4950 else []) + (['sample_cap'] if row['capped'] else [])


def decode_report(raw):
    require(len(raw) <= 16384, 'node load report cap')
    def unique(pairs):
        value = {}
        for key, item in pairs:
            require(key not in value, 'duplicate load report field')
            value[key] = item
        return value
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: require(False, 'non-finite report number'))


class Measurement:
    def __init__(self, base, worker, worker_binary, probe_source, probe_sha256, config,
                 deadline, read_line, drain_factory, *, interface):
        self.base, self.deadline, self.read_line = Path(base), deadline, read_line
        self.interface = interface
        self.environment_series = None
        self.child, self.stderr, self.ran = None, None, False
        self.progress = {'schema_version': 1, 'kind': 'pi-node-measurement-incomplete',
                         'stage': 'prepare', 'samples': 0, 'missed_samples': 0, 'acceptance_run': False}
        self.config = config
        require(set(config) == {'schema_version', 'workers', 'role', 'target'}
                and type(config['workers']) is int and 3 <= config['workers'] <= 5
                and type(config['role']) is int and 0 <= config['role'] < config['workers']
                and type(config['schema_version']) is int and config['schema_version'] == 2, 'node load configuration')
        require(isinstance(config['target'], dict) and set(config['target']) == {'socket', 'formation', 'node'}
                and config['target']['socket'] == str(self.base / 'api.sock'), 'load target must be this session socket')
        metadata = artifact(probe_source)
        require(metadata.get('elf64_aarch64') and metadata.get('executable')
                and metadata['sha256'] == probe_sha256, 'updated probe artifact mismatch')
        # Copy only this explicitly named artifact; never overwrite staged tools.
        destination = self.base / 'bin' / 'formation-telemetry-probe-node'
        digest, copied = hashlib.sha256(), 0
        with Path(probe_source).open('rb') as incoming, destination.open('xb') as outgoing:
            while raw := incoming.read(1048576):
                copied += len(raw)
                require(copied <= 256 * 1024 * 1024, 'probe snapshot byte cap')
                digest.update(raw)
                outgoing.write(raw)
        require(digest.hexdigest() == probe_sha256 and copied == metadata['bytes'], 'probe changed during snapshot')
        destination.chmod(0o700)
        self.worker = watchdog_child(worker, worker_binary)
        self.supervisor = Process(os.getpid())
        write_new_json(self.base / 'load.json', config)
        self.child = subprocess.Popen(['timeout', '--preserve-status', '--signal=TERM', '--kill-after=5s',
                                       '60s', str(destination), 'load-node', str(self.base / 'load.json')],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      bufsize=0, start_new_session=True)
        self.stderr = drain_factory(self.child.stderr)
        try:
            require(read_line(self.child.stdout, min(deadline, time.monotonic() + 15)) == b'READY',
                    'node load warmup marker')
            self.probe = watchdog_child(self.child, destination)
        except BaseException:
            self.close()
            raise

    def run(self, start_unix_ns):
        require(not self.ran, 'measurement cannot be replayed')
        require(type(start_unix_ns) is int, 'integer Unix nanoseconds required')
        now_mono, now_wall = time.monotonic_ns(), time.time_ns()
        wait_ns = start_unix_ns - now_wall
        require(2_000_000_000 <= wait_ns <= 20_000_000_000, 'start must be 2–20 seconds in the future')
        require(time.monotonic() + wait_ns / 1e9 + 17 < self.deadline, 'insufficient session lifetime')
        self.ran = True
        self.progress.update(stage='waiting-for-start', scheduled_start_unix_ns=start_unix_ns)
        planned = now_mono + wait_ns
        processes = (self.worker, self.probe, self.supervisor)
        while time.monotonic_ns() < planned:
            time.sleep(min(0.05, max(0, planned - time.monotonic_ns()) / 1e9))
        before = [process.sample() for process in processes]
        peaks = [row['rss_kib'] for row in before]
        swaps = [row['swap_kib'] for row in before]
        actual_mono, actual_wall = time.monotonic_ns(), time.time_ns()
        self.progress.update(stage='sampling', start_unix_ns=actual_wall)
        require(self.child.poll() is None, 'load exited before start')
        environment = KernelSeries(self.base.name, self.interface, actual_mono)
        self.environment_series = environment
        self.child.stdin.write(b'G')
        end = actual_mono + WINDOW_NS
        next_sample, samples, missed = actual_mono, 0, 0
        while time.monotonic_ns() < end:
            environment.sample_if_due()
            for index, process in enumerate(processes):
                current = process.sample()
                peaks[index] = max(peaks[index], current['rss_kib'])
                swaps[index] = max(swaps[index], current['swap_kib'])
            samples += 1
            next_sample += PERIOD_NS
            now = time.monotonic_ns()
            if next_sample < now:
                skipped = (now - next_sample) // PERIOD_NS + 1
                missed += skipped
                next_sample += skipped * PERIOD_NS
            time.sleep(max(0, min(end, next_sample) - time.monotonic_ns()) / 1e9)
            self.progress.update(samples=samples, missed_samples=missed)
        self.progress['stage'] = 'end-marker'
        require(self.read_line(self.child.stdout, min(self.deadline, time.monotonic() + 2)) == b'END', 'node load end marker')
        after = [process.sample() for process in processes]
        ended_mono, ended_wall = time.monotonic_ns(), time.time_ns()
        self.child.stdin.write(b'A')
        raw = self.read_line(self.child.stdout, min(self.deadline, time.monotonic() + 5))
        self.progress.update(stage='report', report_sha256=hashlib.sha256(raw).hexdigest())
        report = decode_report(raw)
        issues = load_issues(report, self.config['workers'], self.config['role'])
        self.child.stdin.close()
        require(self.child.wait(timeout=2) == 0, 'node load exit')
        resources = [resource_delta(a, b, peak, swap) for a, b, peak, swap in zip(before, after, peaks, swaps)]
        result = {'schema_version': 2, 'kind': 'pi-node-measurement', 'load': report,
                  'environment_series': environment.finish(),
                  'target': self.config['target'], 'resources': dict(zip(('worker', 'probe', 'supervisor'), resources)),
                  'scheduled_start_unix_ns': start_unix_ns, 'start_unix_ns': actual_wall, 'end_unix_ns': ended_wall,
                  'local_start_lateness_ns': actual_mono - planned,
                  'local_wall_mapping_change_ns': (actual_wall - now_wall) - (actual_mono - now_mono),
                  'window_wall_change_ns': (ended_wall - actual_wall) - (ended_mono - actual_mono),
                  'load_control_bracket_ns': ended_mono - actual_mono, 'samples': samples, 'missed_samples': missed,
                  'issues': issues, 'clock_uncertainty_qualified': False, 'acceptance_run': False}
        if missed:
            result['issues'].append('resource_sampling_missed')
        write_new_json(self.base / 'measurement.json', result)
        self.progress['stage'] = 'complete'
        return result

    def close(self):
        if self.child is None:
            return {'started': False}
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
        stderr = self.stderr.finish()
        if self.progress['stage'] != 'complete':
            if self.environment_series is not None:
                self.progress['environment_series'] = self.environment_series.finish()
            write_new_json(self.base / 'measurement-incomplete.json', self.progress)
        return {'started': True, 'timed_window_started': self.ran,
                'exit': child.returncode, 'forced': forced, 'stderr': stderr}
