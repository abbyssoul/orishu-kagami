"""Ephemeral TLS listeners and worker-only sampling for off-host capacity load.

No load generator or TLS private key leaves a Pi. Credentials are returned once
over the verified SSH control channel, never written into result receipts.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import time

from pi_lab_capacity_node import CapacitySession, profile, placement_snapshot
from pi_lab_measurement import Process, watchdog_child
from pi_lab_node import require


class NetworkSession(CapacitySession):
    def __init__(self, config):
        super().__init__(config)
        self.external_generator = True
        self.credentials_exported = False
        try:
            for slot, session in enumerate(self.sessions):
                cert, key = session.base / 'client.crt', session.base / 'client.key'
                # Fresh per-worker self-signed trust anchor, pinned via SSH.
                # Bind the inventory IP only, never a wildcard/public listener.
                subprocess.run(['openssl', 'req', '-x509', '-newkey', 'ec',
                    '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes', '-days', '1',
                    '-subj', '/CN=orishu-private-lab', '-addext', 'subjectAltName=IP:' + session.address,
                    '-addext', 'basicConstraints=critical,CA:FALSE', '-addext', 'keyUsage=critical,digitalSignature',
                    '-addext', 'extendedKeyUsage=serverAuth', '-keyout', str(key), '-out', str(cert)],
                    check=True, timeout=10, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL)
                key.chmod(0o600)
                address = f'[{session.address}]' if ':' in session.address else session.address
                session.client_tls = (f'{address}:{9440 + slot}', cert, key)
        except BaseException:
            self.stop()
            raise

    def credentials(self):
        require(not self.credentials_exported, 'credentials already exported')
        self.credentials_exported = True
        result = []
        for session in self.sessions:
            require(session.worker is not None and session.worker.poll() is None, 'worker not running')
            token = (session.base / 'state' / 'operator.token').read_bytes()
            cert = session.client_tls[1].read_bytes()
            require(len(token) == 64 and len(cert) < 16384, 'credential bounds')
            result.append({'endpoint': session.client_tls[0], 'token': token.decode('ascii'),
                           'certificate': cert.decode('ascii'), 'certificate_sha256': hashlib.sha256(cert).hexdigest()})
        return result

    def prepare(self, args):
        profile(args)
        require(args.get('global_workers', 16) == self.global_workers
                and len(args['nodes']) == len(self.sessions), 'network topology mismatch')
        require(args['cell'] not in self.cells and len(self.cells) < 12
                and time.monotonic() + 60 < self.deadline, 'network cell bound')
        self.cells.add(args['cell'])
        self.cell = args['cell']
        self.progress = {'cell': self.cell, 'stage': 'identities'}
        targets = []
        for slot, session in enumerate(self.sessions):
            view = session.cli(['cluster', 'info'])
            require(view['formationId'] == args['formation'] and view['sourceNodeId'] == args['nodes'][slot]
                    and view['nodes'] == view['alive'] == self.global_workers and view['introducerReady']
                    and not view['locked'], 'network pre-load identity')
            targets.append({'endpoint': session.client_tls[0], 'formation': args['formation'], 'node': args['nodes'][slot]})
        self.config = {'schema_version': 2, 'first_role': args['first_role'], 'targets': targets,
                       'clients_per_worker': args['clients_per_worker'], 'rate_per_worker': args['rate_per_worker']}
        if self.versioned:
            self.config.update(schema_version=4, global_workers=self.global_workers)
        self.processes = [watchdog_child(s.worker, s.base / 'bin' / 'orishu-worker') for s in self.sessions]
        self.processes.append(Process(os.getpid()))
        self.threads = [len(list(Path(f'/proc/{p.pid}/task').iterdir())) for p in self.processes]
        placement = placement_snapshot(self.processes[:-1], self.interface) if self.experiment['placement'] else None
        if placement is not None:
            require(all(row['selected_device_visible'] for row in placement['sockets']),
                    'selected device missing from live sockets')
        self.progress['stage'] = 'warmup'
        return {'prepared': True, 'cell': self.cell, 'config': self.config,
                'probe_sha256': args['probe_sha256'], 'threads': self.threads, 'placement': placement}

    def request(self, request):
        if request == {'operation': 'network-credentials', 'arguments': {}}:
            return self.credentials()
        return super().request(request)
