#!/usr/bin/env python3
"""Local contracts for Pi preparation/SSH readiness; no real Pi or SSH contact."""
import importlib.util
import json
import os
from pathlib import Path
import stat
import struct
import sys
import tempfile
import unittest

import pi_lab_node as node

spec = importlib.util.spec_from_file_location('pi_coordinator', Path(__file__).with_name('pi-lab.py'))
lab = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lab)


def inventory(count=3):
    return {'schema_version': 1, 'nodes': [
        {'name': f'pi{i}', 'ssh_host': f'orishu-pi{i}', 'peer_address': f'192.0.2.{i}',
         'interface': 'eth0', 'root': '/home/orishu/orishu-lab'} for i in range(1, count + 1)]}


class Contracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def load(self, value):
        path = self.root / 'inventory.json'
        path.write_text(json.dumps(value))
        return lab.inventory(path)

    def test_exact_three_or_five_unique_physical_targets(self):
        for count in (3, 5):
            self.assertEqual(len(self.load(inventory(count))['nodes']), count)
        for count in (0, 1, 2, 4, 6, 30):
            with self.assertRaises(ValueError):
                self.load(inventory(count))

    def test_host_injection_duplicate_and_loopback_rejected(self):
        for field, value in (('ssh_host', '-oProxyCommand=bad'), ('name', '../escape'),
                             ('ssh_host', 'pi;echo bad'), ('peer_address', '127.0.0.1'),
                             ('peer_address', '0.0.0.0'), ('peer_address', '224.0.0.1'),
                             ('root', '/home/u/../secret'), ('interface', '../state')):
            data = inventory()
            data['nodes'][0][field] = value
            with self.assertRaises(ValueError):
                self.load(data)
        for field in ('name', 'ssh_host', 'peer_address'):
            data = inventory()
            data['nodes'][1][field] = data['nodes'][0][field]
            with self.assertRaises(ValueError):
                self.load(data)

    def test_duplicate_fields_and_oversized_inventory_rejected(self):
        path = self.root / 'inventory.json'
        for raw in ('{"schema_version":1,"schema_version":1,"nodes":[]}', ' ' * 16385):
            path.write_text(raw)
            with self.assertRaises(ValueError):
                lab.inventory(path)

    def test_remote_root_is_not_resolved_on_coordinator_and_is_shell_quoted(self):
        data = inventory()
        data['nodes'][0]['root'] = "/home/orishu/a path;echo nope"
        target = self.load(data)['nodes'][0]
        cmd = lab.ssh_command(target)
        self.assertIn('BatchMode=yes', cmd)
        self.assertIn('StrictHostKeyChecking=yes', cmd)
        self.assertIn('ForwardAgent=no', cmd)
        self.assertIn("'/home/orishu/a path;echo nope'", cmd[-1])
        self.assertEqual(cmd[-2], target['ssh_host'])

    def test_prepare_is_private_and_never_overwrites_existing_root(self):
        root = self.root / 'lab'
        node.prepare(str(root))
        for name in ('', 'bin', 'runs', 'results'):
            self.assertEqual(stat.S_IMODE((root / name).stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((root / '.orishu-pi-lab.json').stat().st_mode), 0o600)
        with self.assertRaises(FileExistsError):
            node.prepare(str(root))
        link = self.root / 'link'
        link.symlink_to(root, target_is_directory=True)
        with self.assertRaises(ValueError):
            node.prepare(str(link / 'child'))

    def test_artifacts_are_hashed_and_header_checked_without_execution(self):
        path = self.root / 'binary'
        header = bytearray(64)
        header[:6] = b'\x7fELF\x02\x01'
        struct.pack_into('<H', header, 18, 183)
        path.write_bytes(header)
        path.chmod(0o700)
        self.assertTrue(node.artifact(path)['elf64_aarch64'])
        struct.pack_into('<H', header, 18, 62)
        path.write_bytes(header)
        self.assertFalse(node.artifact(path)['elf64_aarch64'])
        link = self.root / 'link'
        link.symlink_to(path)
        self.assertFalse(node.artifact(link)['present'])

    def test_bounded_command_stdin_stdout_stderr_and_exit(self):
        result = node.bounded_command([sys.executable, '-c',
            'import sys; print(len(sys.stdin.buffer.read())); print("note",file=sys.stderr); sys.exit(2)'], b'a' * 20000)
        self.assertEqual(result, {'exit': 2, 'stdout': '20000\n', 'stderr': 'note\n'})

    def test_bounded_command_timeout_and_output_cap(self):
        with self.assertRaises(TimeoutError):
            node.bounded_command([sys.executable, '-c', 'import time; time.sleep(3)'], timeout=0.1)
        with self.assertRaises(ValueError):
            node.bounded_command([sys.executable, '-c', 'print("x"*20000)'], cap=100)

    def test_standalone_script_can_be_streamed_without_repository_imports(self):
        root = self.root / 'lab'
        node.prepare(str(root))
        code = Path(node.__file__).read_bytes()
        result = node.bounded_command([sys.executable, '-', 'check', '--root', str(root),
                                       '--interface', 'lo'], code, timeout=15)
        report = json.loads(result['stdout'])
        self.assertEqual(report['kind'], 'pi-lab-readiness')
        self.assertFalse(report['experiment_ready'])
        self.assertFalse(report['artifacts_staged'])
        self.assertEqual(report['root'], str(root))

    def test_exited_parent_with_live_pipe_holder_is_still_bounded(self):
        with self.assertRaises(TimeoutError):
            node.bounded_command([sys.executable, '-c',
                'import os,time; pid=os.fork(); os._exit(0) if pid else time.sleep(3)'], timeout=0.1)

    def test_closed_pipes_do_not_avoid_command_deadline(self):
        with self.assertRaises(TimeoutError):
            node.bounded_command([sys.executable, '-c',
                'import os,time; os.close(1); os.close(2); time.sleep(3)'], timeout=0.1)

    def test_collection_preserves_all_host_failures_without_retry(self):
        data = inventory()
        calls = []
        def remote(command, source, **limits):
            calls.append(command[-2])
            self.assertIn(b'pi-lab-readiness', source)
            self.assertEqual(limits, {'timeout': 60, 'cap': 131072})
            if len(calls) == 2:
                raise TimeoutError('offline')
            report = {'schema_version': 1, 'kind': 'pi-lab-readiness', 'root': data['nodes'][0]['root'],
                      'interface': 'eth0', 'addresses': [data['nodes'][len(calls)-1]['peer_address']],
                      'infrastructure_ready': len(calls) == 1}
            return {'exit': 0 if len(calls) == 1 else 2, 'stdout': json.dumps(report), 'stderr': 'private remote diagnostic'}
        output = self.root / 'evidence'
        result = lab.collect(data, output, remote)
        self.assertEqual(len(calls), 3)
        self.assertEqual([r['status'] for r in result['nodes']], ['ready', 'failed', 'needs_setup'])
        self.assertFalse(result['all_infrastructure_ready'])
        self.assertFalse(result['experiment_run'])
        self.assertNotIn('private remote diagnostic', (output / 'summary.json').read_text())
        with self.assertRaises(FileExistsError):
            lab.collect(data, output, remote)

    def test_collection_rejects_remote_target_mismatch(self):
        def remote(*args, **kwargs):
            return {'exit': 0, 'stderr': '', 'stdout': json.dumps({
                'schema_version': 1, 'kind': 'pi-lab-readiness', 'root': '/wrong',
                'interface': 'eth0', 'infrastructure_ready': True})}
        result = lab.collect(inventory(), self.root / 'evidence', remote)
        self.assertTrue(all(r['status'] == 'failed' for r in result['nodes']))

    def test_unassigned_peer_address_is_not_ready(self):
        def remote(*args, **kwargs):
            return {'exit': 0, 'stderr': '', 'stdout': json.dumps({
                'schema_version': 1, 'kind': 'pi-lab-readiness', 'root': '/home/orishu/orishu-lab',
                'interface': 'eth0', 'addresses': ['198.51.100.1'], 'infrastructure_ready': True})}
        result = lab.collect(inventory(), self.root / 'evidence', remote)
        self.assertTrue(all(r['status'] == 'failed' for r in result['nodes']))


if __name__ == '__main__':
    unittest.main()
