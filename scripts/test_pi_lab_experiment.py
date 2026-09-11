#!/usr/bin/env python3
"""Physical-harness contracts; no SSH connection or real Pi is required."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import pi_lab_experiment as experiment
import pi_lab_session as node
from pi_lab_node import artifact, bounded_command, prepare


class FakeRemote:
    pool = []
    history = []

    def __init__(self, config, run_id, mode, deadline):
        self.index = len(self.pool)
        self.node = config
        self.id, self.formation, self.pin = 'n' + str(self.index), 'f' + str(self.index), 'p' + str(self.index)
        self.locked, self.joined, self.receipt = False, False, None
        self.ready = {'run_id': run_id, 'artifacts': {}}
        self.pool.append(self)

    def call(self, op, **args):
        group = [p for p in self.pool if p.formation == self.formation]
        history = [entry[1] for entry in self.history if entry[0] == self.formation]
        if op == 'clock':
            from pi_lab_clock import receipt
            return receipt(self.ready['run_id'], args['nonce'])
        if op == 'start':
            return {'started': True}
        if op == 'prepare-load':
            return {'prepared': True, 'probe_sha256': args['probe_sha256'],
                    'workers': args['workers'], 'role': args['role']}
        if op == 'stop-load':
            return {'timed_window_started': False, 'forced': False}
        if op == 'info':
            return {'formationId': self.formation, 'sourceNodeId': self.id,
                    'nodes': len(group) + len(history), 'alive': len(group), 'locked': self.locked,
                    'introducerReady': True, 'participation': 'joined' if self.joined else 'standalone'}
        if op == 'members':
            return [{'nodeId': p.id, 'certFingerprint': p.pin, 'liveness': 'alive'} for p in group] + history
        if op == 'material':
            return {'formationId': self.formation, 'introducerNodeId': self.id,
                    'introducerFingerprint': self.pin, 'introducerReady': True, 'token': 'NEVER_EXPORT_SECRET'}
        if op == 'join':
            previous, old_id = self.formation, self.id
            self.formation, self.id, self.joined = args['material']['formationId'], self.id + 'j', True
            self.receipt = {'sourceFormationId': previous, 'sourceNodeId': old_id,
                            'targetFormationId': self.formation, 'state': {'phase': 'joined', 'nodeId': self.id}}
            return self.receipt
        if op == 'join-status':
            return self.receipt
        if op in ('lock', 'unlock'):
            for member in group:
                member.locked = op == 'lock'
            return {'locked': self.locked}
        if op == 'leave':
            previous, old_id = self.formation, self.id
            self.history.append((previous, {'nodeId': old_id, 'certFingerprint': self.pin, 'liveness': 'dead'}))
            self.formation, self.id, self.joined = self.formation + 'left', self.id + 'left', False
            return {'changed': True, 'previousFormationId': previous, 'previousNodeId': old_id}
        raise AssertionError(op)

    def stop(self):
        return {'clean': True}


class Contracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        FakeRemote.pool = []
        FakeRemote.history = []

    def test_complete_journey_keeps_credentials_out_of_evidence(self):
        for count in (3, 4, 5):
            FakeRemote.pool = []
            FakeRemote.history = []
            inventory = {'nodes': [{'name': 'pi' + str(i)} for i in range(count)]}
            output = self.root / str(count)
            result = experiment.smoke(inventory, output, remote_factory=FakeRemote)
            self.assertEqual(result['status'], 'complete')
            self.assertTrue(result['cleanup_clean'])
            self.assertIn('rejoined', [event['stage'] for event in result['events']])
            for path in output.glob('*.json'):
                self.assertNotIn('NEVER_EXPORT_SECRET', path.read_text())
                self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_initialization_failure_retains_failure_and_stops_started_nodes(self):
        def factory(*args):
            if len(FakeRemote.pool) == 1:
                raise OSError('private remote text')
            return FakeRemote(*args)
        result = experiment.smoke({'nodes': [{'name': str(i)} for i in range(3)]},
                                  self.root / 'failed', remote_factory=factory)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(len(result['cleanup']), 1)
        self.assertNotIn('private remote text', json.dumps(result))

    def test_probe_preflight_warms_all_roles_without_starting_timed_window(self):
        result = experiment.smoke({'nodes': [{'name': str(i)} for i in range(4)]}, self.root / 'probe',
                                  remote_factory=FakeRemote, probe_sha256='a' * 64)
        self.assertEqual(result['status'], 'complete')
        self.assertIs(result['timed_window_started'], False)
        events = [event for event in result['events'] if event['stage'].startswith('probe-warmup-')]
        self.assertEqual([event['prepared']['role'] for event in events], list(range(4)))
        self.assertTrue(all(event['prepared']['workers'] == 4 for event in events))
        self.assertEqual([entry['role'] for entry in result['clock_exchanges']],
                         [role for role in range(4) for _ in range(3)])
        self.assertTrue(all(entry['exchange']['clock_uncertainty_qualified'] is False
                            for entry in result['clock_exchanges']))

    def test_probe_cleanup_failure_cannot_skip_worker_cleanup(self):
        session = object.__new__(node.Session)
        session.base = self.root
        class BrokenMeasurement:
            def close(self):
                raise ValueError('probe cleanup fault')
        session.measurement = BrokenMeasurement()
        session.worker = subprocess.Popen(['timeout', '--preserve-status', '--kill-after=1s', '10s',
                                          sys.executable, '-c', 'import time;time.sleep(9)'],
                                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        session.drain = node.Drain(session.worker.stdout)
        result = session.stop()
        self.assertIsNone(session.worker)
        self.assertFalse(result['clean'])
        self.assertEqual(result['measurement']['error_type'], 'ValueError')

    def test_bad_clock_receipt_keeps_partial_batch_and_stops_all_nodes(self):
        class WrongRun(FakeRemote):
            def call(self, op, **args):
                value = super().call(op, **args)
                if op == 'clock' and self.index == 1:
                    return value | {'run_id': 'wrong-run'}
                return value
        result = experiment.smoke({'nodes': [{'name': str(i)} for i in range(4)]},
                                  self.root / 'clock-failure', remote_factory=WrongRun, probe_sha256='a' * 64)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['stage'], 'clock-exchanges')
        self.assertEqual(len(result['clock_exchanges']), 3)
        self.assertTrue(result['cleanup_clean'])
        self.assertEqual(len(result['cleanup']), 4)
        self.assertFalse(any(e['stage'].startswith('probe-warmup-') for e in result['events']))

    def test_existing_output_is_not_overwritten(self):
        with self.assertRaises(FileExistsError):
            experiment.smoke({'nodes': []}, self.root)

    def test_exact_membership_checks_pins_liveness_identity_and_policy(self):
        view = {'formationId': 'f', 'sourceNodeId': 'n', 'nodes': 1, 'alive': 1,
                'locked': False, 'introducerReady': True}
        members = [{'nodeId': 'n', 'certFingerprint': 'p', 'liveness': 'alive'}]
        self.assertTrue(experiment.exact([view], [members], 'f', ['n'], ['p']))
        self.assertFalse(experiment.exact([view], [members], 'f', ['n'], ['wrong']))
        self.assertFalse(experiment.exact([view | {'locked': True}], [members], 'f', ['n'], ['p']))
        with self.assertRaises(ValueError):
            experiment.exact([view], [members], 'wrong', ['n'], ['p'])

    def test_json_and_identifiers_reject_hostile_inputs(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}', b' ' * (node.CAP + 1)):
            with self.assertRaises(ValueError):
                node.decode(raw)
        for value in ('../escape', '-x', 'x;cmd', 'x' * 97, None):
            with self.assertRaises(ValueError):
                node.identifier(value)

    def test_line_partial_input_deadline_and_eof(self):
        read, write = os.pipe()
        with os.fdopen(read, 'rb', buffering=0) as reader, os.fdopen(write, 'wb', buffering=0) as writer:
            writer.write(b'partial')
            with self.assertRaises(ValueError):
                node.line(reader, time.monotonic() + 0.02)
            writer.close()
            with self.assertRaises(EOFError):
                node.line(reader, time.monotonic() + 1)

    def test_allowlist_refuses_arbitrary_operations_and_mutation_resubmission(self):
        session = object.__new__(node.Session)
        session.operations = set()
        session.cli = lambda arguments: arguments
        for request in ({'operation': 'shell', 'arguments': {}},
                        {'operation': 'info', 'arguments': {'extra': 'x'}},
                        {'operation': 'leave', 'arguments': {'formation': '../x', 'operation_id': 'id'}}):
            with self.assertRaises(ValueError):
                session.request(request)
        request = {'operation': 'lock', 'arguments': {'formation': 'f', 'operation_id': 'once'}}
        self.assertEqual(session.request(request)[0:2], ['cluster', 'lock'])
        with self.assertRaises(ValueError):
            session.request(request)

    def test_join_material_is_private_transient_and_removed_on_cli_failure(self):
        session = object.__new__(node.Session)
        session.operations, session.base = set(), self.root
        def fail(args):
            path = self.root / 'join.join.json'
            self.assertEqual(path.stat().st_mode & 0o077, 0)
            self.assertIn('secret', path.read_text())
            raise ValueError('CLI failed')
        session.cli = fail
        with self.assertRaises(ValueError):
            session.request({'operation': 'join', 'arguments': {
                'formation': 'f', 'operation_id': 'join', 'material': {'token': 'secret'}}})
        self.assertFalse((self.root / 'join.join.json').exists())

    def test_stop_reaps_only_owned_watchdog_and_drains_output(self):
        session = object.__new__(node.Session)
        session.base = self.root
        session.worker = subprocess.Popen(['timeout', '--preserve-status', '--signal=TERM', '--kill-after=1s', '10s',
                                          sys.executable, '-c', 'import time; print("bounded",flush=True); time.sleep(20)'],
                                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        session.drain = node.Drain(session.worker.stdout)
        result = session.stop()
        self.assertTrue(result['clean'])
        self.assertTrue(result['socket_removed'])
        self.assertIsNone(session.worker)

    def test_independent_watchdog_expires_even_without_supervisor_cleanup(self):
        began = time.monotonic()
        result = bounded_command(['timeout', '--signal=TERM', '--kill-after=0.1s', '0.1s',
                                  sys.executable, '-c', 'import os,time; print(os.getpid(),flush=True); time.sleep(10)'], timeout=2)
        # Older uutils returns 125 rather than GNU/newer uutils' 124 here.
        # Neither is success: prove actual child death, not a version-specific
        # exit convention. An expired experiment always remains a failure.
        self.assertNotEqual(result['exit'], 0)
        self.assertLess(time.monotonic() - began, 2)
        self.assertFalse(Path('/proc', result['stdout'].strip()).exists())

    def test_bundle_runs_without_installing_remote_modules(self):
        code = experiment.bundle()
        result = bounded_command([sys.executable, '-u', '-c',
            experiment.BOOTSTRAP, str(len(code))],
            code + b'{}\n', timeout=5)
        self.assertFalse(json.loads(result['stdout'])['ok'])
        self.assertEqual(json.loads(result['stdout'])['error_type'], 'ValueError')
        self.assertNotIn('Traceback', result['stdout'])

    def test_real_session_eof_stops_worker_and_preserves_staged_artifacts(self):
        root = self.root / 'lab'
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
        original_hash = artifact(worker)['sha256']
        readiness = {'infrastructure_ready': True, 'artifacts_staged': True, 'addresses': ['192.0.2.1'],
                     'artifacts': {p.name: artifact(p) for p in (worker, ctl)}}
        # Mock only the ARM64 readiness check, not session IO, child/watchdog,
        # EOF handling, artifact copying, or cleanup. The fake worker uses a
        # real Unix socket and the same CLI flags as production.
        code = ('import sys;sys.path.insert(0,' + repr(str(Path(node.__file__).parent)) + ');'
                'import pi_lab_session as session;session.check=lambda *args:' + repr(readiness) + ';session.main()')
        child = subprocess.Popen([sys.executable, '-u', '-c', code], stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
        try:
            config = {'root': str(root), 'interface': 'eth0', 'peer_address': '192.0.2.1',
                      'name': 'test', 'run_id': 'eof', 'mode': 'compiled_off'}
            experiment.send(child.stdin, json.dumps(config).encode() + b'\n', time.monotonic() + 2)
            self.assertTrue(node.decode(node.line(child.stdout, time.monotonic() + 5))['ready'])
            experiment.send(child.stdin, b'{"operation":"start","arguments":{}}\n', time.monotonic() + 2)
            self.assertTrue(node.decode(node.line(child.stdout, time.monotonic() + 5))['result']['started'])
            child.stdin.close()
            child.wait(timeout=10)
            cleanup = json.loads((root / 'runs' / 'eof' / 'cleanup.json').read_text())
            self.assertTrue(cleanup['clean'])
            self.assertTrue(cleanup['socket_removed'])
            self.assertEqual(artifact(worker)['sha256'], original_hash)
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=10)
            for stream in (child.stdin, child.stdout, child.stderr):
                stream.close()


if __name__ == '__main__':
    unittest.main()
