#!/usr/bin/env python3
"""Prepare a private Pi lab directory or emit bounded, read-only readiness JSON.

No package installation, service control, credential export or benchmark run.
Can be streamed to `python3 - check ...` over authenticated SSH.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import selectors
import shutil
import signal
import struct
import subprocess
import sys
import time

BINARIES = ('orishu-worker', 'orishu-worker-omitted', 'orishuctl', 'formation-telemetry-probe')
TOOLS = ('python3', 'openssl', 'ssh', 'rsync', 'ip', 'ping', 'timedatectl')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded_command(command, data=b'', timeout=15, cap=131072):
    """Bound both pipes, stdin progress and lifetime; kill only this child group."""
    require(len(data) <= 131072 and 0 < timeout <= 60 and 0 < cap <= 1048576, 'command bounds')
    child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, start_new_session=True)
    output = {'stdout': bytearray(), 'stderr': bytearray()}
    began = time.monotonic()
    sent = 0
    try:
        with selectors.DefaultSelector() as poll:
            for stream, name in ((child.stdout, 'stdout'), (child.stderr, 'stderr')):
                os.set_blocking(stream.fileno(), False)
                poll.register(stream, selectors.EVENT_READ, name)
            if data:
                os.set_blocking(child.stdin.fileno(), False)
                poll.register(child.stdin, selectors.EVENT_WRITE, 'stdin')
            else:
                child.stdin.close()
            while poll.get_map():
                remaining = timeout - (time.monotonic() - began)
                if remaining <= 0:
                    raise TimeoutError('command deadline')
                for key, _ in poll.select(min(remaining, 0.1)):
                    if key.data == 'stdin':
                        try:
                            sent += os.write(key.fd, data[sent:sent + 4096])
                        except BrokenPipeError:
                            sent = len(data)
                        if sent == len(data):
                            poll.unregister(key.fileobj)
                            key.fileobj.close()
                    else:
                        chunk = os.read(key.fd, 4096)
                        if not chunk:
                            poll.unregister(key.fileobj)
                            key.fileobj.close()
                        else:
                            output[key.data].extend(chunk)
                            require(sum(map(len, output.values())) <= cap, 'command output cap')
            code = child.wait(timeout=max(0.01, timeout - (time.monotonic() - began)))
        return {'exit': code, **{k: bytes(v).decode('utf-8', errors='replace') for k, v in output.items()}}
    except BaseException as error:
        # A child can exit while a grandchild still holds a pipe. On failure,
        # reap the whole owned group even when the group leader has exited.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=2)
        if isinstance(error, subprocess.TimeoutExpired):
            raise TimeoutError('command deadline') from error
        raise
    finally:
        if child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=2)
        for stream in (child.stdin, child.stdout, child.stderr):
            if not stream.closed:
                stream.close()


def read_text(path, cap=8192):
    try:
        with Path(path).open('rb') as stream:
            value = stream.read(cap + 1)
        require(len(value) <= cap, 'inventory file exceeds bound')
        return value.decode('utf-8', errors='replace').strip().strip('\x00')
    except OSError:
        return None


def root_path(value):
    root = Path(value)
    require(root.is_absolute() and '..' not in root.parts and len(root.parts) >= 3,
            'use an absolute dedicated lab path without traversal')
    require(len(str(root)) <= 240 and root.resolve() == root, 'root must not traverse symlinks')
    return root


def write_new_json(path, value):
    raw = (json.dumps(value, indent=2, allow_nan=False) + '\n').encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)


def prepare(root):
    root = root_path(root)
    require(os.geteuid() != 0, 'prepare as the dedicated non-root SSH account')
    require(root.parent.is_dir() and root.parent.stat().st_uid == os.geteuid(),
            'lab parent must be an existing directory owned by this account')
    root.mkdir(mode=0o700, exist_ok=False)
    for name in ('bin', 'runs', 'results'):
        (root / name).mkdir(mode=0o700)
    write_new_json(root / '.orishu-pi-lab.json', {'schema_version': 1, 'owner_uid': os.geteuid()})
    return {'schema_version': 1, 'kind': 'pi-lab-prepared', 'root': str(root)}


def artifact(path):
    if path.is_symlink() or not path.is_file():
        return {'present': False}
    size = path.stat().st_size
    require(size <= 256 * 1024 * 1024, 'artifact exceeds 256 MiB bound')
    with path.open('rb') as stream:
        header = stream.read(64)
        stream.seek(0)
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    arm64 = (len(header) == 64 and header[:6] == b'\x7fELF\x02\x01'
             and struct.unpack_from('<H', header, 18)[0] == 183)
    return {'present': True, 'bytes': size, 'sha256': digest, 'elf64_aarch64': arm64,
            'executable': os.access(path, os.X_OK)}


def check(root, interface):
    root = root_path(root)
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,14}', interface), 'invalid interface')
    errors, warnings = [], []
    if platform.system() != 'Linux' or platform.machine() != 'aarch64' or struct.calcsize('P') != 8:
        errors.append('64-bit aarch64 Linux required')
    if sys.version_info < (3, 11):
        errors.append('Python 3.11 or newer required')
    if os.geteuid() == 0:
        errors.append('run checks as the dedicated non-root account')
    marker = read_text(root / '.orishu-pi-lab.json', 1024)
    prepared = False
    try:
        prepared = json.loads(marker or '{}') == {'schema_version': 1, 'owner_uid': os.geteuid()}
    except ValueError:
        pass
    for path in (root, *(root / n for n in ('bin', 'runs', 'results'))):
        prepared &= path.is_dir() and not path.is_symlink()
        if path.is_dir():
            prepared &= path.stat().st_uid == os.geteuid() and path.stat().st_mode & 0o077 == 0
    if not prepared:
        errors.append('private lab directories are not prepared for this account')
    tools = {name: shutil.which(name) is not None for name in TOOLS}
    errors.extend('missing tool: ' + k for k, v in tools.items() if not v)
    net = Path('/sys/class/net') / interface
    link = {key: read_text(net / key, 128) for key in ('type', 'operstate', 'carrier', 'speed', 'duplex', 'mtu')}
    link['wireless'] = (net / 'wireless').exists()
    if link['carrier'] != '1' or link['type'] != '1' or link['wireless']:
        errors.append('selected interface must have a live wired link')
    addresses = []
    if tools['ip']:
        try:
            response = bounded_command(['ip', '-j', 'address', 'show', 'dev', interface], timeout=3, cap=16384)
            entries = json.loads(response['stdout']) if response['exit'] == 0 else []
            require(isinstance(entries, list) and len(entries) <= 16, 'interface inventory bound')
            for entry in entries:
                for address in entry.get('addr_info', []):
                    require(len(addresses) < 32, 'interface address bound')
                    addresses.append(address['local'])
        except (OSError, ValueError, TimeoutError, KeyError, TypeError, AttributeError):
            errors.append('interface addresses could not be inspected')
    if not addresses:
        errors.append('selected interface has no inspected IP address')
    clock = {'available': False}
    if tools['timedatectl']:
        try:
            value = bounded_command(['timedatectl', 'show', '-p', 'NTPSynchronized', '--value'], timeout=3, cap=4096)
            clock = {'available': value['exit'] == 0, 'synchronized': value['stdout'].strip() == 'yes'}
        except (OSError, ValueError, TimeoutError):
            pass
    if not clock.get('synchronized'):
        warnings.append('time synchronization unverified; inspect chronyc tracking before distributed windows')
    firmware = {}
    for command in ('measure_temp', 'get_throttled'):
        if shutil.which('vcgencmd'):
            try:
                result = bounded_command(['vcgencmd', command], timeout=3, cap=4096)
                firmware[command] = result['stdout'].strip() if result['exit'] == 0 else None
            except (OSError, ValueError, TimeoutError):
                firmware[command] = None
    if not firmware.get('get_throttled'):
        warnings.append('Pi firmware throttle/undervoltage status unavailable')
    elif firmware['get_throttled'] != 'throttled=0x0':
        warnings.append('firmware reports current or historical throttling/undervoltage; inspect bits before testing')
    policies = sorted(Path('/sys/devices/system/cpu/cpufreq').glob('policy*'))
    require(len(policies) <= 128, 'CPU policy inventory cap')
    governors = {p.name: read_text(p / 'scaling_governor', 128) for p in policies}
    binaries = {name: artifact(root / 'bin' / name) for name in BINARIES} if prepared else {}
    binary_ready = len(binaries) == len(BINARIES) and all(
        a.get('elf64_aarch64') and a.get('executable') for a in binaries.values())
    if not binary_ready:
        warnings.append('four source-built ARM64 artifacts are not staged; no binary was executed')
    disk = shutil.disk_usage(root if root.exists() else root.parent)
    return {'schema_version': 1, 'kind': 'pi-lab-readiness', 'root': str(root),
            'host': platform.node(), 'model': read_text('/proc/device-tree/model', 256),
            'architecture': platform.machine(), 'kernel': platform.release(),
            'python': platform.python_version(), 'os_release': read_text('/etc/os-release'),
            'cpu_count': os.cpu_count(), 'memory': read_text('/proc/meminfo'),
            'disk_free_bytes': disk.free, 'interface': interface, 'link': link, 'addresses': addresses,
            'tools': tools, 'clock': clock, 'firmware': firmware, 'governors': governors,
            'artifacts': binaries, 'errors': errors, 'warnings': warnings,
            'infrastructure_ready': not errors, 'artifacts_staged': binary_ready,
            'experiment_ready': False,
            'note': 'readiness only; remote experiment driver and hardware validation remain planned'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('prepare', 'check'))
    parser.add_argument('--root', required=True)
    parser.add_argument('--interface', default='eth0')
    args = parser.parse_args()
    os.umask(0o077)
    try:
        report = prepare(args.root) if args.action == 'prepare' else check(args.root, args.interface)
        print(json.dumps(report, allow_nan=False))
        return 0 if args.action == 'prepare' or report['infrastructure_ready'] else 2
    except (OSError, ValueError) as error:
        print(json.dumps({'schema_version': 1, 'kind': 'pi-lab-error', 'error': str(error)[:256]}))
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
