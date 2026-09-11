"""Coordinator for a bounded physical formation smoke, not acceptance timing."""
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import os
import re
from pathlib import Path
import selectors
import shlex
import signal
import subprocess
import time
import uuid

from pi_lab_node import require, root_path, write_new_json
from pi_lab_session import CAP, Drain, decode, line


def bundle():
    """Stream reviewed modules; no checkout or installed script is trusted."""
    root = Path(__file__).parent
    helper = (root / 'pi_lab_node.py').read_text()
    clock = (root / 'pi_lab_clock.py').read_text()
    environment = (root / 'pi_lab_environment.py').read_text()
    measurement = (root / 'pi_lab_measurement.py').read_text()
    session = (root / 'pi_lab_session.py').read_text()
    code = ('import sys,types\n'
            'module=types.ModuleType("pi_lab_node")\n'
            'sys.modules["pi_lab_node"]=module\n'
            f'exec(compile({helper!r},"pi_lab_node.py","exec"),module.__dict__)\n'
            'module=types.ModuleType("pi_lab_clock")\n'
            'sys.modules["pi_lab_clock"]=module\n'
            f'exec(compile({clock!r},"pi_lab_clock.py","exec"),module.__dict__)\n'
            'module=types.ModuleType("pi_lab_environment")\n'
            'sys.modules["pi_lab_environment"]=module\n'
            f'exec(compile({environment!r},"pi_lab_environment.py","exec"),module.__dict__)\n'
            'module=types.ModuleType("pi_lab_measurement")\n'
            'sys.modules["pi_lab_measurement"]=module\n'
            f'exec(compile({measurement!r},"pi_lab_measurement.py","exec"),module.__dict__)\n'
            f'exec(compile({session!r},"pi_lab_session.py","exec"))\n').encode()
    require(len(code) <= 131072, 'session bundle cap')
    return code


BOOTSTRAP = ('import os,sys\n'
             'remaining=int(sys.argv[1]); code=bytearray()\n'
             'while remaining:\n'
             ' chunk=os.read(0,min(remaining,4096))\n'
             ' if not chunk: raise EOFError("bootstrap truncated")\n'
             ' code.extend(chunk); remaining-=len(chunk)\n'
             'exec(compile(code,"pi-bootstrap","exec"))\n')


def send(stream, raw, deadline):
    require(len(raw) <= 131072, 'session input cap')
    os.set_blocking(stream.fileno(), False)
    sent = 0
    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_WRITE)
        while sent < len(raw):
            remaining = deadline - time.monotonic()
            require(remaining > 0 and selector.select(remaining), 'session input deadline')
            try:
                sent += os.write(stream.fileno(), raw[sent:sent + 4096])
            except BlockingIOError:
                pass


class Remote:
    def __init__(self, node, run_id, mode, deadline, *, bundle_factory=bundle, direct_ip=False):
        require(mode in ('omitted', 'compiled_off', 'metrics', 'clock_only'), 'remote session mode')
        self.node, self.deadline, self.child, self.stderr = node, deadline, None, None
        self.run_id, self.expected_measurement = run_id, None
        self.mode, self.prepared_probe_sha256, self.measurement_attempted = mode, None, False
        self.poisoned = False
        require(type(direct_ip) is bool, 'direct SSH selection')
        destination, trust = node['ssh_host'], []
        if direct_ip:
            import ipaddress
            destination = str(ipaddress.ip_address(node['peer_address']))
            # The user-provided hostname is the existing trust identity. Keep
            # strict checking; do not add a new host key or trust an IP blindly.
            trust = ['-o', 'HostKeyAlias=' + node['ssh_host']]
        code = bundle_factory()
        command = ['ssh', '-T', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
                   '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2',
                   '-o', 'ClearAllForwardings=yes', '-o', 'ForwardAgent=no', *trust, '--', destination,
                   shlex.join(['python3', '-u', '-c', BOOTSTRAP, str(len(code))])]
        self.child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, bufsize=0, start_new_session=True)
        self.stderr = Drain(self.child.stderr)
        try:
            send(self.child.stdin, code, min(deadline, time.monotonic() + 15))
            config = {} if mode == 'clock_only' else {key: node[key] for key in ('root', 'interface', 'peer_address', 'name')}
            if mode != 'clock_only' and 'experiment' in node:
                from pi_lab_session import experiment_options
                config['experiment'] = experiment_options(node['experiment'])
            self.write(config | {'run_id': run_id, 'mode': mode})
            self.ready = self.read(30)
            require(self.ready.get('ok') is True and self.ready.get('ready') is True
                    and self.ready.get('run_id') == run_id, 'remote initialization failed')
            if mode == 'clock_only':
                require(self.ready == {'ok': True, 'ready': True, 'run_id': run_id, 'clock_only': True},
                        'clock-only initialization receipt')
        except BaseException:
            self.close()
            raise

    def write(self, value):
        raw = json.dumps(value, allow_nan=False).encode() + b'\n'
        require(len(raw) <= CAP, 'RPC input cap')
        send(self.child.stdin, raw, min(self.deadline, time.monotonic() + 5))

    def read(self, seconds=10):
        return decode(line(self.child.stdout, min(self.deadline, time.monotonic() + seconds)))

    def call(self, operation, **arguments):
        require(not getattr(self, 'poisoned', False), 'session reply boundary is no longer trustworthy')
        try:
            return self._call(operation, **arguments)
        except BaseException:
            # A timeout may consume a partial line or leave a whole late reply
            # pending. Neither a fresh deadline nor a new RPC restores pairing.
            # Validation failures and interrupted writes also forbid reuse.
            self.poisoned = True
            raise

    def _call(self, operation, **arguments):
        require(getattr(self, 'mode', None) != 'clock_only' or operation == 'clock', 'clock-only RPC')
        if operation == 'measure':
            require(self.expected_measurement is not None, 'coordinator has no prepared measurement target')
            require(set(arguments) == {'start_unix_ns'} and type(arguments['start_unix_ns']) is int,
                    'coordinator measurement start')
            require(not self.measurement_attempted, 'measurement request cannot be retried')
            self.measurement_attempted = True  # Even an uncertain write consumes this attempt.
        self.write({'operation': operation, 'arguments': arguments})
        response = self.read(40 if operation == 'measure' else 25 if operation == 'stop'
                             else 20 if operation in ('start', 'prepare-load') else 12)
        require(isinstance(response, dict) and response.get('ok') is True and 'result' in response,
                'remote operation failed: ' + operation)
        if operation == 'prepare-load':
            require(response['result'] == {'prepared': True, 'probe_sha256': arguments['probe_sha256'],
                                           'workers': arguments['workers'], 'role': arguments['role']},
                    'prepared measurement receipt mismatch')
            self.prepared_probe_sha256 = arguments['probe_sha256']
            self.expected_measurement = {'schema_version': 2, 'workers': arguments['workers'], 'role': arguments['role'],
                                         'target': {'formation': arguments['formation'], 'node': arguments['node'],
                                                    'socket': str(Path(self.node['root']) / 'runs' / self.run_id / 'api.sock')}}
        if operation == 'measure':
            from pi_lab_results import review
            review(response['result'], self.expected_measurement)
            require(response['result']['scheduled_start_unix_ns'] == arguments['start_unix_ns'],
                    'measurement window request mismatch')
        if operation == 'stop-load':
            self.expected_measurement = None
        return response['result']

    def close(self):
        if self.child is None:
            return
        if not self.child.stdin.closed:
            self.child.stdin.close()  # EOF asks the node supervisor to clean up.
        try:
            self.child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(self.child.pid, signal.SIGTERM)
            try:
                self.child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.child.pid, signal.SIGKILL)
                self.child.wait(timeout=2)
        self.child.stdout.close()
        self.stderr.finish()

    def stop(self):
        try:
            if getattr(self, 'poisoned', False):
                # Close asks the supervisor to clean up via EOF. Its independent
                # watchdog remains the fallback; without a trusted receipt we
                # cannot report worker cleanup as verified.
                return {'clean': False, 'reason': 'session_reply_unverified'}
            # Cleanup has a separate, bounded allowance, never more setup time.
            self.deadline = time.monotonic() + 25
            return self.call('stop')
        finally:
            self.close()


def clock_check(inventory, output, remote_factory=Remote, start_policy=None):
    """Three raw exchanges per node, no worker/root preparation or load."""
    from pi_lab_clock import exchange, plan_start, stamp, validate_start_policy
    if start_policy is not None:
        validate_start_policy(start_policy)
    nodes = inventory['nodes']
    require(isinstance(nodes, list) and 3 <= len(nodes) <= 5, 'clock-check cohort bound')
    output = root_path(output)
    output.mkdir(mode=0o700, exist_ok=False)
    began, run_id = time.monotonic(), 'clock-' + uuid.uuid4().hex[:12]
    deadline = began + 30
    result = {'schema_version': 1, 'kind': 'pi-clock-check', 'run_id': run_id,
              'status': 'incomplete', 'workers_started': False, 'acceptance_run': False,
              'clock_uncertainty_qualified': False, 'session_bundle_sha256': hashlib.sha256(bundle()).hexdigest(),
              'coordinator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'nodes': [{'role': role, 'status': 'not_collected'} for role in range(len(nodes))]}
    if start_policy is not None:
        result['proposed_start_policy'] = dict(start_policy)
    write_new_json(output / 'inventory.json', inventory)
    def collect(role):
        remote = None
        row = {'role': role, 'status': 'incomplete', 'exchanges': []}
        try:
            remote = remote_factory(nodes[role], run_id, 'clock_only', deadline)
            for _ in range(3):
                row['exchanges'].append(exchange(remote.call, run_id))
            row['status'] = 'complete_unqualified'
        except Exception as error:
            row['error_type'] = type(error).__name__
        finally:
            if remote is not None:
                try:
                    remote.close()  # Normal EOF; clock-only sessions have no stop RPC.
                    row['ssh_exit'] = remote.child.returncode
                    if row['ssh_exit'] != 0:
                        row['status'] = 'incomplete'
                except Exception as error:
                    row.update(status='incomplete', close_error_type=type(error).__name__)
        return row
    try:
        with ThreadPoolExecutor(max_workers=len(nodes)) as pool:
            futures = {pool.submit(collect, role): role for role in range(len(nodes))}
            for future in as_completed(futures):
                row = future.result()
                result['nodes'][row['role']] = row
                write_new_json(output / f"node-{row['role']}.json", row)
        if all(row['status'] == 'complete_unqualified' for row in result['nodes']):
            result['status'] = 'complete_unqualified'
            if start_policy is not None:
                try:
                    result['start_proposal'] = plan_start(
                        [{'role': row['role'], 'exchanges': row['exchanges']} for row in result['nodes']],
                        run_id, stamp(), start_policy)
                except Exception as error:
                    # Successful clock collection remains evidence even when
                    # the proposed scheduling policy rejects it. Never retry.
                    result['start_proposal_error_type'] = type(error).__name__
    except (Exception, KeyboardInterrupt) as error:
        result['error_type'] = type(error).__name__
    finally:
        result['elapsed_seconds'] = time.monotonic() - began
        write_new_json(output / 'clock-check.json', result)
    return result


def collect_window(sessions, starts_unix_ns, output, *, timing_policy=None):
    """Collect one prepared 3–5-node window concurrently, with no retries.

    Internal seam, not an unqualified public pilot command: the caller must
    supply reviewed clock-derived starts, own the experiment budget and stop
    every session in its finally block. This function never extends session
    deadlines, forms workers or chooses clock policy. An explicit timing_policy
    adds a conditional actual-window review, never approval or M4 acceptance.
    """
    from pi_lab_results import configuration, integer, review, overlap_policy, review_overlap
    if timing_policy is not None:
        overlap_policy(timing_policy)  # Reject malformed limits before IO/load.
    require(isinstance(sessions, list) and 3 <= len(sessions) <= 5
            and isinstance(starts_unix_ns, list) and len(starts_unix_ns) == len(sessions), 'window cohort size')
    require(len({id(session) for session in sessions}) == len(sessions), 'duplicate session')
    run_id, mode = sessions[0].run_id, sessions[0].mode
    require(isinstance(run_id, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,95}', run_id)
            and mode in ('omitted', 'compiled_off'), 'window run/mode')
    binary = 'orishu-worker-omitted' if mode == 'omitted' else 'orishu-worker'
    expected, artifacts, probes, hosts, peers = [], [], [], [], []
    for role, session in enumerate(sessions):
        config = session.expected_measurement
        configuration(config)
        require(session.run_id == run_id and session.mode == mode
                and not session.measurement_attempted and config['role'] == role
                and config['workers'] == len(sessions), 'window session/run/role mismatch')
        require(config['target']['socket'] == str(Path(session.node['root']) / 'runs' / run_id / 'api.sock'),
                'window target is not the session socket')
        hashes = session.ready['artifacts']
        require(isinstance(hashes, dict) and set(hashes) == {binary, 'orishuctl'}
                and all(isinstance(value, str) and re.fullmatch(r'[0-9a-f]{64}', value)
                        for value in hashes.values()), 'window worker/CLI artifact receipts')
        require(isinstance(session.prepared_probe_sha256, str)
                and re.fullmatch(r'[0-9a-f]{64}', session.prepared_probe_sha256), 'prepared probe digest')
        require(all(isinstance(session.node[key], str) and 0 < len(session.node[key]) <= 253
                    for key in ('ssh_host', 'peer_address')), 'window host identity fields')
        require(isinstance(session.node.get('interface'), str)
                and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', session.node['interface']), 'window network interface')
        integer(starts_unix_ns[role])
        expected.append(config)
        artifacts.append(hashes)
        probes.append(session.prepared_probe_sha256)
        hosts.append(session.node['ssh_host'])
        peers.append(session.node['peer_address'])
    require(len(set(hosts)) == len(set(peers)) == len(sessions), 'window duplicate host/address')
    require(len({config['target']['formation'] for config in expected}) == 1
            and len({config['target']['node'] for config in expected}) == len(sessions), 'window membership identity')
    require(all(hashes == artifacts[0] for hashes in artifacts) and len(set(probes)) == 1,
            'window artifact mismatch')
    output = root_path(output)
    output.mkdir(mode=0o700, exist_ok=False)
    began = time.monotonic()
    measurements = [None] * len(sessions)
    result = {'schema_version': 1, 'kind': 'pi-window-collection', 'run_id': run_id, 'mode': mode,
              'status': 'incomplete', 'clock_uncertainty_qualified': False, 'acceptance_run': False,
              'cleanup_owner': 'caller', 'cleanup_verified': False,
              'nodes': [{'role': role, 'status': 'not_collected'} for role in range(len(sessions))]}
    write_new_json(output / 'manifest.json', {
        'schema_version': 1, 'kind': 'pi-window-manifest', 'run_id': run_id, 'mode': mode,
        'expected': expected, 'starts_unix_ns': starts_unix_ns, 'artifacts': artifacts[0],
        'probe_sha256': probes[0], 'hosts': hosts, 'peer_addresses': peers,
        'session_bundle_sha256': hashlib.sha256(bundle()).hexdigest(),
        'coordinator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'reader_sha256': hashlib.sha256(Path(__file__).with_name('pi_lab_results.py').read_bytes()).hexdigest(),
        'timing_policy': timing_policy,
        'acceptance_run': False})

    def collect(role):
        from pi_lab_clock import exchange
        from pi_lab_environment import validate, compare
        session = sessions[role]
        receipt = {'role': role, 'status': 'invalid_or_unavailable'}
        try:
            # Remote's write/read deadlines already bound each RPC. Tighten to
            # this one collection ceiling without resetting the setup budget.
            session.deadline = min(session.deadline, began + 45)
            require(session.deadline > time.monotonic(), 'window collection deadline')
            environment_before = session.call('environment')
            validate(environment_before, run_id, session.node['interface'])
            receipt['environment_before'] = environment_before
            receipt['clock_before'] = exchange(session.call, run_id)
            value = session.call('measure', start_unix_ns=starts_unix_ns[role])
            checked = review(value, expected[role])
            require(value['environment_series']['interface'] == session.node['interface'],
                    'periodic sample interface differs from inventory')
            require(value['scheduled_start_unix_ns'] == starts_unix_ns[role], 'window request mismatch')
            path = output / f'node-{role}.json'
            write_new_json(path, value)
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            receipt.update(file=path.name, sha256=digest, review=checked)
            measurements[role] = value
            receipt['clock_after'] = exchange(session.call, run_id)
            environment_after = session.call('environment')
            receipt['environment'] = compare(environment_before, environment_after, run_id, session.node['interface'])
            receipt['status'] = 'valid_unqualified'
        except Exception as error:
            receipt['error_type'] = type(error).__name__
        return receipt

    try:
        # One in-flight RPC per already prepared node; never serial ten-second
        # windows. Completed roles are saved even if another role disconnects.
        with ThreadPoolExecutor(max_workers=len(sessions)) as pool:
            futures = {pool.submit(collect, role): role for role in range(len(sessions))}
            for future in as_completed(futures):
                receipt = future.result()
                write_new_json(output / f"receipt-{receipt['role']}.json", receipt)
                result['nodes'][receipt['role']] = receipt
        if all(row['status'] == 'valid_unqualified' for row in result['nodes']):
            result['status'] = 'complete_unqualified'
            if timing_policy is not None:
                result['timing_review'] = review_overlap([
                    {'role': row['role'], 'measurement': measurements[row['role']],
                     'clock_before': row['clock_before'], 'clock_after': row['clock_after']}
                    for row in result['nodes']], expected, run_id, timing_policy)
    except (Exception, KeyboardInterrupt) as error:
        result['error_type'] = type(error).__name__
    finally:
        if timing_policy is not None and 'timing_review' not in result:
            result['timing_review'] = {'status': 'invalid_or_unavailable', 'acceptance_run': False}
        result['nodes'].sort(key=lambda row: row['role'])
        result['elapsed_seconds'] = time.monotonic() - began
        write_new_json(output / 'collection.json', result)
    return result


def summary(view):
    return {key: view[key] for key in ('formationId', 'sourceNodeId', 'nodes', 'alive',
                                      'locked', 'introducerReady', 'participation')}


def members(value):
    require(isinstance(value, list) and len(value) <= 6, 'membership list bound including one retired identity')
    return [{key: node[key] for key in ('nodeId', 'certFingerprint', 'liveness')} for node in value]


def exact(views, lists, formation, assigned, pins, locked=False, retired=None):
    expected = {identity: (pin, 'alive') for identity, pin in zip(assigned, pins)}
    require(len(expected) == len(assigned) == len(pins), 'duplicate assigned identity')
    for identity, pin in (retired or {}).items():
        require(identity not in expected, 'retired identity reused')
        expected[identity] = (pin, 'dead')
    require(len(views) == len(lists) == len(assigned), 'observer count')
    for index, view in enumerate(views):
        require(view['formationId'] == formation and view['sourceNodeId'] == assigned[index], 'formation/source mismatch')
    return all(view['nodes'] == len(expected) and view['alive'] == len(assigned) and view['locked'] is locked
               and view['introducerReady'] is True for view in views) and all(
        len(nodes) == len(expected) and {n['nodeId']: (n['certFingerprint'], n['liveness']) for n in nodes} == expected
        for nodes in lists)


def smoke(inventory, output, mode='compiled_off', remote_factory=Remote, probe_sha256=None):
    """One 120s setup/recovery deadline; no mutation resubmission or timed load."""
    return _formation_run(inventory, output, mode, remote_factory, probe_sha256)


def validate_pilot_policy(policy):
    """An explicit execution policy, not approval inferred from a saved file."""
    from pi_lab_clock import validate_start_policy
    from pi_lab_results import overlap_policy
    require(isinstance(policy, dict) and set(policy) == {'schema_version', 'max_elapsed_seconds', 'start', 'overlap'}
            and type(policy['schema_version']) is int and policy['schema_version'] == 1,
            'pilot policy fields/version')
    require(type(policy['max_elapsed_seconds']) is int and policy['max_elapsed_seconds'] == 300,
            'pilot requires a separate five-minute allowance')
    validate_start_policy(policy['start'])
    overlap_policy(policy['overlap'])


def pilot(inventory, output, policy, probe_sha256, mode='compiled_off', remote_factory=Remote):
    """Execute ONE explicitly requested physical pilot, never a retry/matrix.

    The caller owns operator approval of the separate five-minute allowance
    and clock assumptions. The public CLI validates the inventory first.
    Builds, package installation and host policy changes are not performed.
    """
    validate_pilot_policy(policy)
    require(mode in ('omitted', 'compiled_off') and isinstance(probe_sha256, str)
            and re.fullmatch(r'[0-9a-f]{64}', probe_sha256), 'pilot mode/probe digest')
    require(isinstance(inventory, dict) and isinstance(inventory.get('nodes'), list)
            and 3 <= len(inventory['nodes']) <= 5, 'pilot cohort bound')
    return _formation_run(inventory, output, mode, remote_factory, probe_sha256, pilot_policy=policy)


def _formation_run(inventory, output, mode, remote_factory, probe_sha256, *, pilot_policy=None):
    """Shared formation/cleanup ownership for smoke and explicit pilot only."""
    output = root_path(output)
    output.mkdir(mode=0o700, exist_ok=False)
    run_id = 'p-' + uuid.uuid4().hex[:12]
    began, sessions = time.monotonic(), []
    deadline = began + 120
    result = {'schema_version': 1, 'kind': 'pi-formation-smoke', 'run_id': run_id,
              'status': 'incomplete', 'mode': mode, 'acceptance_run': False,
              'setup_recovery_budget_seconds': 120, 'node_watchdog_seconds': 180,
              'session_bundle_sha256': hashlib.sha256(bundle()).hexdigest(), 'events': [], 'cleanup': []}
    result['bootstrap_sha256'] = hashlib.sha256(BOOTSTRAP.encode()).hexdigest()
    result['coordinator_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    if pilot_policy is not None:
        result.update(kind='pi-formation-pilot', policy=pilot_policy, timed_window_started=False,
                      measurement_dispatch_attempts=0, setup_measurement_budget_seconds=120,
                      clock_uncertainty_qualified=False)
        del result['setup_recovery_budget_seconds']
    write_new_json(output / 'inventory.json', inventory)

    def record(stage, **values):
        require(len(result['events']) < 256, 'event count cap')
        result['events'].append({'stage': stage, 'elapsed_seconds': time.monotonic() - began, **values})

    def poll(stage, read, predicate):
        result['stage'] = stage
        checks = 0
        while time.monotonic() < deadline:
            value = read()
            checks += 1
            if predicate(value) and time.monotonic() < deadline:
                record(stage, checks=checks)
                return value
            time.sleep(0.2)
        raise TimeoutError('whole setup/recovery deadline')

    def views(indices):
        return [summary(sessions[i].call('info')) for i in indices]

    def verify(stage, indices, formation, assigned, pins, locked=False, retired=None):
        def read():
            observed = views(indices)
            lists = [members(sessions[i].call('members')) for i in indices]
            result['last_views'], result['last_members'] = observed, lists
            return observed, lists
        poll(stage, read, lambda value: exact(*value, formation, assigned, pins, locked, retired))

    def join(index, issuer, formation, initial, issuer_id, issuer_pin, operation):
        material = sessions[issuer].call('material')
        require(material['introducerReady'] and material['formationId'] == formation
                and material['introducerNodeId'] == issuer_id
                and material['introducerFingerprint'] == issuer_pin, 'introducer identity mismatch')
        sessions[index].call('join', formation=initial['formationId'], operation_id=operation, material=material)
        # The token exists only inside authenticated SSH/RPC memory and the
        # node's transient 0600 file. Never put raw replies into the evidence.
        del material
        def accepted(value):
            require(value['sourceFormationId'] == initial['formationId']
                    and value['sourceNodeId'] == initial['sourceNodeId']
                    and value['targetFormationId'] == formation, 'join receipt identity')
            phase = value['state']['phase']
            require(phase not in ('rejected', 'failed', 'cancelled'), 'join terminal failure')
            return phase == 'joined'
        receipt = poll('join-' + str(index), lambda: sessions[index].call('join-status', operation_id=operation), accepted)
        assigned = receipt['state']['nodeId']
        require(assigned != initial['sourceNodeId'], 'join retained standalone node identity')
        poll('introducer-ready-' + str(index), lambda: sessions[index].call('info'),
             lambda value: value['introducerReady'] is True)
        return assigned

    try:
        require(mode in ('omitted', 'compiled_off', 'metrics'), 'unsupported smoke mode')
        if probe_sha256 is not None:
            require(mode in ('omitted', 'compiled_off') and isinstance(probe_sha256, str)
                    and re.fullmatch(r'[0-9a-f]{64}', probe_sha256), 'probe preflight mode/digest')
            result['probe_sha256' if pilot_policy is not None else 'probe_preflight_sha256'] = probe_sha256
        nodes = inventory['nodes']
        require(3 <= len(nodes) <= 5, 'physical node count outside 3–5')
        for node in nodes:
            result['stage'] = 'session-' + node['name']
            session = remote_factory(node, run_id, mode, deadline)
            sessions.append(session)
            write_new_json(output / (node['name'] + '-readiness.json'), session.ready)
            if pilot_policy is not None:
                binary = 'orishu-worker-omitted' if mode == 'omitted' else 'orishu-worker'
                hashes = session.ready['artifacts']
                require(isinstance(hashes, dict) and set(hashes) == {binary, 'orishuctl'}
                        and all(isinstance(value, str) and re.fullmatch(r'[0-9a-f]{64}', value) for value in hashes.values())
                        and hashes == sessions[0].ready['artifacts'], 'pilot worker/CLI artifacts differ')
            session.call('start')
        initial = views(range(len(nodes)))
        require(len({v['formationId'] for v in initial}) == len(nodes), 'standalone formations not distinct')
        pins = []
        for index, view in enumerate(initial):
            listed = members(sessions[index].call('members'))
            require(len(listed) == 1 and listed[0]['nodeId'] == view['sourceNodeId'], 'standalone membership mismatch')
            pins.append(listed[0]['certFingerprint'])
        require(len(set(pins)) == len(nodes), 'duplicate worker certificate')
        formation, assigned = initial[0]['formationId'], [initial[0]['sourceNodeId']]
        for index in range(1, len(nodes)):
            assigned.append(join(index, index - 1, formation, initial[index], assigned[-1], pins[index - 1], 'admit-' + str(index)))
        verify('formed', range(len(nodes)), formation, assigned, pins)
        record('identities', formation=formation, assigned=list(assigned), fingerprints=pins)
        if pilot_policy is not None:
            from pi_lab_clock import exchange, plan_start, stamp
            for role, session in enumerate(sessions):
                result['stage'] = 'probe-warmup-' + str(role)
                ready = session.call('prepare-load', formation=formation, node=assigned[role],
                                     workers=len(nodes), role=role, probe_sha256=probe_sha256)
                require(ready == {'prepared': True, 'probe_sha256': probe_sha256,
                                  'workers': len(nodes), 'role': role}, 'pilot probe preparation receipt')
                record('probe-warmup-' + str(role), prepared=ready)
            # All probes remain READY. Collect fresh clocks AFTER every warmup;
            # never reuse clock-only/preflight receipts from an earlier run.
            result['stage'] = 'pilot-clock-exchanges'
            observations = result['clock_exchanges'] = []
            for role, session in enumerate(sessions):
                row = {'role': role, 'exchanges': []}
                observations.append(row)
                for _ in range(3):
                    row['exchanges'].append(exchange(session.call, run_id))
            result['start_proposal'] = plan_start(observations, run_id, stamp(), pilot_policy['start'])
            starts = [row['start_unix_ns'] for row in result['start_proposal']['roles']]
            result['stage'] = 'pilot-window'
            # Once dispatch begins, a lost reply cannot prove that G was never
            # received. A valid measurement proves a start; otherwise unknown.
            result['timed_window_started'] = None
            collection = collect_window(sessions, starts, output / 'window', timing_policy=pilot_policy['overlap'])
            path = output / 'window' / 'collection.json'
            result['collection'] = {'file': 'window/collection.json', 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                                    'status': collection['status']}
            if any('review' in row for row in collection['nodes']):
                result['timed_window_started'] = True
            require(collection['status'] == 'complete_unqualified', 'pilot collection incomplete')
            verify('after-window', range(len(nodes)), formation, assigned, pins)
            findings = []
            if collection.get('timing_review', {}).get('status') != 'within_conditional_bounds':
                findings.append('timing_bounds_not_met_or_unavailable')
            for row in collection['nodes']:
                if row['review']['measurement_issues']:
                    findings.append(f"role_{row['role']}_load_or_resource_sampling")
                if row['review']['environment']['issues'] or row['environment']['issues']:
                    findings.append(f"role_{row['role']}_environment")
            result['findings'] = findings
            result['status'] = 'complete_with_findings' if findings else 'complete_unqualified'
            return result  # The shared finally ALWAYS stops workers/probes.
        if probe_sha256 is not None:
            from pi_lab_clock import exchange
            result['stage'] = 'clock-exchanges'
            result['clock_exchanges'] = []
            for role, session in enumerate(sessions):
                # Retain each exchange, including partial batches on failure.
                # These are diagnostics only; no policy or window is qualified.
                for _ in range(3):
                    observation = exchange(session.call, run_id)
                    result['clock_exchanges'].append({'role': role, 'exchange': observation})
            for role, session in enumerate(sessions):
                result['stage'] = 'probe-warmup-' + str(role)
                ready = session.call('prepare-load', formation=formation, node=assigned[role],
                                     workers=len(nodes), role=role, probe_sha256=probe_sha256)
                require(ready == {'prepared': True, 'probe_sha256': probe_sha256,
                                  'workers': len(nodes), 'role': role}, 'probe preflight receipt')
                stopped = session.call('stop-load')
                require(stopped['timed_window_started'] is False and not stopped['forced'], 'probe preflight cleanup')
                record('probe-warmup-' + str(role), prepared=ready, cleanup=stopped)
            result['timed_window_started'] = False
        if mode == 'metrics':
            diagnostics = [session.call('diagnostics') for session in sessions]
            record('diagnostics', nodes=diagnostics)
            require(all(all(reply['status'] == 200 for reply in value.values())
                        and value['/metrics']['series'] > 0 for value in diagnostics), 'diagnostics endpoint qualification')
        for locked, role in ((True, 0), (False, len(nodes) - 1)):
            op = 'lock' if locked else 'unlock'
            receipt = sessions[role].call(op, formation=formation, operation_id='smoke-' + op)
            require(receipt['locked'] is locked, 'policy not accepted')
            verify(op, range(len(nodes)), formation, assigned, pins, locked)
        result['stage'] = 'leave'
        receipt = sessions[-1].call('leave', formation=formation, operation_id='smoke-leave')
        require(receipt['changed'] and receipt['previousFormationId'] == formation
                and receipt['previousNodeId'] == assigned[-1], 'leave receipt identity')
        standalone = summary(sessions[-1].call('info'))
        require(standalone['formationId'] != formation and standalone['sourceNodeId'] not in assigned
                and standalone['participation'] == 'standalone' and standalone['nodes'] == standalone['alive'] == 1,
                'leave did not create a fresh standalone formation')
        retired = {assigned[-1]: pins[-1]}
        verify('survivors-after-leave', range(len(nodes) - 1), formation, assigned[:-1], pins[:-1], retired=retired)
        assigned[-1] = join(len(nodes) - 1, 0, formation, standalone, assigned[0], pins[0], 'smoke-rejoin')
        verify('rejoined', range(len(nodes)), formation, assigned, pins, retired=retired)
        result['status'] = 'complete'
    except (Exception, KeyboardInterrupt) as error:
        result['error_type'] = type(error).__name__
        # Exceptions can originate in hostile replies; do not export their text.
    finally:
        result['setup_recovery_seconds'] = time.monotonic() - began
        if pilot_policy is not None:
            result['setup_measurement_seconds'] = result.pop('setup_recovery_seconds')
            result['measurement_dispatch_attempts'] = sum(session.measurement_attempted for session in sessions)
            if not result['measurement_dispatch_attempts']:
                result['timed_window_started'] = False
        def stop(session):
            try:
                return {'name': session.node['name'], **session.stop()}
            except Exception as error:
                return {'name': session.node['name'], 'clean': False, 'error_type': type(error).__name__}
        if sessions:
            with ThreadPoolExecutor(max_workers=5) as pool:
                result['cleanup'] = list(pool.map(stop, sessions))
        result['cleanup_clean'] = len(sessions) == len(inventory['nodes']) and all(r['clean'] for r in result['cleanup'])
        result['elapsed_seconds'] = time.monotonic() - began
        if not result['cleanup_clean']:
            result['status'] = 'incomplete'
        if pilot_policy is not None and result['elapsed_seconds'] > pilot_policy['max_elapsed_seconds']:
            result.update(status='incomplete', elapsed_budget_exceeded=True)
        write_new_json(output / 'result.json', result)
    return result
