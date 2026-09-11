"""Clock exchange evidence, not synchronization or performance acceptance.

Offsets are node wall time minus coordinator wall time. The causal envelope
assumes a non-stepping coordinator wall clock during this one exchange. Keep
the raw monotonic brackets so a reader can test that assumption; no midpoint,
NTP status, or fastest exchange proves the offset throughout a future window.
"""
import re
import time
import uuid

from pi_lab_node import require

STAMP_FIELDS = {'monotonic_before_ns', 'unix_ns', 'monotonic_after_ns'}


def stamp():
    before = time.monotonic_ns()
    wall = time.time_ns()
    after = time.monotonic_ns()
    return {'monotonic_before_ns': before, 'unix_ns': wall, 'monotonic_after_ns': after}


def validate_stamp(value):
    require(isinstance(value, dict) and set(value) == STAMP_FIELDS, 'clock stamp fields')
    require(all(type(n) is int and 0 <= n < 2**63 for n in value.values()), 'clock stamp integer bounds')
    require(value['monotonic_before_ns'] <= value['monotonic_after_ns'], 'clock bracket reversed')


def receipt(run_id, nonce):
    require(isinstance(nonce, str) and re.fullmatch(r'[0-9a-f]{32}', nonce), 'clock nonce')
    return {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': run_id,
            'nonce': nonce, 'stamp': stamp()}


def evidence(before, after, remote, run_id, nonce):
    """Validate one exact receipt and retain conservative exchange intervals."""
    require(isinstance(run_id, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', run_id), 'clock run ID')
    require(isinstance(nonce, str) and re.fullmatch(r'[0-9a-f]{32}', nonce), 'clock nonce')
    require(isinstance(remote, dict) and set(remote) ==
            {'schema_version', 'kind', 'run_id', 'nonce', 'stamp'}, 'clock receipt fields')
    require(type(remote['schema_version']) is int and remote['schema_version'] == 1
            and remote['kind'] == 'pi-clock-receipt' and remote['run_id'] == run_id
            and remote['nonce'] == nonce, 'clock receipt identity/version')
    for value in (before, after, remote['stamp']):
        validate_stamp(value)
    require(before['monotonic_after_ns'] <= after['monotonic_before_ns'], 'clock exchange order')
    # The wall samples can occur anywhere inside their monotonic brackets.
    elapsed_min = after['monotonic_before_ns'] - before['monotonic_after_ns']
    elapsed_max = after['monotonic_after_ns'] - before['monotonic_before_ns']
    wall_elapsed = after['unix_ns'] - before['unix_ns']
    node = remote['stamp']
    return {'schema_version': 1, 'kind': 'pi-clock-exchange', 'run_id': run_id, 'nonce': nonce,
            'coordinator_before': before, 'coordinator_after': after, 'node': node,
            'round_trip_bracket_ns': [elapsed_min, elapsed_max],
            'coordinator_wall_change_bracket_ns': [wall_elapsed - elapsed_max, wall_elapsed - elapsed_min],
            # Reversed wall clocks deliberately retain a reversed envelope,
            # never a fabricated zero uncertainty. Assessment rejects it.
            'offset_bracket_ns': [node['unix_ns'] - after['unix_ns'], node['unix_ns'] - before['unix_ns']],
            'clock_uncertainty_qualified': False, 'acceptance_run': False}


def assess(value, *, max_round_trip_ns, max_offset_width_ns, max_wall_change_ns):
    """Apply explicit exchange-only limits; this never qualifies a load window.

    No default policy is hidden here. Recompute derived fields from raw stamps
    before assessment so a saved receipt cannot forge a narrow interval.
    """
    for bound in (max_round_trip_ns, max_offset_width_ns, max_wall_change_ns):
        require(type(bound) is int and 0 <= bound < 2**63, 'clock policy bound')
    require(isinstance(value, dict), 'clock evidence object')
    require(type(value.get('schema_version')) is int
            and value.get('clock_uncertainty_qualified') is False
            and value.get('acceptance_run') is False, 'clock evidence types')
    rebuilt = evidence(value['coordinator_before'], value['coordinator_after'],
                       {'schema_version': 1, 'kind': 'pi-clock-receipt', 'run_id': value['run_id'],
                        'nonce': value['nonce'], 'stamp': value['node']}, value['run_id'], value['nonce'])
    require(value == rebuilt, 'clock evidence derived fields mismatch')
    require(all(type(n) is int for key in ('round_trip_bracket_ns', 'offset_bracket_ns',
                                          'coordinator_wall_change_bracket_ns')
                for n in value[key]), 'clock evidence interval types')
    lower, upper = value['offset_bracket_ns']
    change = value['coordinator_wall_change_bracket_ns']
    issues = []
    if value['round_trip_bracket_ns'][1] > max_round_trip_ns:
        issues.append('clock_round_trip_limit')
    if lower > upper or upper - lower > max_offset_width_ns:
        issues.append('clock_offset_envelope_limit')
    if max(abs(n) for n in change) > max_wall_change_ns:
        issues.append('coordinator_wall_clock_change')
    return issues


def exchange(call, run_id):
    """One synchronous allowlisted RPC, retaining send/receive clock brackets."""
    nonce = uuid.uuid4().hex
    before = stamp()
    remote = call('clock', nonce=nonce)
    after = stamp()
    return evidence(before, after, remote, run_id, nonce)


def validate_start_policy(policy):
    """Validate explicit proposal limits before any clock-session IO."""
    fields = {'lead_ns', 'max_age_ns', 'max_round_trip_ns', 'max_offset_width_ns',
              'max_wall_change_ns', 'max_relative_drift_ppm', 'max_start_uncertainty_ns',
              'max_local_start_lateness_ns', 'max_pairwise_start_skew_ns'}
    require(isinstance(policy, dict) and set(policy) == fields, 'start policy fields')
    require(all(type(n) is int and 0 <= n < 2**63 for n in policy.values()), 'start policy bounds/types')
    require(3_000_000_000 <= policy['lead_ns'] <= 15_000_000_000
            and 0 < policy['max_age_ns'] <= 60_000_000_000
            and policy['max_relative_drift_ppm'] <= 1_000_000, 'start policy horizon/rate')


def plan_start(observations, run_id, now, policy):
    """Propose offset-compensated starts under explicit, unapproved assumptions.

    All three exchanges per role contribute to the conservative envelope; no
    outlier or slow sample is dropped. Drift bounds offset change per elapsed
    coordinator wall-clock time with no steps; it is an assumption, not a
    measured fact. This pure function neither dispatches a request nor
    qualifies the ensuing load window.
    """
    validate_start_policy(policy)
    validate_stamp(now)
    require(isinstance(observations, list) and 3 <= len(observations) <= 5, 'start cohort bound')
    reference = now['unix_ns'] + policy['lead_ns']
    require(reference < 2**63, 'reference start overflow')
    checks = {key: policy[key] for key in ('max_round_trip_ns', 'max_offset_width_ns', 'max_wall_change_ns')}
    roles, nonces = [], set()
    for role, row in enumerate(observations):
        require(isinstance(row, dict) and set(row) == {'role', 'exchanges'}
                and type(row['role']) is int and row['role'] == role
                and isinstance(row['exchanges'], list) and len(row['exchanges']) == 3, 'start role/sample coverage')
        bounds, ages, margins = [], [], []
        previous_coordinator, previous_node = -1, -1
        for value in row['exchanges']:
            require(not assess(value, **checks), 'clock exchange exceeds proposed policy')
            require(value['run_id'] == run_id and value['nonce'] not in nonces, 'start clock run/replay')
            nonces.add(value['nonce'])
            before, after, node = value['coordinator_before'], value['coordinator_after'], value['node']
            require(before['monotonic_before_ns'] >= previous_coordinator
                    and node['monotonic_before_ns'] >= previous_node, 'clock samples reordered')
            previous_coordinator, previous_node = after['monotonic_after_ns'], node['monotonic_after_ns']
            require(now['monotonic_before_ns'] >= after['monotonic_after_ns'], 'clock sample is from the future')
            age = now['monotonic_after_ns'] - before['monotonic_before_ns']
            require(age <= policy['max_age_ns'], 'clock sample is stale')
            # Check the coordinator's mapping across the whole observation age,
            # not just across the network exchange. Reject steps/drift outside
            # the supplied tolerance rather than silently treating clocks equal.
            elapsed_min = now['monotonic_before_ns'] - after['monotonic_after_ns']
            elapsed_max = now['monotonic_after_ns'] - after['monotonic_before_ns']
            wall_elapsed = now['unix_ns'] - after['unix_ns']
            require(max(abs(wall_elapsed - elapsed_min), abs(wall_elapsed - elapsed_max))
                    <= policy['max_wall_change_ns'], 'coordinator clock mapping changed since exchange')
            # Ceil integer nanoseconds; never round an uncertainty inward.
            horizon = reference - before['unix_ns']
            require(horizon >= 0, 'reference precedes clock observation')
            margin = (horizon * policy['max_relative_drift_ppm'] + 999_999) // 1_000_000
            lower, upper = value['offset_bracket_ns']
            bounds.append((lower - margin, upper + margin))
            ages.append(age)
            margins.append(margin)
        # The expanded intervals must be mutually consistent, but use their
        # union extent, not their intersection, for the actual conservative plan.
        require(max(low for low, _ in bounds) <= min(high for _, high in bounds), 'clock intervals inconsistent with drift assumption')
        lower, upper = min(low for low, _ in bounds), max(high for _, high in bounds)
        offset = (lower + upper) // 2
        uncertainty = max(offset - lower, upper - offset)
        require(uncertainty <= policy['max_start_uncertainty_ns'], 'start uncertainty exceeds proposed policy')
        start = reference + offset
        require(0 <= start < 2**63, 'node start overflow')
        # Positive offset means this node's Unix deadline must be later, not
        # earlier, to target the same coordinator-reference instant.
        roles.append({'role': role, 'start_unix_ns': start, 'offset_bracket_ns': [lower, upper],
                      'selected_offset_ns': offset, 'start_uncertainty_ns': uncertainty,
                      'sample_ages_ns': ages, 'drift_margins_ns': margins,
                      'conditional_start_error_ns': [offset - upper,
                                                     offset - lower + policy['max_local_start_lateness_ns']]})
    skew = max(row['conditional_start_error_ns'][1] for row in roles) - min(
        row['conditional_start_error_ns'][0] for row in roles)
    require(skew <= policy['max_pairwise_start_skew_ns'], 'conditional start skew exceeds proposed policy')
    return {'schema_version': 1, 'kind': 'pi-start-proposal', 'run_id': run_id,
            'status': 'proposed_unqualified', 'policy': dict(policy), 'planned_at': dict(now),
            'reference_start_unix_ns': reference, 'roles': roles, 'conditional_pairwise_skew_ns': skew,
            'assumptions': ['bounded_relative_wall_clock_rate_without_steps', 'bounded_local_start_lateness'],
            'clock_uncertainty_qualified': False, 'dispatch_authorized': False, 'acceptance_run': False}
