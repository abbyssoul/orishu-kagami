"""Network capacity's security, receipt and lifecycle boundaries."""
import hashlib
import importlib.util
import json
from pathlib import Path
import ssl
import subprocess
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

from pi_lab_network import NetworkRemote, bundle, endpoint, qualify
from pi_lab_network_node import NetworkSession
import test_pi_lab_capacity as fixtures


class NetworkTests(unittest.TestCase):
    def test_direct_ssh_keeps_hostname_trust_and_strict_checking(self):
        from pi_lab_experiment import Remote
        node = {'ssh_host': 'pi.example', 'peer_address': '192.0.2.10'}
        with patch('pi_lab_experiment.subprocess.Popen') as popen, \
                patch('pi_lab_experiment.Drain'), patch('pi_lab_experiment.send'), \
                patch.object(Remote, 'write'), patch.object(Remote, 'read', return_value={
                    'ok': True, 'ready': True, 'run_id': 'run', 'clock_only': True}):
            Remote(node, 'run', 'clock_only', time.monotonic() + 5,
                   bundle_factory=lambda: b'pass', direct_ip=True)
        command = popen.call_args.args[0]
        self.assertEqual(command[command.index('--') + 1], '192.0.2.10')
        self.assertIn('HostKeyAlias=pi.example', command)
        self.assertIn('StrictHostKeyChecking=yes', command)
        self.assertNotIn('StrictHostKeyChecking=no', command)

    def test_explicit_network_receipt_cannot_be_accepted_as_colocated(self):
        value = fixtures.CapacityTests().report()
        value.update(schema_version=2, kind='pi-network-capacity-load')
        expected = fixtures.CapacityTests().config()
        self.assertEqual(len(fixtures.capacity.review_load(value, expected, network=True)), 4)
        with self.assertRaises(ValueError):
            fixtures.capacity.review_load(value, expected)
        value['schema_version'] = 1
        with self.assertRaises(ValueError):
            fixtures.capacity.review_load(value, expected, network=True)

    def test_bundle_and_literal_endpoints(self):
        raw = bundle()
        self.assertLessEqual(len(raw), 131072)
        compile(raw, 'network-bundle', 'exec')
        self.assertEqual(endpoint('192.0.2.1', 3), '192.0.2.1:9443')
        self.assertEqual(endpoint('::1', 0), '[::1]:9440')
        with self.assertRaises(ValueError):
            endpoint('host;command', 0)

    def test_nested_worker_dispatch_uses_real_remote_call(self):
        remote = NetworkRemote.__new__(NetworkRemote)
        remote.poisoned = False
        remote.write = Mock()
        remote.read = Mock(return_value={'ok': True, 'result': {'started': True}})
        self.assertEqual(remote.worker(3, 'start'), {'started': True})
        remote.write.assert_called_once_with({'operation': 'worker', 'arguments': {
            'slot': 3, 'operation': 'start', 'arguments': {}}})

    def test_credentials_are_private_pinned_not_returned_and_removed_on_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            remote = NetworkRemote.__new__(NetworkRemote)
            remote.base, remote.secrets, remote.credentials, remote.security = Path(directory), [], None, []
            remote.node = {'peer_address': '192.0.2.1'}
            cert, token = 'CERTIFICATE', 'a' * 64
            remote.rpc = Mock(return_value=[{'endpoint': endpoint('192.0.2.1', slot),
                'token': token, 'certificate': cert, 'certificate_sha256': hashlib.sha256(cert.encode()).hexdigest()}
                for slot in range(4)])
            with patch('pi_lab_network.qualify', return_value={'verified': True}):
                self.assertIsNone(remote.load_credentials())
                remote.load_credentials()
            self.assertEqual(remote.rpc.call_count, 1)
            self.assertNotIn(token, json.dumps(remote.credentials))
            for path in remote.secrets:
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            remote.stop_probe = Mock(return_value={'clean': False})
            with patch('pi_lab_network.Remote.stop', return_value={'clean': True}):
                self.assertFalse(remote.stop()['clean'])
            self.assertTrue(all(not p.exists() for p in remote.secrets))

    def test_changed_endpoint_is_rejected_before_credential_write(self):
        remote = NetworkRemote.__new__(NetworkRemote)
        remote.credentials = None
        remote.node = {'peer_address': '192.0.2.1'}
        remote.rpc = Mock(return_value=[{'endpoint': '192.0.2.2:9440', 'token': 'a' * 64,
                                       'certificate': 'X', 'certificate_sha256': hashlib.sha256(b'X').hexdigest()}] * 4)
        with self.assertRaises(ValueError):
            remote.load_credentials()

    def test_tls_creation_failure_cleans_all_workers(self):
        session = NetworkSession.__new__(NetworkSession)
        worker = Mock()
        worker.base, worker.address = Path('/tmp/uncreated-test-path'), '127.0.0.1'
        def initialize(_config):
            session.sessions = [worker] * 4
        session.stop = Mock()
        with patch('pi_lab_network_node.CapacitySession.__init__', side_effect=initialize), \
                patch('pi_lab_network_node.subprocess.run', side_effect=ValueError('failure')):
            with self.assertRaises(ValueError):
                session.__init__({})
        session.stop.assert_called_once()

    def test_real_tls_worker_rejects_missing_wrong_tokens_and_untrusted_names(self):
        worker = Path(__file__).resolve().parents[1] / 'target/debug/orishu-worker'
        if not worker.exists():
            self.skipTest('build orishu-worker for the real TLS fixture')
        with tempfile.TemporaryDirectory(prefix='net-tls-', dir='/tmp') as directory:
            base = Path(directory)
            import socket
            with socket.socket() as reservation:
                reservation.bind(('127.0.0.1', 0))
                port = reservation.getsockname()[1]
            cert, key = base / 'cert', base / 'key'
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256',
                '-nodes', '-days', '1', '-subj', '/CN=lab', '-addext', 'subjectAltName=IP:127.0.0.1',
                '-addext', 'basicConstraints=critical,CA:FALSE', '-addext', 'keyUsage=critical,digitalSignature',
                '-addext', 'extendedKeyUsage=serverAuth', '-out', str(cert), '-keyout', str(key)],
                check=True, timeout=10, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            import os
            environment = {k: v for k, v in os.environ.items() if not k.startswith(('ORISHU_', 'OTEL_'))}
            child = subprocess.Popen(['timeout', '--kill-after=2s', '20s', str(worker), '--state-dir', str(base / 'state'),
                '--listen.clients', f'127.0.0.1:{port}', '--tls-cert', str(cert), '--tls-key', str(key)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=environment)
            try:
                deadline = time.monotonic() + 5
                while True:
                    try:
                        result = qualify('127.0.0.1', port, cert)
                        break
                    except ConnectionRefusedError:
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(.05)
                self.assertEqual(result['missing_token_status'], 401)
                import http.client
                context = ssl.create_default_context(cafile=str(cert))
                client = http.client.HTTPSConnection('127.0.0.1', port, context=context, timeout=2)
                try:
                    client.request('GET', '/api/v1/cluster', headers={
                        'Authorization': 'Bearer ' + (base / 'state/operator.token').read_text()})
                    response = client.getresponse()
                    self.assertEqual(response.status, 200)
                    self.assertLess(len(response.read(8193)), 8193)
                finally:
                    client.close()
            finally:
                child.terminate()
                child.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
