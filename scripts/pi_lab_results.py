#!/usr/bin/env python3
"""Strict reader for one node-local physical pilot receipt, never M4 acceptance.

The caller supplies the expected configuration from its own run manifest, not
from the received measurement. Mixed nodes are deliberately not pooled here.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re

from pi_lab_measurement import WINDOW_NS, load_issues, probe_clock_bounds
from pi_lab_clock import assess
from pi_lab_environment import review_series
from pi_lab_node import require
from pi_lab_session import CAP, decode

FIELDS = {'schema_version', 'kind', 'load', 'target', 'resources', 'scheduled_start_unix_ns',
          'start_unix_ns', 'end_unix_ns', 'local_start_lateness_ns', 'local_wall_mapping_change_ns',
          'window_wall_change_ns', 'load_control_bracket_ns', 'samples', 'missed_samples',
          'issues', 'clock_uncertainty_qualified', 'acceptance_run', 'environment_series'}
RESOURCE_FIELDS = {'cpu_nanoseconds', 'cpu_seconds', 'cpu_bracket_ns', 'rss_peak_kib',
                   'swap_peak_kib', 'setup_hwm_kib', 'lifetime_hwm_kib'}


def integer(value, *, signed=False):
    require(type(value) is int and (-2**63 if signed else 0) <= value < 2**63,
            'measurement integer bound/type')


def configuration(value):
    require(isinstance(value, dict) and set(value) == {'schema_version', 'workers', 'role', 'target'},
            'expected measurement configuration fields')
    require(type(value['schema_version']) is int and value['schema_version'] == 2
            and type(value['workers']) is int and 3 <= value['workers'] <= 5
            and type(value['role']) is int and 0 <= value['role'] < value['workers'], 'expected profile')
    target = value['target']
    require(isinstance(target, dict) and set(target) == {'socket', 'formation', 'node'}, 'expected target fields')
    for key in ('formation', 'node'):
        require(isinstance(target[key], str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', target[key]),
                'expected formation/node identity')
    socket = target['socket']
    require(isinstance(socket, str) and socket.startswith('/') and len(socket.encode()) <= 100
            and all(ord(c) >= 32 and ord(c) != 127 for c in socket)
            and '..' not in Path(socket).parts, 'expected private Unix socket')


def review(value, expected):
    """Reject invalid receipts; retain valid measurements with unmet gates."""
    configuration(expected)
    require(isinstance(value, dict) and set(value) == FIELDS, 'measurement receipt fields')
    require(type(value['schema_version']) is int and value['schema_version'] == 2
            and value['kind'] == 'pi-node-measurement', 'measurement receipt version/kind')
    require(value['target'] == expected['target'], 'measurement target/run mismatch')
    require(value['clock_uncertainty_qualified'] is False and value['acceptance_run'] is False,
            'node receipt cannot qualify a distributed window or acceptance')
    issues = load_issues(value['load'], expected['workers'], expected['role'])
    probe_clock = probe_clock_bounds(value['load']['timing'])
    for name in ('scheduled_start_unix_ns', 'start_unix_ns', 'end_unix_ns', 'local_start_lateness_ns',
                 'load_control_bracket_ns', 'samples', 'missed_samples'):
        integer(value[name])
    for name in ('local_wall_mapping_change_ns', 'window_wall_change_ns'):
        integer(value[name], signed=True)
    require(value['load_control_bracket_ns'] >= WINDOW_NS and 0 < value['samples'] <= 200,
            'measurement window/sample shape')
    require(probe_clock['end_marker_elapsed_ns'] <= value['load_control_bracket_ns'], 'probe window exceeds supervisor bracket')
    require(value['end_unix_ns'] - value['start_unix_ns'] ==
            value['load_control_bracket_ns'] + value['window_wall_change_ns'], 'measurement wall/monotonic accounting')
    require(value['start_unix_ns'] - value['scheduled_start_unix_ns'] ==
            value['local_start_lateness_ns'] + value['local_wall_mapping_change_ns'], 'measurement start accounting')
    if value['missed_samples']:
        issues.append('resource_sampling_missed')
    require(value['issues'] == issues, 'measurement issue accounting mismatch')
    environment = value['environment_series']
    require(isinstance(environment, dict), 'environment series object')
    environment_review = review_series(environment, Path(expected['target']['socket']).parent.name,
                                       environment.get('interface'), value['load_control_bracket_ns'])
    resources = value['resources']
    require(isinstance(resources, dict) and set(resources) == {'worker', 'probe', 'supervisor'},
            'worker/probe/supervisor accounting required separately')
    rates = {}
    for name, resource in resources.items():
        require(isinstance(resource, dict) and set(resource) == RESOURCE_FIELDS, 'resource receipt fields')
        for field in RESOURCE_FIELDS - {'cpu_seconds'}:
            integer(resource[field])
        seconds = resource['cpu_seconds']
        require(type(seconds) in (int, float) and math.isfinite(seconds)
                and seconds == resource['cpu_nanoseconds'] / 1e9, 'CPU seconds/nanoseconds mismatch')
        require(resource['cpu_bracket_ns'] > 0 and resource['rss_peak_kib'] > 0
                and resource['lifetime_hwm_kib'] >= max(resource['setup_hwm_kib'], resource['rss_peak_kib']),
                'resource bracket/memory accounting')
        rates[name] = 100 * resource['cpu_nanoseconds'] / resource['cpu_bracket_ns']
    # There is no cross-mode baseline, clock drift/overlap proof, collector or
    # thermal/network qualification in a single node receipt. Never infer one.
    return {'schema_version': 1, 'kind': 'pi-node-measurement-review', 'status': 'valid_unqualified',
            'workers': expected['workers'], 'role': expected['role'], 'target': expected['target'],
            'measurement_issues': issues, 'qualification_issues': ['distributed_window_unqualified'],
            'cpu_percent_of_one_core': rates, 'resources': resources,
            'timing': {key: value[key] for key in (
                'scheduled_start_unix_ns', 'start_unix_ns', 'end_unix_ns', 'local_start_lateness_ns',
                'local_wall_mapping_change_ns', 'window_wall_change_ns', 'load_control_bracket_ns',
                'samples', 'missed_samples')},
            'load': value['load'], 'probe_clock': probe_clock, 'environment': environment_review,
            'clock_uncertainty_qualified': False, 'acceptance_run': False}


def read_report(path, expected, sha256):
    require(isinstance(sha256, str) and re.fullmatch(r'[0-9a-f]{64}', sha256), 'expected report digest')
    with Path(path).open('rb') as source:
        raw = source.read(CAP + 1)
    require(len(raw) <= CAP, 'measurement file byte cap')
    require(hashlib.sha256(raw).hexdigest() == sha256, 'measurement digest mismatch')
    return review(decode(raw), expected) | {'source_sha256': sha256}


def overlap_policy(policy):
    """Validate explicit offline limits, without authorizing their assumptions."""
    fields = {'max_round_trip_ns', 'max_offset_width_ns', 'max_wall_change_ns',
              'max_offset_change_ns', 'max_collection_span_ns', 'max_pairwise_start_skew_ns',
              'min_common_window_ns', 'max_first_request_offset_ns', 'min_last_completion_offset_ns'}
    require(isinstance(policy, dict) and set(policy) == fields, 'overlap policy fields')
    for value in policy.values():
        integer(value)
    require(0 < policy['max_collection_span_ns'] <= 45_000_000_000
            and 0 < policy['min_common_window_ns'] <= WINDOW_NS
            and policy['max_first_request_offset_ns'] <= WINDOW_NS
            and policy['min_last_completion_offset_ns'] <= WINDOW_NS, 'overlap policy horizon')


def review_overlap(rows, expected, run_id, policy):
    """Bound actual probe-window overlap, conditional on explicit clock limits.

    Inputs are raw measurements and both exchanges from the same collection,
    with trusted expected configurations. Revalidate them instead of trusting
    cached reviews. No clock midpoint or scheduled start substitutes for the
    probe's actual start. End-marker lateness is subtracted, not counted as
    additional load. Bounds assume monotonic wall clocks, a wall/monotonic
    mapping change no greater than max_wall_change_ns over ANY subinterval,
    and offset change no greater than max_offset_change_ns throughout the
    collection. Endpoint checks can contradict but cannot prove those
    assumptions (for example, a step and its reversal between samples).
    """
    overlap_policy(policy)
    require(isinstance(rows, list) and 3 <= len(rows) <= 5
            and isinstance(expected, list) and len(expected) == len(rows), 'overlap cohort coverage')
    for role, config in enumerate(expected):
        configuration(config)
        require(config['role'] == role and config['workers'] == len(rows), 'overlap expected role/count')
    require(len({config['target']['formation'] for config in expected}) == 1
            and len({config['target']['node'] for config in expected}) == len(rows), 'overlap membership identity')
    checks = {key: policy[key] for key in ('max_round_trip_ns', 'max_offset_width_ns', 'max_wall_change_ns')}
    nonces, stamps, nodes, issues = set(), [], [], []
    mapping = policy['max_wall_change_ns']
    for role, (row, config) in enumerate(zip(rows, expected)):
        require(isinstance(row, dict) and set(row) == {'role', 'measurement', 'clock_before', 'clock_after'}
                and type(row['role']) is int and row['role'] == role, 'overlap role/receipt fields')
        checked = review(row['measurement'], config)
        local_issues = []
        exchanges = [row['clock_before'], row['clock_after']]
        for exchange in exchanges:
            local_issues.extend(assess(exchange, **checks))
            require(exchange['run_id'] == run_id and exchange['nonce'] not in nonces, 'overlap clock run/replay')
            nonces.add(exchange['nonce'])
            stamps.extend((exchange['coordinator_before'], exchange['coordinator_after']))
        before, after = exchanges
        require(before['coordinator_after']['monotonic_after_ns'] <= after['coordinator_before']['monotonic_before_ns']
                and before['node']['monotonic_after_ns'] <= after['node']['monotonic_before_ns'], 'overlap exchange order')
        first, last = before['node'], after['node']
        delta = last['unix_ns'] - first['unix_ns']
        if max(abs(delta - (last['monotonic_before_ns'] - first['monotonic_after_ns'])),
               abs(delta - (last['monotonic_after_ns'] - first['monotonic_before_ns']))) > mapping:
            local_issues.append('node_wall_clock_change')
        offsets = [value['offset_bracket_ns'] for value in exchanges]
        if max(low for low, _ in offsets) - min(high for _, high in offsets) > policy['max_offset_change_ns']:
            local_issues.append('offset_change_limit')
        # Use both complete envelopes plus the whole-collection change bound.
        # Do not narrow uncertainty by selecting the faster exchange.
        offset = [min(low for low, _ in offsets) - policy['max_offset_change_ns'],
                  max(high for _, high in offsets) + policy['max_offset_change_ns']]
        probe = checked['probe_clock']
        # ClockMark associates the instant AFTER its wall-clock read. Extend
        # the read bracket for the allowed wall/monotonic mapping change too;
        # a nonzero rate error must never narrow an uncertainty interval.
        start = list(probe['start_wall_interval_ns'])
        marker = list(probe['end_marker_wall_interval_ns'])
        start[1] += mapping
        marker[1] += mapping
        if max(abs(n) for n in probe['wall_change_interval_ns']) > mapping:
            local_issues.append('probe_wall_clock_change')
        if start[0] < first['unix_ns'] - mapping or marker[1] > last['unix_ns'] + mapping:
            local_issues.append('probe_outside_clock_bracket')
        timing = checked['timing']
        if start[0] < timing['start_unix_ns'] - mapping or marker[1] > timing['end_unix_ns'] + mapping:
            local_issues.append('probe_outside_supervisor_bracket')
        lateness = probe['end_marker_elapsed_ns'] - WINDOW_NS
        start_reference = [start[0] - offset[1], start[1] - offset[0]]
        end_reference = [marker[0] - lateness - mapping - offset[1],
                         marker[1] - lateness + mapping - offset[0]]
        activity = checked['load']['workers'][0]['activity']
        for client in activity:
            first_request, last_completion = client['first_request_offset_ns'], client['last_timed_completion_offset_ns']
            if first_request is None or last_completion is None:
                local_issues.append('client_has_no_timed_activity')
            else:
                if first_request > policy['max_first_request_offset_ns']:
                    local_issues.append('client_first_request_late')
                if last_completion < policy['min_last_completion_offset_ns']:
                    local_issues.append('client_last_completion_early')
        if local_issues:
            issues.append({'role': role, 'issues': sorted(set(local_issues))})
        nodes.append({'role': role, 'offset_envelope_ns': offset,
                      'probe_start_reference_interval_ns': start_reference,
                      'probe_end_reference_interval_ns': end_reference,
                      'end_marker_lateness_ns': lateness, 'client_activity': activity,
                      'measurement_issues': checked['measurement_issues']})
    # The coordinator's own clock mapping must hold across the entire cohort,
    # not merely inside each short exchange. Monotonic values from different
    # nodes are never compared with one another or with the coordinator.
    anchor = min(stamps, key=lambda value: value['monotonic_before_ns'])
    span = max(value['monotonic_after_ns'] for value in stamps) - anchor['monotonic_before_ns']
    cohort_issues = []
    if span > policy['max_collection_span_ns']:
        cohort_issues.append('collection_span_limit')
    for value in stamps:
        delta = value['unix_ns'] - anchor['unix_ns']
        if max(abs(delta - (value['monotonic_before_ns'] - anchor['monotonic_after_ns'])),
               abs(delta - (value['monotonic_after_ns'] - anchor['monotonic_before_ns']))) > mapping:
            cohort_issues.append('coordinator_wall_clock_change')
            break
    skew = max(a['probe_start_reference_interval_ns'][1] - b['probe_start_reference_interval_ns'][0]
               for a in nodes for b in nodes if a['role'] != b['role'])
    common = max(0, min(node['probe_end_reference_interval_ns'][0] for node in nodes)
                 - max(node['probe_start_reference_interval_ns'][1] for node in nodes))
    if skew > policy['max_pairwise_start_skew_ns']:
        cohort_issues.append('pairwise_probe_start_skew_limit')
    if common < policy['min_common_window_ns']:
        cohort_issues.append('common_probe_window_too_short')
    # If observed evidence contradicts a clock assumption, numeric bounds are
    # diagnostic only. Never present them as a qualified common duration.
    return {'schema_version': 1, 'kind': 'pi-window-overlap-review', 'run_id': run_id,
            'status': 'outside_conditional_bounds' if issues or cohort_issues else 'within_conditional_bounds',
            'policy': dict(policy), 'nodes': nodes, 'node_issues': issues, 'cohort_issues': cohort_issues,
            'collection_span_ns': span, 'conditional_pairwise_start_skew_upper_ns': skew,
            'conditional_common_window_lower_ns': common,
            'assumptions': ['monotonic_wall_clocks_without_steps', 'bounded_wall_monotonic_mapping_on_every_subinterval',
                            'bounded_offset_change_throughout_collection'],
            'activity_scope': 'first request and last completion only, not continuous activity',
            'clock_uncertainty_qualified': False, 'acceptance_run': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', required=True)
    parser.add_argument('--expected-config', required=True, help='trusted coordinator load configuration, not received target')
    parser.add_argument('--sha256', required=True, help='digest captured by the trusted collection path')
    args = parser.parse_args()
    try:
        with Path(args.expected_config).open('rb') as source:
            raw = source.read(4097)
        require(len(raw) <= 4096, 'expected config byte cap')
        result = read_report(args.report, decode(raw), args.sha256)
    except (Exception, KeyboardInterrupt) as error:
        # Neither hostile fields nor credentials in exception messages escape.
        print(json.dumps({'schema_version': 1, 'kind': 'pi-node-measurement-review',
                          'status': 'invalid_or_unavailable', 'error_type': type(error).__name__,
                          'acceptance_run': False}))
        return 1
    print(json.dumps(result, allow_nan=False))
    return 0  # Valid receipt only, explicitly not a performance pass.


if __name__ == '__main__':
    raise SystemExit(main())
