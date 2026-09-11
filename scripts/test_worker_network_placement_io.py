"""Regression tests at the actual coordinator and TLS probe boundaries."""
import importlib.util
import os
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from worker_placement_topology import COUNTERS

spec = importlib.util.spec_from_file_location(
    'placement_harness', Path(__file__).with_name('check-worker-network-placement.py'))
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class LinkRecovery(unittest.TestCase):
    def run_case(self, *, established=True, recovers=True, change_error=False):
        state = {'down': False, 'changed': False, 'bytes': 0}
        actions = []
        def collect():
            return {'views': [{'locked': state['changed']},
                              {'locked': state['changed'] and not state['down'] and recovers}],
                    'valid': established}
        def switch(up):
            state['down'] = not up
            actions.append(up)
        def change():
            if change_error:
                raise ValueError('command failed')
            state['changed'] = True
        def sample():
            state['bytes'] += 10000
            return {(ns, role): dict.fromkeys(COUNTERS, 0) |
                    {'tx_bytes': state['bytes'] if role == 'selected' else 0}
                    for ns in harness.NAMESPACES for role in ('selected', 'excluded')}
        def wait(action, accept, *args, **kwargs):
            result = action()
            return result if accept(result) else None
        baseline = sample()
        try:
            result = harness.verify_link_recovery(
                collect, lambda value: bool(value and value['valid']), sample, baseline,
                switch, change, wait=wait, pause=lambda _: None)
            return result, actions
        finally:
            self.actions = actions

    def test_requires_formation_before_disruption(self):
        with self.assertRaises(AssertionError):
            self.run_case(established=False)
        self.assertEqual(self.actions, [])

    def test_requires_fresh_state_to_cross_after_restoration(self):
        with self.assertRaises(AssertionError):
            self.run_case(recovers=False)
        self.assertEqual(self.actions, [False, True])

    def test_restores_link_even_if_command_fails(self):
        with self.assertRaises(ValueError):
            self.run_case(change_error=True)
        self.assertEqual(self.actions, [False, True])

    def test_success_records_loss_and_recovery(self):
        result, actions = self.run_case()
        self.assertEqual(actions, [False, True])
        self.assertTrue(result[2]['recovery_verified'])


class HarnessSafety(unittest.TestCase):
    def test_evidence_is_private_and_never_overwrites_an_existing_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            harness.write_report(output, {'status': 'passed'})
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            self.assertIn('passed', output.read_text())
            with self.assertRaises(FileExistsError):
                harness.write_report(output, {'status': 'failed'})
            self.assertIn('passed', output.read_text())

    def test_existing_resources_are_refused_without_cleanup(self):
        with patch.object(sys, 'argv', ['check-worker-network-placement.py']), \
                patch.object(harness.os, 'geteuid', return_value=0), \
                patch.object(harness.shutil, 'which', return_value='/test/tool'), \
                patch.object(Path, 'is_file', return_value=True), \
                patch.object(harness, 'existing_resources', return_value=['namespace occupied']), \
                patch.object(harness, 'remove') as remove, \
                patch.object(harness, 'force_cleanup') as force, \
                patch.object(harness, 'build_topology') as build:
            with self.assertRaisesRegex(AssertionError, 'refusing to run'):
                harness.main()
            remove.assert_not_called()
            force.assert_not_called()
            build.assert_not_called()

    def test_scenario_directory_exists_before_worker_construction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'scenario'
            def construct(*args, **kwargs):
                self.assertTrue(root.is_dir())
                self.assertEqual(root.stat().st_mode & 0o777, 0o700)
                raise RuntimeError('stop before launching anything')
            with patch.object(harness, 'Worker', side_effect=construct):
                with self.assertRaisesRegex(RuntimeError, 'stop before'):
                    harness.attempt_formation('worker', 'ctl', root, {},
                                             introducer_placed=True, applicant_placed=True)

    def test_remove_only_uses_created_resources_in_reverse_order(self):
        created = [('namespace', 'owned-a'), ('link', 'owned-link')]
        with patch.object(harness.subprocess, 'run') as run:
            harness.remove(created)
        self.assertEqual([call.args[0] for call in run.call_args_list],
                         [['ip', 'link', 'delete', 'owned-link'],
                          ['ip', 'netns', 'delete', 'owned-a']])
        self.assertEqual(created, [])


class RealClientProbe(unittest.TestCase):
    """Execute the identical probe against a real placed worker, never a fake TLS peer."""
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix='placement-probe-test-')
        cls.addClassCleanup(cls.directory.cleanup)
        root = Path(cls.directory.name)
        cls.cert, key = harness.client_certificate(root, '127.0.0.1')
        other = root / 'other'
        other.mkdir()
        cls.untrusted, _ = harness.client_certificate(other, '127.0.0.1')
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            cls.port = reservation.getsockname()[1]
        cls.token = root / 'state' / 'operator.token'
        log = (root / 'worker.log').open('wb')
        cls.addClassCleanup(log.close)
        worker = subprocess.Popen([
            str(Path('target/debug/orishu-worker').resolve()), '--state-dir', str(root / 'state'),
            '--listen.clients', f'0.0.0.0:{cls.port}', '--interface.clients', 'lo',
            '--tls-cert', str(cls.cert), '--tls-key', str(key)],
            stdout=log, stderr=log,
            env={k: v for k, v in os.environ.items() if not k.startswith('ORISHU_')})
        def stop():
            worker.terminate()
            try:
                worker.wait(timeout=10)
            except subprocess.TimeoutExpired:
                worker.kill()
                worker.wait(timeout=5)
        cls.addClassCleanup(stop)
        for _ in range(100):
            try:
                with socket.create_connection(('127.0.0.1', cls.port), timeout=.1):
                    return
            except OSError:
                time.sleep(.05)
        raise AssertionError('test worker did not start')

    def probe(self, mode='valid', verify_host='127.0.0.1', untrusted=False):
        return subprocess.run([
            sys.executable, '-c', harness.CLIENT_PROBE, 'lo', '127.0.0.1', str(self.port),
            str(self.untrusted if untrusted else self.cert), str(self.token), '1', mode,
            verify_host], capture_output=True, timeout=15)

    def test_authenticated_request_over_placed_listener(self):
        self.assertEqual(self.probe().returncode, 0)

    def test_missing_and_wrong_credentials_are_rejected(self):
        for mode in ('missing-token', 'wrong-token'):
            result = self.probe(mode)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_ip_identity_is_not_network_exclusion(self):
        self.assertEqual(self.probe(verify_host='127.0.0.2').returncode,
                         harness.PROBE_BAD_IDENTITY)

    def test_untrusted_certificate_is_not_network_exclusion(self):
        self.assertEqual(self.probe(untrusted=True).returncode, harness.PROBE_BAD_IDENTITY)


if __name__ == '__main__':
    unittest.main()
