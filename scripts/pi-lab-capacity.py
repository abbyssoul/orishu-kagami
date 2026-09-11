#!/usr/bin/env python3
"""Bounded 4-host / 16-worker Unix or off-host HTTPS capacity diagnostic.

No acceptance claim, automatic admission retry, host tuning or production fix.
Every attempted cell is retained; the original whole-run deadline never moves.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import time
import uuid

from pi_lab_experiment import Remote, exact
from pi_lab_node import require, root_path, write_new_json
from pi_lab_clock import exchange, plan_start, stamp, validate_start_policy
from pi_lab_environment import validate as validate_environment, review_series, compare
from pi_lab_measurement import probe_clock_bounds


def bundle():
    root = Path(__file__).parent
    code = 'import sys,types\n'
    for name in ('pi_lab_node', 'pi_lab_clock', 'pi_lab_environment', 'pi_lab_measurement', 'pi_lab_session'):
        source = (root / (name + '.py')).read_text()
        code += (f'module=types.ModuleType({name!r});sys.modules[{name!r}]=module\n'
                 f'exec(compile({source!r},{name!r},"exec"),module.__dict__)\n')
    source = (root / 'pi_lab_capacity_node.py').read_text()
    code += f'exec(compile({source!r},"pi_lab_capacity_node.py","exec"))\n'
    raw = code.encode()
    require(len(raw) <= 131072, 'capacity bundle cap')
    return raw


class CapacityRemote(Remote):
    def __init__(self, node, run_id, deadline):
        super().__init__(node, run_id, 'compiled_off', deadline, bundle_factory=bundle)

    def _call(self, operation, **arguments):
        if operation == 'worker':
            require(set(arguments) == {'payload'}, 'worker RPC envelope')
            arguments = arguments['payload']
        self.write({'operation': operation, 'arguments': arguments})
        reply = self.read(45 if operation == 'measure-capacity' else 30)
        if isinstance(reply, dict) and reply.get('ok') is False:
            self.last_node_error = {key: reply.get(key) for key in ('error_type', 'stage')}
        require(isinstance(reply, dict) and reply.get('ok') is True and 'result' in reply,
                'capacity RPC failed: ' + operation)
        return reply['result']

    def worker(self, slot, operation, **arguments):
        return self.call('worker', payload={'slot': slot, 'operation': operation, 'arguments': arguments})


def read_inventory(path):
    spec = importlib.util.spec_from_file_location('pi_lab_cli', Path(__file__).with_name('pi-lab.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    value = module.inventory(path)
    require(len(value['nodes']) == 4, 'capacity requires exactly four physical hosts')
    return value


def review_load(value, expected, *, network=False):
    """Identity/accounting before QPS. No averaging percentiles across workers."""
    require(isinstance(value, dict) and set(value) == {'schema_version', 'kind', 'global_workers',
            'first_role', 'clients_per_worker', 'rate_per_worker', 'seconds', 'workers',
            'start', 'end_marker', 'end_marker_elapsed_ns'}, 'capacity load fields')
    require(value['schema_version'] == (2 if network else 1)
            and value['kind'] == ('pi-network-capacity-load' if network else 'pi-capacity-load')
            and value['global_workers'] == 16 and value['seconds'] == 10, 'capacity load version/window')
    for key in ('first_role', 'clients_per_worker', 'rate_per_worker'):
        require(value[key] == expected[key], 'capacity profile mismatch')
    probe_clock_bounds({'window_ns': 10_000_000_000, 'start': value['start'],
                        'end_marker': value['end_marker'], 'end_marker_elapsed_ns': value['end_marker_elapsed_ns']})
    require(isinstance(value['workers'], list) and len(value['workers']) == 4, 'capacity rows')
    rows = []
    for slot, row in enumerate(value['workers']):
        require(set(row) == {'role', 'latency', 'scheduling_delay', 'scheduled_arrivals', 'skipped_arrivals',
                            'tail_requests', 'transport_errors', 'invalid_responses', 'capped'}
                and row['role'] == expected['first_role'] + slot and type(row['capped']) is bool, 'capacity row identity')
        for key in ('skipped_arrivals', 'tail_requests', 'transport_errors', 'invalid_responses'):
            require(type(row[key]) is int and 0 <= row[key] <= 1_000_000, 'capacity accounting bound')
        for key in ('latency', 'scheduling_delay'):
            item = row[key]
            require(isinstance(item, dict) and set(item) == {'requests', 'median_us', 'p95_us'}
                    and type(item['requests']) is int and 0 <= item['requests'] <= 1_000_000, 'latency counts')
            for field in ('median_us', 'p95_us'):
                require(item[field] is None if item['requests'] == 0 else
                        type(item[field]) in (int, float) and math.isfinite(item[field])
                        and 0 <= item[field] <= 11_000_000, 'latency finite bounds')
        count = row['latency']['requests']
        rate = expected['rate_per_worker']
        if rate is not None:
            require(row['scheduled_arrivals'] == rate * 10 and count + row['skipped_arrivals']
                    + row['tail_requests'] + row['transport_errors'] + row['invalid_responses'] == rate * 10
                    and row['scheduling_delay']['requests'] == count, 'offered arrival accounting')
        else:
            require(row['scheduled_arrivals'] is None and row['skipped_arrivals'] == 0
                    and row['scheduling_delay']['requests'] == 0, 'unpaced accounting')
        rows.append({'role': row['role'], 'qps': count / 10,
                     'delivery': count / (rate * 10) if rate else None, **row})
    return rows


def record_start_plan(clocks, run_id, policy, path):
    """Preserve the actual evidence even when the unchanged timing gate refuses."""
    now = stamp()
    write_new_json(path, {'clocks': clocks, 'run_id': run_id, 'now': now, 'policy': policy})
    return plan_start(clocks, run_id, now, policy)


def execute_curve(cell, baseline, *, short_comparison=False):
    """Select windows only; the same measured cell retains every safety gate."""
    if short_comparison:
        for _ in range(2):
            for rate, clients in ((baseline, 32), (None, 32), (None, 64)):
                cell(rate, clients)
        return
    baseline_row = cell(baseline, 32)
    if baseline_row['delivery_met']:
        for multiplier in (2, 4, 8):
            if baseline * multiplier > 100000:
                break
            if not cell(baseline * multiplier, 32)['delivery_met']:
                break
    else:
        for divisor in (2, 4, 8):
            if cell(max(100, baseline // divisor), 32)['delivery_met']:
                break
    unpaced = [cell(None, clients) for clients in (2, 8, 32)]
    if unpaced[-1]['aggregate_qps'] > unpaced[-2]['aggregate_qps'] * 1.05:
        unpaced.append(cell(None, 64))
    best = max(unpaced, key=lambda row: row['aggregate_qps'])
    cell(None, best['clients_per_worker'])


def run(inventory, output, digest, baseline, policy, max_elapsed_seconds=600, *, network=None,
        short_comparison=False):
    require(type(baseline) is int and 100 <= baseline <= 100000, 'baseline rate bound')
    require(type(short_comparison) is bool and (not short_comparison or network is not None),
            'short comparison requires off-host network profile')
    require(type(max_elapsed_seconds) is int and 180 <= max_elapsed_seconds <= 600, 'whole experiment bound')
    require(isinstance(inventory, dict) and len(inventory['nodes']) == 4, 'four physical hosts required')
    require(isinstance(policy, dict) and 'start' in policy, 'explicit start policy required')
    validate_start_policy(policy['start'])
    import re
    require(re.fullmatch('[0-9a-f]{64}', digest), 'probe digest')
    output = root_path(output)
    output.mkdir(mode=0o700, exist_ok=False)
    if network is not None:
        network.output = output
    began, run_id, sessions = time.monotonic(), 'q-' + uuid.uuid4().hex[:10], []
    deadline = began + max_elapsed_seconds - 90  # Cleanup reserve; never reset per cell.
    result = {'schema_version': 1, 'kind': 'pi-capacity-diagnostic', 'run_id': run_id,
              'status': 'incomplete', 'acceptance_run': False, 'cells': [], 'events': [],
              'baseline_per_worker': baseline, 'max_elapsed_seconds': max_elapsed_seconds,
              'bundle_sha256': hashlib.sha256(bundle()).hexdigest(), 'probe_sha256': digest,
              'source_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'policy': policy, 'cleanup': []}
    result['transport'] = 'authenticated-https-off-host' if network else 'colocated-unix'
    result['curve'] = 'three-profiles-twice' if short_comparison else 'adaptive-capacity'
    result['ssh_selection'] = ('peer-address-with-hostname-key' if network and network.direct_ip
                               else 'inventory-hostname')
    if network:
        from pi_lab_network import bundle as network_bundle
        result['bundle_sha256'] = hashlib.sha256(network_bundle()).hexdigest()
    write_new_json(output / 'inventory.json', inventory)

    def save():
        # Each checkpoint is create-new, including failures. No existing evidence is overwritten.
        write_new_json(output / f'checkpoint-{len(result["events"]):03d}-{len(result["cells"]):02d}.json', result)

    def event(stage):
        result['stage'] = stage
        result['events'].append({'stage': stage, 'elapsed_seconds': time.monotonic() - began})
        print(json.dumps(result['events'][-1]), flush=True)

    def rpc(role, operation, **args):
        return sessions[role // 4].worker(role % 4, operation, **args)

    def poll(read, predicate):
        while time.monotonic() < deadline:
            value = read()
            if predicate(value):
                return value
            time.sleep(.2)
        raise TimeoutError('whole capacity deadline')

    def all_views():
        def host_views(host):
            return [sessions[host].worker(slot, 'info') for slot in range(4)]
        with ThreadPoolExecutor(max_workers=4) as pool:
            return [view for group in pool.map(host_views, range(4)) for view in group]

    def verify():
        views = all_views()
        lists = [rpc(role, 'members') for role in range(16)]
        require(all(isinstance(row, list) and len(row) <= 16 for row in lists), 'capacity membership count')
        return exact(views, lists, formation, assigned, pins)

    def safe_environment(value, host):
        validate_environment(value, run_id, inventory['nodes'][host]['interface'])
        require(value['temperature_millicelsius'] is not None and value['temperature_millicelsius'] < 75000
                and value['firmware_throttled'] == 0, 'thermal/firmware safety stop')

    def cell(rate, clients):
        require(len(result['cells']) < 12 and time.monotonic() + 60 < deadline, 'capacity cell/time bound')
        number = len(result['cells'])
        row = {'cell': number, 'rate_per_worker': rate, 'clients_per_worker': clients,
               'status': 'incomplete', 'hosts': [], 'dispatch_attempted': False}
        result['cells'].append(row)
        event(f'cell-{number}-prepare-rate-{rate}-clients-{clients}')
        save()
        require(verify(), 'membership changed before cell')
        for host, session in enumerate(sessions):
            result['stage'] = f'cell-{number}-host-{host}-warmup'
            before = session.call('environment')
            safe_environment(before, host)
            args = {'cell': number, 'first_role': host * 4, 'rate_per_worker': rate,
                    'clients_per_worker': clients, 'formation': formation,
                    'nodes': assigned[host * 4:host * 4 + 4], 'probe_sha256': digest}
            prepared = session.call('prepare-capacity', **args)
            require(prepared['prepared'] is True and prepared['cell'] == number
                    and prepared['probe_sha256'] == digest, 'capacity preparation identity')
            expected = {'schema_version': 1, 'first_role': host * 4, 'rate_per_worker': rate,
                        'clients_per_worker': clients, 'targets': [
                            {'socket': str(Path(session.node['root']) / 'runs' / f'{run_id}-{slot}' / 'api.sock'),
                             'formation': formation, 'node': assigned[host * 4 + slot]} for slot in range(4)]}
            if network:
                from pi_lab_network import endpoint
                expected['schema_version'] = 2
                expected['targets'] = [{'endpoint': endpoint(session.node['peer_address'], slot),
                                        'formation': formation, 'node': assigned[host * 4 + slot]} for slot in range(4)]
            require(prepared['config'] == expected, 'capacity target mismatch')
            row['hosts'].append({'host': host, 'before': before, 'prepared': prepared})
        result['stage'] = f'cell-{number}-clock-planning'
        clocks = [{'role': host, 'exchanges': [exchange(session.call, run_id) for _ in range(3)]}
                  for host, session in enumerate(sessions)]
        row['clocks'] = clocks
        proposal = record_start_plan(clocks, run_id, policy['start'], output / f'cell-{number}-clock-evidence.json')
        row.update(clocks=clocks, start_proposal=proposal, dispatch_attempted=True)
        result['stage'] = f'cell-{number}-measurement'
        def measure(host):
            if network:
                sessions[host].local_start_ns = proposal['reference_start_unix_ns']
            return sessions[host].call('measure-capacity', start_unix_ns=proposal['roles'][host]['start_unix_ns'])
        errors = []
        with ThreadPoolExecutor(max_workers=4) as pool:
            futures = {pool.submit(measure, host): host for host in range(4)}
            for future in as_completed(futures):
                host = futures[future]
                try:
                    report = future.result()
                    write_new_json(output / f'cell-{number}-host-{host}.json', report)
                    expected = row['hosts'][host]['prepared']['config']
                    require(report['kind'] == ('pi-network-capacity-measurement' if network else 'pi-capacity-measurement')
                            and report['schema_version'] == (2 if network else 1)
                            and report['run_id'] == run_id and report['cell'] == number
                            and report['config'] == expected, 'measurement identity')
                    reviewed = review_load(report['load'], expected, network=bool(network))
                    require(len(report['resources']) == (5 if network else 6), 'process resource count')
                    row['hosts'][host].update(report=report, reviewed=reviewed)
                except Exception as error:
                    row['hosts'][host]['error_type'] = type(error).__name__
                    errors.append(host)
        require(not errors, 'capacity cell incomplete')
        result['stage'] = f'cell-{number}-post-window'
        for host, session in enumerate(sessions):
            after = session.call('environment')
            row['hosts'][host]['after'] = after
            safe_environment(after, host)
            report = row['hosts'][host]['report']
            row['hosts'][host]['environment_review'] = review_series(report['environment_series'], run_id,
                    session.node['interface'], report['control_bracket_ns'])
            row['hosts'][host]['environment_comparison'] = compare(row['hosts'][host]['before'], after,
                    run_id, session.node['interface'])
            row['hosts'][host]['clock_after'] = exchange(session.call, run_id)
        # These are conditional nominal-window bounds, not proof of every
        # client's continuous activity. Retain all clock exchanges, no trimming.
        intervals = []
        for host, observed in enumerate(row['hosts']):
            proposal_role = proposal['roles'][host]
            low, high = proposal_role['offset_bracket_ns']
            after_low, after_high = observed['clock_after']['offset_bracket_ns']
            margin = 20_000_000  # Explicit drift/clock-change allowance; never silently widened.
            require(after_low >= low - margin and after_high <= high + margin, 'clock offset changed outside allowance')
            mark = observed['report']['load']['start']
            if network:
                # Pi-side sampling start, not the desktop probe's wall clock.
                mark = {'unix_ns': observed['report']['start_unix_ns'], 'read_bracket_ns': 0}
            intervals.append([mark['unix_ns'] - high - margin,
                              mark['unix_ns'] + mark['read_bracket_ns'] - low + margin])
        skew = max(i[1] for i in intervals) - min(i[0] for i in intervals)
        row['timing'] = {'conditional_start_skew_upper_ns': skew,
                         'conditional_nominal_common_window_lower_ns': max(0, 10_000_000_000 - skew),
                         'offset_change_allowance_ns': 20_000_000, 'clock_uncertainty_qualified': False}
        if network:
            starts = [host['report']['load']['start'] for host in row['hosts']]
            generator_skew = max(s['unix_ns'] + s['read_bracket_ns'] for s in starts) - min(s['unix_ns'] for s in starts)
            joint = intervals + [[s['unix_ns'], s['unix_ns'] + s['read_bracket_ns']] for s in starts]
            joint_skew = max(i[1] for i in joint) - min(i[0] for i in joint)
            row['timing'].update(generator_start_skew_ns=generator_skew,
                                 conditional_generator_sampler_skew_upper_ns=joint_skew,
                                 conditional_joint_common_window_lower_ns=max(0, 10_000_000_000 - joint_skew))
            require(joint_skew < 100_000_000, 'capacity windows insufficiently aligned')
        require(skew < 100_000_000, 'capacity windows insufficiently aligned')
        require(verify(), 'membership changed after cell')
        all_rows = [worker for host in row['hosts'] for worker in host['reviewed']]
        row['aggregate_qps'] = sum(worker['qps'] for worker in all_rows)
        row['delivery_met'] = rate is not None and all(worker['delivery'] >= .99 for worker in all_rows)
        row['load_failed'] = any(worker['transport_errors'] or worker['invalid_responses'] or worker['capped'] for worker in all_rows)
        row['sampler_findings'] = [host['host'] for host in row['hosts']
                                   if host['report']['missed_samples'] or host['environment_review']['missed_samples']]
        row['status'] = 'complete_with_findings' if (row['load_failed'] or row['sampler_findings']
                                                   or rate and not row['delivery_met']) else 'complete_unqualified'
        event(f'cell-{number}-qps-{row["aggregate_qps"]:.1f}-delivery-{row["delivery_met"]}')
        save()
        require(not row['load_failed'], 'request/correctness/sample-cap failure; stop escalation')
        for host in row['hosts']:
            resources = host['report']['resources']
            require(all(p['swap_peak_kib'] == 0 for p in resources), 'swap safety stop')
        return row

    try:
        for node in inventory['nodes']:
            sessions.append(network.remote(node, run_id, deadline) if network else CapacityRemote(node, run_id, deadline))
            require(sessions[-1].ready['artifacts'] == sessions[0].ready['artifacts'], 'host artifacts differ')
        result['artifacts'] = sessions[0].ready['artifacts']
        event('prepared-four-hosts')
        for role in range(16):
            rpc(role, 'start')
        initial = all_views()
        require(len({v['formationId'] for v in initial}) == 16, 'standalone identities')
        pins = [rpc(role, 'members')[0]['certFingerprint'] for role in range(16)]
        require(len(set(pins)) == 16, 'certificate uniqueness')
        formation, assigned = initial[0]['formationId'], [initial[0]['sourceNodeId']]
        for role in range(1, 16):
            # One chain crosses all four physical hosts; local groups remain
            # members of the same formation, not four independent clusters.
            issuer = role - 1
            material = rpc(issuer, 'material')
            require(material['formationId'] == formation and material['introducerNodeId'] == assigned[issuer]
                    and material['introducerFingerprint'] == pins[issuer] and material['introducerReady'], 'join issuer identity')
            operation = 'capacity-admit-' + str(role)
            rpc(role, 'join', formation=initial[role]['formationId'], operation_id=operation, material=material)
            del material
            def joined(receipt):
                require(receipt['sourceFormationId'] == initial[role]['formationId']
                        and receipt['sourceNodeId'] == initial[role]['sourceNodeId']
                        and receipt['targetFormationId'] == formation, 'join receipt identity')
                require(receipt['state']['phase'] not in ('failed', 'rejected', 'cancelled'), 'join terminal failure')
                return receipt['state']['phase'] == 'joined'
            receipt = poll(lambda: rpc(role, 'join-status', operation_id=operation), joined)
            assigned.append(receipt['state']['nodeId'])
            poll(lambda: rpc(role, 'info'), lambda v: v['introducerReady'])
        poll(verify, bool)
        result.update(formation=formation, assigned=assigned, fingerprints=pins)
        event('formed-sixteen-workers')
        execute_curve(cell, baseline, short_comparison=short_comparison)
        result['status'] = 'complete_with_findings'
    except (Exception, KeyboardInterrupt) as error:
        result['error_type'] = type(error).__name__
        # Only our fixed diagnostic assertions are eligible for an explanatory
        # label; never copy arbitrary CLI/SSH/JSON exception text into evidence.
        reasons = {'capacity cell incomplete', 'membership changed before cell', 'membership changed after cell',
                   'request/correctness/sample-cap failure; stop escalation', 'swap safety stop',
                   'sampler cannot sustain capacity profile', 'capacity windows insufficiently aligned',
                   'clock offset changed outside allowance', 'thermal/firmware safety stop',
                   'clock exchange exceeds proposed policy', 'capacity cell/time bound',
                   'join terminal failure', 'whole capacity deadline'}
        if str(error) in reasons:
            result['failure_reason'] = str(error)
        # Exception text can contain a hostile remote value; export only class.
    finally:
        result['node_failures'] = [getattr(session, 'last_node_error', None) for session in sessions]
        def stop(session):
            try:
                return session.stop()
            except Exception as error:
                return {'clean': False, 'error_type': type(error).__name__}
        with ThreadPoolExecutor(max_workers=4) as pool:
            result['cleanup'] = list(pool.map(stop, sessions))
        result['cleanup_clean'] = len(sessions) == 4 and all(row['clean'] for row in result['cleanup'])
        result['elapsed_seconds'] = time.monotonic() - began
        if not result['cleanup_clean'] or result['elapsed_seconds'] > max_elapsed_seconds:
            result['status'] = 'incomplete'
        write_new_json(output / 'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--probe-sha256', required=True)
    parser.add_argument('--baseline-per-worker', type=int, default=5000,
                        help='Offered summary requests/second for EACH worker (default: 5000; 80000 for sixteen).')
    parser.add_argument('--network-probe', help='Local native probe binary: authenticated HTTPS from coordinator instead of Pi-local Unix load.')
    parser.add_argument('--short-comparison', action='store_true',
                        help='Network only: baseline/32 clients, unpaced/32, unpaced/64, repeated twice; no adaptive sweep.')
    parser.add_argument('--ssh-via-peer-address', action='store_true',
                        help='Network lab only: SSH to the literal inventory IP while verifying its existing hostname host key.')
    parser.add_argument('--max-elapsed-seconds', type=int, default=600)
    parser.add_argument('--start-policy', required=True, help='Existing pilot policy supplies only start scheduling bounds.')
    args = parser.parse_args()
    if args.ssh_via_peer_address and not args.network_probe:
        parser.error('--ssh-via-peer-address requires --network-probe')
    from pi_lab_session import decode
    policy = decode(Path(args.start_policy).read_bytes())
    network = None
    if args.network_probe:
        from pi_lab_network import NetworkBackend
        network = NetworkBackend(args.network_probe, args.probe_sha256, direct_ip=args.ssh_via_peer_address)
    result = run(read_inventory(args.inventory), args.output, args.probe_sha256, args.baseline_per_worker,
                 policy, args.max_elapsed_seconds, network=network, short_comparison=args.short_comparison)
    print(json.dumps({key: result[key] for key in ('run_id', 'status', 'elapsed_seconds', 'cleanup_clean')}))
    return 0 if result['status'] == 'complete_with_findings' and result['cleanup_clean'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
