#!/usr/bin/env python3
"""Capacity profile bounds, accounting and owned-worker cleanup regressions."""
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

from pi_lab_capacity_node import CapacitySession, ProbeDrain, profile, scheduler_snapshot
from pi_lab_session import Session, experiment_options
from pi_lab_session import decode, line
from pi_lab_node import artifact, prepare
from pi_lab_experiment import send

spec = importlib.util.spec_from_file_location('capacity_cli', Path(__file__).with_name('pi-lab-capacity.py'))
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)


class CapacityTests(unittest.TestCase):
    def test_error_diagnostic_is_one_pair_and_never_retries_failure(self):
        cell = Mock()
        capacity.execute_curve(cell, 5000, error_diagnostic=True)
        self.assertEqual([c.args for c in cell.call_args_list], [(None, 32), (None, 64)])
        cell = Mock(side_effect=ValueError('stop'))
        with self.assertRaises(ValueError):
            capacity.execute_curve(cell, 5000, error_diagnostic=True)
        self.assertEqual(cell.call_count, 1)

    def test_error_details_bound_categories_counts_status_and_time(self):
        import copy
        first = {'kind': 'transport_empty_body', 'status': 503, 'request_started_us': 900,
                 'completed_us': 1000, 'duration_us': 100, 'after_window': False}
        details = {'counts': [{'kind': 'transport_empty_body', 'status': 503, 'count': 1}], 'first': first}
        capacity.review_error_details(details, 1, 32)
        capacity.review_error_details({'counts': [], 'first': None}, 0, 32)
        mutations = [lambda d: d['counts'].append(d['counts'][0]),
                     lambda d: d['counts'][0].update(count=True),
                     lambda d: d['counts'][0].update(status=999),
                     lambda d: d['counts'][0].update(kind='raw secret'),
                     lambda d: d['first'].update(duration_us=101),
                     lambda d: d['first'].update(after_window=True),
                     lambda d: d['first'].update(message='secret')]
        for mutate in mutations:
            bad = copy.deepcopy(details)
            mutate(bad)
            with self.assertRaises(ValueError):
                capacity.review_error_details(bad, 1, 32)
        with self.assertRaises(ValueError):
            capacity.review_error_details(details, 2, 32)

    def test_network_v4_requires_details_and_preserves_arrival_accounting(self):
        value = self.report()
        value.update(schema_version=4, kind='pi-network-capacity-load')
        expected = self.config() | {'schema_version': 4}
        for row in value['workers']:
            row['client_error_details'] = {'counts': [], 'first': None}
        capacity.review_load(value, expected, network=True)
        del value['workers'][0]['client_error_details']
        with self.assertRaises(ValueError):
            capacity.review_load(value, expected, network=True)

    def check_scheduler_lead(self, network, overrun=False):
        from pi_lab_network import NetworkRemote
        class AtResourceBoundary(Exception):
            pass
        clock, observed = [0], []
        def snapshot(process):
            observed.append(clock[0])
            clock[0] += 9_000_000_000 if overrun else 80_000_000
            return {'monotonic_ns': clock[0], 'threads': {}}
        process = Mock()
        process.sample.side_effect = AtResourceBoundary
        session = (NetworkRemote if network else CapacitySession).__new__(NetworkRemote if network else CapacitySession)
        session.deadline = 60
        if network:
            session.process, session.local_start_ns = process, 8_000_000_000
            invoke = session.measure_local
        else:
            session.processes, session.child = [process], None
            session.external_generator, session.progress = True, {'stage': 'warmup'}
            invoke = lambda: session.measure({'start_unix_ns': 8_000_000_000})
        module = 'pi_lab_network' if network else 'pi_lab_capacity_node'
        with patch(module + '.time.monotonic_ns', side_effect=lambda: clock[0]), \
                patch(module + '.time.monotonic', side_effect=lambda: clock[0] / 1e9), \
                patch(module + '.time.time_ns', return_value=0), \
                patch(module + '.time.sleep', side_effect=lambda seconds: clock.__setitem__(0, clock[0] + int(seconds * 1e9))), \
                patch(module + '.scheduler_snapshot', side_effect=snapshot):
            with self.assertRaises(ValueError if overrun else AtResourceBoundary):
                invoke()
        self.assertEqual(observed, [0], 'scheduler reads must precede the scheduled resource boundary')
        if overrun:
            process.sample.assert_not_called()
        else:
            self.assertEqual(clock[0], 8_000_000_000)

    def test_node_scheduler_reads_use_lead_not_measurement_start(self):
        self.check_scheduler_lead(False)

    def test_generator_scheduler_reads_use_lead_not_measurement_start(self):
        self.check_scheduler_lead(True)

    def test_node_rejects_scheduler_collection_overrunning_lead(self):
        self.check_scheduler_lead(False, overrun=True)

    def test_generator_rejects_scheduler_collection_overrunning_lead(self):
        self.check_scheduler_lead(True, overrun=True)

    def test_scheduler_receipt_brackets_real_owned_process(self):
        import os
        from pi_lab_measurement import Process
        process = Process(os.getpid())
        before, after = scheduler_snapshot(process), scheduler_snapshot(process)
        self.assertGreaterEqual(after['monotonic_ns'], before['monotonic_ns'])
        self.assertIn(str(os.getpid()), before['threads'])
        self.assertEqual(len(before['threads'][str(os.getpid())]['schedstat']), 3)

    def test_explicit_topologies_and_executor_options_are_bounded(self):
        for local in (1, 4):
            for hosts in (3, 4, 5):
                options = {'hosts': hosts, 'workers_per_host': local,
                           'executor_threads': 2, 'placement': True}
                self.assertEqual(experiment_options(options), options)
                profile(self.config() | {'global_workers': hosts * local,
                        'first_role': (hosts - 1) * local,
                        'nodes': [str(n) for n in range(local)]})
        for key, value in (('hosts', 6), ('hosts', True), ('workers_per_host', 0),
                           ('executor_threads', 32), ('executor_threads', True), ('placement', 'yes')):
            with self.subTest(key=key), self.assertRaises(ValueError):
                experiment_options(options | {key: value})
        with self.assertRaises(ValueError):
            profile(self.config() | {'global_workers': 5})

    def test_new_reports_validate_five_and_twenty_member_identities(self):
        for local in (1, 4):
            report = self.report()
            report.update(schema_version=3, kind='pi-network-capacity-load', global_workers=5 * local)
            report['workers'] = report['workers'][:local]
            expected = self.config() | {'schema_version': 3, 'global_workers': 5 * local,
                                       'nodes': [str(n) for n in range(local)]}
            self.assertEqual(len(capacity.review_load(report, expected, network=True)), local)
            report['global_workers'] = 16
            with self.assertRaises(ValueError):
                capacity.review_load(report, expected, network=True)

    def test_placed_launch_keeps_unix_control_and_sets_only_worker_executor(self):
        with tempfile.TemporaryDirectory() as directory:
            for address, wildcard in (('192.0.2.11', '0.0.0.0'), ('2001:db8::11', '[::]')):
                session = Session.__new__(Session)
                session.base = Path(directory)
                (session.base / 'api.sock').touch()
                session.worker, session.mode = None, 'compiled_off'
                session.address, session.interface, session.name = address, 'eth0', 'test'
                session.hashes, session.deadline = {}, time.monotonic() + 120
                session.client_tls = (f'{address}:9440', Path('/tmp/cert'), Path('/tmp/key'))
                session.experiment = {'hosts': 5, 'workers_per_host': 1,
                                      'executor_threads': 2, 'placement': True}
                with patch('pi_lab_session.subprocess.Popen') as spawn, patch('pi_lab_session.Drain'):
                    session.start()
                command = spawn.call_args.args[0]
                self.assertEqual(command[command.index('--listen.peers') + 1], wildcard + ':9000')
                self.assertIn(wildcard + ':9440', command)
                self.assertIn(str(session.base / 'api.sock'), command)
                self.assertIn('--advertise.peers', command)
                self.assertIn('--interface.clients', command)
                self.assertEqual(spawn.call_args.kwargs['env']['TOKIO_WORKER_THREADS'], '2')

    def test_failed_clock_planning_keeps_unmodified_input_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'clocks.json'
            with patch.object(capacity, 'stamp', return_value={'time': 123}), \
                    patch.object(capacity, 'plan_start', side_effect=ValueError('gate')):
                with self.assertRaises(ValueError):
                    capacity.record_start_plan([{'raw': 'sample'}], 'run', {'bound': 25}, path)
            self.assertEqual(json.loads(path.read_text()), {'clocks': [{'raw': 'sample'}],
                'run_id': 'run', 'now': {'time': 123}, 'policy': {'bound': 25}})

    def test_short_comparison_runs_exactly_three_profiles_twice_despite_rate_misses(self):
        cell = Mock(return_value={'delivery_met': False})
        capacity.execute_curve(cell, 5000, short_comparison=True)
        self.assertEqual([call.args for call in cell.call_args_list],
                         [(5000, 32), (None, 32), (None, 64)] * 2)

    def test_short_comparison_does_not_continue_after_a_cell_safety_failure(self):
        cell = Mock(side_effect=ValueError('safety stop'))
        with self.assertRaises(ValueError):
            capacity.execute_curve(cell, 5000, short_comparison=True)
        self.assertEqual(cell.call_count, 1)

    def test_adaptive_curve_keeps_rate_bracket_and_best_concurrency_confirmation(self):
        calls = []
        def cell(rate, clients):
            calls.append((rate, clients))
            return {'delivery_met': rate == 1250, 'aggregate_qps': {2: 10, 8: 30, 32: 20}[clients],
                    'clients_per_worker': clients}
        capacity.execute_curve(cell, 5000)
        self.assertEqual(calls, [(5000, 32), (2500, 32), (1250, 32),
                                (None, 2), (None, 8), (None, 32), (None, 8)])

    def config(self):
        return {'cell': 0, 'first_role': 0, 'rate_per_worker': 5000, 'clients_per_worker': 32,
                'formation': 'formation', 'nodes': ['a', 'b', 'c', 'd'], 'probe_sha256': 'a' * 64}

    def report(self, rate=5000):
        latency = {'requests': 49999, 'median_us': 40., 'p95_us': 60.}
        return {'schema_version': 1, 'kind': 'pi-capacity-load', 'global_workers': 16,
                'first_role': 0, 'clients_per_worker': 32, 'rate_per_worker': rate, 'seconds': 10,
                'workers': [{'role': role, 'latency': latency.copy(),
                             'scheduling_delay': latency.copy() if rate else {'requests': 0, 'median_us': None, 'p95_us': None},
                             'scheduled_arrivals': rate * 10 if rate else None, 'skipped_arrivals': 1 if rate else 0,
                             'tail_requests': 0, 'transport_errors': 0, 'invalid_responses': 0, 'capped': False}
                            for role in range(4)],
                'start': {'unix_ns': 1, 'read_bracket_ns': 0},
                'end_marker': {'unix_ns': 10_000_000_001, 'read_bracket_ns': 0},
                'end_marker_elapsed_ns': 10_000_000_000}

    def test_profiles_bound_ports_roles_rates_clients_and_attempts(self):
        profile(self.config())
        for key, values in {'cell': [-1, 12, True], 'first_role': [1, 16, True],
                            'clients_per_worker': [0, 65, True], 'rate_per_worker': [0, 100001, True]}.items():
            for value in values:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    profile(self.config() | {key: value})
        for port, lifetime in ((8999, 180), (9004, 180), (9000, 601), (9000, 179)):
            with self.assertRaises(ValueError):
                Session({}, peer_port=port, lifetime=lifetime)

    def test_rejects_duplicate_targets_and_extra_fields(self):
        with self.assertRaises(ValueError):
            profile(self.config() | {'nodes': ['a'] * 4})
        with self.assertRaises(ValueError):
            profile(self.config() | {'shell': 'unexpected'})

    def test_fixed_rate_and_unpaced_reports_keep_separate_accounting(self):
        for rate in (5000, None):
            report = self.report(rate)
            rows = capacity.review_load(report, self.config() | {'rate_per_worker': rate})
            self.assertEqual(sum(row['qps'] for row in rows), 19999.6)
        report = self.report()
        report['workers'][0]['transport_errors'] = 1
        with self.assertRaises(ValueError):
            capacity.review_load(report, self.config())
        report['workers'][0]['skipped_arrivals'] = 0
        self.assertEqual(capacity.review_load(report, self.config())[0]['transport_errors'], 1)

    def test_hostile_reports_fail_before_qps_aggregation(self):
        for change in ('duplicate_role', 'nan', 'wrong_rate', 'wrong_membership', 'unknown_field'):
            report = self.report()
            if change == 'duplicate_role': report['workers'][1]['role'] = 0
            elif change == 'nan': report['workers'][0]['latency']['p95_us'] = float('nan')
            elif change == 'wrong_rate': report['rate_per_worker'] = 500
            elif change == 'wrong_membership': report['global_workers'] = 4
            else: report['surprise'] = True
            with self.subTest(change=change), self.assertRaises(ValueError):
                capacity.review_load(report, self.config())

    def test_partial_initialization_cleans_all_previously_created_sessions(self):
        first = Mock()
        first.stop.return_value = {'clean': True}
        with patch('pi_lab_capacity_node.Session', side_effect=[first, ValueError('failure')]):
            with self.assertRaises(ValueError):
                CapacitySession({'mode': 'compiled_off', 'name': 'host', 'run_id': 'run'})
        first.stop.assert_called_once()

    def test_probe_cleanup_failure_cannot_skip_any_worker(self):
        session = CapacitySession.__new__(CapacitySession)
        session.sessions = [Mock() for _ in range(4)]
        for worker in session.sessions:
            worker.stop.return_value = {'clean': True}
        session.stop_probe = Mock(side_effect=ValueError('failure'))
        result = session.stop()
        self.assertFalse(result['clean'])
        for worker in session.sessions:
            worker.stop.assert_called_once()

    def test_bundle_is_bounded_and_contains_node_owned_watchdogs(self):
        raw = capacity.bundle()
        self.assertLessEqual(len(raw), 131072)
        compile(raw, 'capacity-bundle', 'exec')
        self.assertIn(b'pi_lab_capacity_node', raw)

    def test_probe_diagnostics_are_bounded_and_never_export_raw_text(self):
        drain = ProbeDrain(io.BytesIO(b'Error: failed to fill whole buffer SECRET\n' + b'x' * 10000))
        result = drain.finish()
        self.assertEqual(result['classification'], 'control_eof')
        self.assertEqual(len(drain.prefix), 4096)
        self.assertNotIn('SECRET', json.dumps(result))

    def test_non_capacity_worker_operations_are_rejected(self):
        session = CapacitySession.__new__(CapacitySession)
        session.sessions = [Mock() for _ in range(4)]
        with self.assertRaises(ValueError):
            session.request({'operation': 'worker', 'arguments': {
                'slot': 0, 'operation': 'leave', 'arguments': {}}})
        self.assertFalse(session.sessions[0].request.called)

    def test_coordinator_worker_dispatch_preserves_nested_operation_on_real_rpc_path(self):
        remote = capacity.CapacityRemote.__new__(capacity.CapacityRemote)
        remote.poisoned = False
        remote.write = Mock()
        remote.read = Mock(return_value={'ok': True, 'result': {'started': True}})
        self.assertEqual(remote.worker(2, 'start'), {'started': True})
        remote.write.assert_called_once_with({'operation': 'worker', 'arguments': {
            'slot': 2, 'operation': 'start', 'arguments': {}}})

    def test_real_four_worker_session_eof_preserves_artifacts_and_stops_owned_children(self):
        with tempfile.TemporaryDirectory(prefix='cap-', dir='/tmp') as temporary:
            root = Path(temporary) / 'lab'
            prepare(str(root))
            worker = root / 'bin' / 'orishu-worker'
            worker.write_text('#!' + sys.executable + '\n'
                              'import os,signal,socket,sys,time\n'
                              'path=sys.argv[sys.argv.index("--listen.clients")+1]\n'
                              's=socket.socket(socket.AF_UNIX);s.bind(path)\n'
                              'def stop(*args):\n s.close();os.unlink(path);sys.exit(0)\n'
                              'signal.signal(signal.SIGTERM,stop)\n'
                              'while True: time.sleep(1)\n')
            worker.chmod(0o700)
            ctl = root / 'bin' / 'orishuctl'
            ctl.write_bytes(worker.read_bytes())
            ctl.chmod(0o700)
            original = artifact(worker)['sha256']
            readiness = {'infrastructure_ready': True, 'artifacts_staged': True, 'addresses': ['192.0.2.1'],
                         'artifacts': {p.name: artifact(p) for p in (worker, ctl)}}
            code = ('import sys;sys.path.insert(0,' + repr(str(Path(__file__).parent.resolve())) + ');'
                    'import pi_lab_session as session;session.check=lambda *args:' + repr(readiness) + ';'
                    'import pi_lab_capacity_node as capacity;capacity.main()')
            child = subprocess.Popen([sys.executable, '-u', '-c', code], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
            try:
                config = {'root': str(root), 'interface': 'eth0', 'peer_address': '192.0.2.1',
                          'name': 'test', 'run_id': 'cap-eof', 'mode': 'compiled_off'}
                send(child.stdin, json.dumps(config).encode() + b'\n', time.monotonic() + 2)
                self.assertTrue(decode(line(child.stdout, time.monotonic() + 5))['ready'])
                for slot in range(4):
                    request = {'operation': 'worker', 'arguments': {'slot': slot, 'operation': 'start', 'arguments': {}}}
                    send(child.stdin, json.dumps(request).encode() + b'\n', time.monotonic() + 2)
                    self.assertTrue(decode(line(child.stdout, time.monotonic() + 5))['result']['started'])
                child.stdin.close()
                child.wait(timeout=10)
                for slot in range(4):
                    base = root / 'runs' / f'cap-eof-{slot}'
                    manifest = json.loads((base / 'manifest.json').read_text())
                    self.assertEqual(manifest['peer_port'], 9000 + slot)
                    self.assertEqual(manifest['lifetime_seconds'], 600)
                    cleanup = json.loads((base / 'cleanup.json').read_text())
                    self.assertTrue(cleanup['clean'])
                    self.assertTrue(cleanup['socket_removed'])
                self.assertEqual(artifact(worker)['sha256'], original)
            finally:
                if child.poll() is None:
                    child.terminate()
                    child.wait(timeout=10)
                for stream in (child.stdin, child.stdout, child.stderr):
                    stream.close()


if __name__ == '__main__':
    unittest.main()
