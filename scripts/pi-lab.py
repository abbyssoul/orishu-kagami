#!/usr/bin/env python3
"""Validate a 3/5-Pi inventory and collect readiness over strict, key-only SSH.

This does not deploy or run workers. No password, key or remote state is copied.
"""
import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex

from pi_lab_node import bounded_command, require, root_path, write_new_json


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def inventory(path):
    with Path(path).open('rb') as stream:
        raw = stream.read(16385)
    require(len(raw) <= 16384, 'inventory byte cap')
    value = json.loads(raw, object_pairs_hook=unique)
    require(isinstance(value, dict) and set(value) == {'schema_version', 'nodes'}
            and type(value['schema_version']) is int and value['schema_version'] == 1,
            'inventory schema')
    nodes = value['nodes']
    require(isinstance(nodes, list) and len(nodes) in (3, 5), 'use exactly 3 or 5 physical nodes')
    names, hosts, addresses = set(), set(), set()
    for node in nodes:
        require(isinstance(node, dict) and set(node) == {'name', 'ssh_host', 'peer_address', 'interface', 'root'}, 'node fields')
        require(all(isinstance(v, str) for v in node.values()), 'node fields must be strings')
        for field in ('name', 'ssh_host'):
            require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,62}', node[field]), 'unsafe node name or SSH alias')
        address = ipaddress.ip_address(node['peer_address'])
        require(not (address.is_loopback or address.is_unspecified or address.is_multicast
                     or address.is_link_local) and '%' not in node['peer_address'], 'use a routable lab unicast address')
        require(node['name'] not in names and node['ssh_host'] not in hosts and str(address) not in addresses,
                'each physical node needs a distinct name, alias and peer address')
        names.add(node['name'])
        hosts.add(node['ssh_host'])
        addresses.add(str(address))
        # Remote paths are validated lexically, never resolved against this host.
        root = Path(node['root'])
        require(root.is_absolute() and '..' not in root.parts and len(root.parts) >= 3
                and len(str(root)) <= 240 and not any(ord(c) < 32 for c in str(root)), 'unsafe remote root')
        require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', node['interface']), 'invalid interface')
    return value


def ssh_command(node):
    remote = shlex.join(['python3', '-', 'check', '--root', node['root'], '--interface', node['interface']])
    return ['ssh', '-T', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
            '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2',
            '-o', 'ClearAllForwardings=yes', '-o', 'ForwardAgent=no',
            '--', node['ssh_host'], remote]


def collect(value, output, runner=bounded_command):
    output = root_path(output)
    output.mkdir(mode=0o700, exist_ok=False)
    code = Path(__file__).with_name('pi_lab_node.py').read_bytes()
    require(len(code) <= 131072, 'remote script byte cap')
    write_new_json(output / 'inventory.json', value)
    results = []
    for node in value['nodes']:
        entry = {'name': node['name'], 'status': 'failed'}
        try:
            response = runner(ssh_command(node), code, timeout=60, cap=131072)
            entry.update(exit=response['exit'], stderr_sha256=hashlib.sha256(response['stderr'].encode()).hexdigest())
            report = json.loads(response['stdout'], object_pairs_hook=unique)
            require(isinstance(report, dict) and report.get('schema_version') == 1
                    and report.get('kind') == 'pi-lab-readiness', 'remote readiness schema')
            require(report.get('root') == node['root'] and report.get('interface') == node['interface'], 'remote target mismatch')
            write_new_json(output / (node['name'] + '.json'), report)
            require(node['peer_address'] in report.get('addresses', []), 'peer address is not assigned to selected interface')
            entry['status'] = 'ready' if response['exit'] == 0 and report.get('infrastructure_ready') is True else 'needs_setup'
        except (OSError, ValueError, TimeoutError, RecursionError) as error:
            entry['error_type'] = type(error).__name__
            if isinstance(error, ValueError):
                entry['error'] = str(error)[:160]
        results.append(entry)
    summary = {'schema_version': 1, 'kind': 'pi-lab-readiness-collection',
               'node_script_sha256': hashlib.sha256(code).hexdigest(), 'nodes': results,
               'all_infrastructure_ready': all(n['status'] == 'ready' for n in results),
               'experiment_run': False}
    write_new_json(output / 'summary.json', summary)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('validate', 'check'))
    parser.add_argument('--inventory', required=True, type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        value = inventory(args.inventory)
        if args.action == 'validate':
            print(json.dumps({'valid': True, 'nodes': len(value['nodes']), 'ssh_contacted': False}))
            return 0
        require(args.output is not None, 'check requires a fresh --output directory')
        result = collect(value, args.output)
        print(json.dumps(result))
        return 0 if result['all_infrastructure_ready'] else 2
    except (OSError, ValueError, RecursionError) as error:
        print(json.dumps({'error': str(error)[:256]}))
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
