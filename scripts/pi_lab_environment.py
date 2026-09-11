"""Bounded endpoint and periodic host evidence, not worker-attributed telemetry.

Firmware commands run outside the load/CPU window. NIC counters include SSH
and other host traffic; sampled temperatures are not a continuous peak.
"""
from pathlib import Path
import re
import time

from pi_lab_node import bounded_command, require

COUNTERS = ('rx_bytes', 'tx_bytes', 'rx_errors', 'tx_errors', 'rx_dropped', 'tx_dropped')
FIELDS = {'schema_version', 'kind', 'run_id', 'interface', 'ifindex', 'boot_id',
          'monotonic_before_ns', 'monotonic_after_ns', 'network', 'temperature_millicelsius',
          'firmware_throttled', 'unavailable'}


def read(path):
    with path.open('rb') as source:
        raw = source.read(129)
    require(len(raw) <= 128, 'environment input bound')
    return raw.decode('ascii').strip()


def natural(value, maximum=2**64 - 1):
    require(type(value) is int and 0 <= value <= maximum, 'environment integer bound/type')


def validate(value, run_id, interface):
    require(isinstance(value, dict) and set(value) == FIELDS, 'environment receipt fields')
    require(type(value['schema_version']) is int and value['schema_version'] == 1
            and value['kind'] == 'pi-environment' and value['run_id'] == run_id
            and value['interface'] == interface, 'environment identity/version')
    require(isinstance(run_id, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', run_id)
            and isinstance(interface, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', interface),
            'environment run/interface')
    natural(value['ifindex'], 2**31 - 1)
    require(value['ifindex'] > 0 and isinstance(value['boot_id'], str)
            and re.fullmatch(r'[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}', value['boot_id']), 'environment device/boot identity')
    for key in ('monotonic_before_ns', 'monotonic_after_ns'):
        natural(value[key], 2**63 - 1)
    require(value['monotonic_after_ns'] >= value['monotonic_before_ns'], 'environment bracket order')
    require(isinstance(value['network'], dict) and set(value['network']) == set(COUNTERS), 'network counter fields')
    for count in value['network'].values():
        natural(count)
    missing = []
    temperature = value['temperature_millicelsius']
    if temperature is None:
        missing.append('temperature')
    else:
        require(type(temperature) is int and -273150 <= temperature <= 300000, 'temperature bound/type')
    if value['firmware_throttled'] is None:
        missing.append('firmware_throttled')
    else:
        natural(value['firmware_throttled'], 2**32 - 1)
    require(value['unavailable'] == missing, 'environment availability accounting')


def snapshot(run_id, interface, *, sys_root=Path('/sys'), proc_root=Path('/proc'), command=bounded_command,
             include_firmware=True):
    # Roots are local test seams, never remote request arguments.
    require(isinstance(interface, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', interface),
            'environment interface')
    require(type(include_firmware) is bool, 'firmware selection type')
    began = time.monotonic_ns()
    net = sys_root / 'class/net' / interface
    index = int(read(net / 'ifindex'))
    boot = read(proc_root / 'sys/kernel/random/boot_id')
    counters = {name: int(read(net / 'statistics' / name)) for name in COUNTERS}
    missing, temperature, flags = [], None, None
    try:
        zone = sys_root / 'class/thermal/thermal_zone0'
        require(read(zone / 'type') == 'cpu-thermal', 'unexpected thermal sensor')
        temperature = int(read(zone / 'temp'))
        require(-273150 <= temperature <= 300000, 'temperature range')
    except (OSError, ValueError):
        temperature = None
        missing.append('temperature')
    # Periodic kernel-only samples never spawn a firmware subprocess. The
    # enclosing series explicitly records that firmware was not sampled.
    if include_firmware:
        try:
            response = command(['vcgencmd', 'get_throttled'], timeout=3, cap=128)
            require(response['exit'] == 0, 'firmware command unavailable')
            match = re.fullmatch(r'throttled=0x([0-9a-fA-F]{1,8})\s*', response['stdout'])
            require(match is not None, 'firmware reply shape')
            flags = int(match.group(1), 16)
        except (OSError, ValueError, TimeoutError):
            missing.append('firmware_throttled')
    else:
        missing.append('firmware_throttled')
    require(index == int(read(net / 'ifindex')) and boot == read(proc_root / 'sys/kernel/random/boot_id'),
            'environment identity changed while sampling')
    value = {'schema_version': 1, 'kind': 'pi-environment', 'run_id': run_id, 'interface': interface,
             'ifindex': index, 'boot_id': boot, 'network': counters, 'monotonic_before_ns': began,
             'monotonic_after_ns': time.monotonic_ns(), 'temperature_millicelsius': temperature,
             'firmware_throttled': flags, 'unavailable': missing}
    validate(value, run_id, interface)
    return value


def compare(before, after, run_id, interface):
    for value in (before, after):
        validate(value, run_id, interface)
    issues = []
    if before['boot_id'] != after['boot_id']:
        issues.append('boot_identity_changed')
    if before['ifindex'] != after['ifindex']:
        issues.append('network_interface_replaced')
    if after['monotonic_before_ns'] <= before['monotonic_after_ns']:
        issues.append('environment_brackets_not_ordered')
    if any(after['network'][key] < before['network'][key] for key in COUNTERS):
        issues.append('network_counter_regressed')
    delta = None if issues else {key: after['network'][key] - before['network'][key] for key in COUNTERS}
    if delta is not None and any(delta[key] for key in COUNTERS if key.endswith(('_errors', '_dropped'))):
        issues.append('host_network_errors_or_drops_in_bracket')
    for side, value in (('before', before), ('after', after)):
        issues.extend(f'{side}_{name}_unavailable' for name in value['unavailable'])
        if value['firmware_throttled']:
            issues.append(f'{side}_firmware_flags_nonzero')
    return {'schema_version': 1, 'kind': 'pi-environment-comparison', 'scope': 'host_pre_post_bracket',
            'network_delta': delta, 'before': before, 'after': after, 'issues': issues,
            'continuous_temperature_peak_measured': False, 'acceptance_run': False}


SERIES_PERIOD_NS = 1_000_000_000
SERIES_WINDOW_NS = 10_000_000_000


class KernelSeries:
    """At most one kernel-only snapshot per one-second slot, never catch-up IO.

    Called inside the supervisor's CPU bracket. Kernel IO time contributes to
    supervisor accounting and delayed/missing sampling, not worker CPU. Node
    and coordinator watchdogs remain responsible for a stalled kernel read.
    """
    def __init__(self, run_id, interface, start_ns):
        self.run_id, self.interface, self.start_ns = run_id, interface, start_ns
        self.samples = []

    def sample_if_due(self):
        before = time.monotonic_ns()
        slot = (before - self.start_ns) // SERIES_PERIOD_NS
        if not 0 <= slot < 10 or self.samples and slot <= self.samples[-1]['slot']:
            return
        row = {'slot': slot, 'begin_offset_ns': before - self.start_ns}
        try:
            row['snapshot'] = snapshot(self.run_id, self.interface, include_firmware=False)
        except (OSError, ValueError, TimeoutError) as error:
            row['error_type'] = 'OSError' if isinstance(error, OSError) else 'ValueError'
        row['end_offset_ns'] = time.monotonic_ns() - self.start_ns
        self.samples.append(row)

    def finish(self):
        return {'schema_version': 1, 'kind': 'pi-kernel-environment-series',
                'run_id': self.run_id, 'interface': self.interface, 'monotonic_start_ns': self.start_ns,
                'period_ns': SERIES_PERIOD_NS, 'window_ns': SERIES_WINDOW_NS,
                'samples': self.samples, 'missed_samples': 10 - len(self.samples),
                'firmware_sampled': False, 'acceptance_run': False}


def review_series(value, run_id, interface, control_bracket_ns):
    """Validate raw sample slots and retain missing/failed data, never zero it.

    Counter differences span the first/last successful reads, not exactly ten
    seconds and not worker-only traffic. A sampled temperature maximum is not
    a continuous peak. Firmware remains a separate pre/post instrument.
    """
    require(isinstance(value, dict) and set(value) == {'schema_version', 'kind', 'run_id', 'interface',
            'monotonic_start_ns', 'period_ns', 'window_ns', 'samples', 'missed_samples',
            'firmware_sampled', 'acceptance_run'}, 'environment series fields')
    require(type(value['schema_version']) is int and value['schema_version'] == 1
            and value['kind'] == 'pi-kernel-environment-series' and value['run_id'] == run_id
            and value['interface'] == interface and value['firmware_sampled'] is False
            and value['acceptance_run'] is False, 'environment series identity/version')
    require(isinstance(run_id, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', run_id)
            and isinstance(interface, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', interface),
            'environment series run/interface')
    for field in ('monotonic_start_ns', 'period_ns', 'window_ns', 'missed_samples'):
        natural(value[field], 2**63 - 1)
    natural(control_bracket_ns, 2**63 - 1)
    require(value['period_ns'] == SERIES_PERIOD_NS and value['window_ns'] == SERIES_WINDOW_NS,
            'environment series profile')
    rows = value['samples']
    require(isinstance(rows, list) and len(rows) <= 10 and value['missed_samples'] == 10 - len(rows),
            'environment sample coverage')
    previous_slot, previous_end = -1, -1
    good, temperatures, issues = [], [], []
    if value['missed_samples']:
        issues.append('environment_samples_missed')
    for row in rows:
        require(isinstance(row, dict) and set(row) in (
            {'slot', 'begin_offset_ns', 'end_offset_ns', 'snapshot'},
            {'slot', 'begin_offset_ns', 'end_offset_ns', 'error_type'}), 'environment sample fields')
        for key in ('slot', 'begin_offset_ns', 'end_offset_ns'):
            natural(row[key], 2**63 - 1)
        require(previous_slot < row['slot'] < 10 and previous_end <= row['begin_offset_ns']
                and row['slot'] * SERIES_PERIOD_NS <= row['begin_offset_ns'] < (row['slot'] + 1) * SERIES_PERIOD_NS
                and row['begin_offset_ns'] <= row['end_offset_ns'] <= control_bracket_ns, 'environment sample slot/bracket')
        previous_slot, previous_end = row['slot'], row['end_offset_ns']
        if row['end_offset_ns'] > SERIES_WINDOW_NS:
            issues.append('environment_sample_crossed_window_end')
        if 'error_type' in row:
            require(row['error_type'] in ('OSError', 'FileNotFoundError', 'PermissionError', 'ValueError', 'TimeoutError'),
                    'environment sample error class')
            issues.append('environment_sample_failed')
            continue
        sample = row['snapshot']
        validate(sample, run_id, interface)
        require(sample['firmware_throttled'] is None, 'firmware must not be sampled in CPU window')
        require(value['monotonic_start_ns'] + row['begin_offset_ns'] <= sample['monotonic_before_ns']
                <= sample['monotonic_after_ns'] <= value['monotonic_start_ns'] + row['end_offset_ns'],
                'kernel snapshot outside sampling bracket')
        if sample['temperature_millicelsius'] is None:
            issues.append('temperature_sample_unavailable')
        else:
            temperatures.append(sample['temperature_millicelsius'])
        good.append(sample)
    comparable = len(good) == 10
    for before, after in zip(good, good[1:]):
        compared = compare(before, after, run_id, interface)
        issues.extend(item for item in compared['issues'] if not item.endswith('_firmware_throttled_unavailable'))
        comparable = comparable and compared['network_delta'] is not None
    delta, bracket = None, None
    if comparable:
        first, last = good[0], good[-1]
        delta = {key: last['network'][key] - first['network'][key] for key in COUNTERS}
        bracket = [last['monotonic_before_ns'] - first['monotonic_after_ns'],
                   last['monotonic_after_ns'] - first['monotonic_before_ns']]
    return {'schema_version': 1, 'kind': 'pi-kernel-environment-review',
            'issues': sorted(set(issues)), 'missed_samples': value['missed_samples'],
            'failed_samples': len(rows) - len(good), 'sampled_temperature_peak_millicelsius': max(temperatures, default=None),
            'temperature_samples_complete': len(temperatures) == 10,
            'network_delta': delta, 'network_bracket_ns': bracket, 'scope': 'host_sample_brackets',
            'firmware_sampled': False, 'continuous_temperature_peak_measured': False,
            'sampler_cpu_owner': 'supervisor', 'acceptance_run': False}
