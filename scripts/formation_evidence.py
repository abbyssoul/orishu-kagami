"""Bounded, private failure evidence for the real formation process harness.

Never archive a worker's state directory or command arguments. Raw process
output stays in a bounded memory tail until failure, then is scrubbed before
writing. This is test infrastructure, not worker telemetry or durable audit.
"""

import base64
import json
import os
import re
import subprocess
import tempfile
import threading
from collections import deque
from pathlib import Path

LOG_LIMIT = 64 * 1024
CLI_LIMIT = 4096
REDACTED = "[redacted]"


class Tail:
    def __init__(self):
        self.lock = threading.Lock()
        self.data = bytearray()
        self.truncated = False

    def append(self, data):
        with self.lock:
            self.data.extend(data)
            if len(self.data) > LOG_LIMIT:
                del self.data[:-LOG_LIMIT]
                self.truncated = True

    def snapshot(self):
        with self.lock:
            data = bytes(self.data)
            truncated = self.truncated
        # Never export a tail-cut secret or key fragment from the first line.
        if truncated:
            _, separator, data = data.partition(b"\n")
            if not separator:
                return "[overlong log line omitted]\n"
        return data.decode("utf-8", errors="replace")

    def clear(self):
        with self.lock:
            self.data.clear()


def redact(text, secrets):
    for secret in sorted(set(secrets), key=len, reverse=True):
        if secret:
            text = text.replace(secret, REDACTED)
    # Include unknown/rotated hex tokens and unlabelled encoded key material.
    # This deliberately redacts certificate/identity hex values in raw logs too.
    text = re.sub(r"[A-Fa-f0-9]{64,}", REDACTED, text)
    text = re.sub(r"[A-Za-z0-9+/=_-]{40,}", REDACTED, text)
    lines = []
    for line in text.splitlines():
        if re.search(r"private.?key|certificate|bearer|\btoken\b|-----BEGIN", line, re.I):
            lines.append("[credential-bearing log line omitted]")
        elif re.fullmatch(r"[\s\d,\[\]]+", line) or re.search(r"(?:\d{1,3},\s*){16}", line):
            lines.append("[numeric key material omitted]")
        else:
            lines.append(line)
    return "\n".join(lines) + "\n"


def credential_secrets(state):
    """Read only the harness-owned credential files, with production size caps.

    Invalid/unreadable existing identities cause failure-log export to fail
    closed. Missing files are normal when a process fails before initialization.
    """
    secrets = []
    for name, limit in [("operator.token", 65), ("identity.json", 65536)]:
        path = state / name
        try:
            with path.open("rb") as stream:
                data = stream.read(limit + 1)
        except FileNotFoundError:
            continue
        if len(data) > limit:
            raise ValueError("credential evidence size limit")
        if name == "operator.token":
            token = data.decode("ascii").strip()
            if not re.fullmatch(r"[a-f0-9]{64}", token):
                raise ValueError("invalid credential for evidence redaction")
            secrets.append(token)
        else:
            identity = json.loads(data)
            if identity["version"] != 1:
                raise ValueError("invalid identity for evidence redaction")
            for field in ("private_key", "certificate"):
                value = identity[field]
                if not isinstance(value, list) or not value or any(type(byte) is not int for byte in value):
                    raise ValueError("invalid identity bytes for evidence redaction")
                raw = bytes(value)
                secrets.extend([raw.hex(), base64.b64encode(raw).decode("ascii"),
                                json.dumps(value), json.dumps(value, separators=(",", ":"))])
    return secrets


class Evidence:
    def __init__(self, scenario):
        self.scenario = scenario
        self.processes = []
        self.recent = deque(maxlen=16)

    def start(self, arguments, environment, state, replace=None):
        if replace is None and len(self.processes) >= 3:
            raise ValueError("formation evidence process limit")
        tail = Tail()
        if replace is not None:
            if not isinstance(replace, int) or not 0 <= replace < len(self.processes):
                raise ValueError("invalid logical worker slot")
            old, old_state, tail, old_thread = self.processes[replace]
            if old.poll() is None or old_state != Path(state):
                raise ValueError("restart requires an exited process and the same state directory")
            old_thread.join(timeout=1)
            if old_thread.is_alive():
                raise ValueError("old process output pipe has not closed")
            tail.append(b"\n[harness: explicit process restart]\n")
        process = subprocess.Popen(arguments, env=environment,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

        def drain():
            try:
                while data := process.stdout.read1(4096):
                    tail.append(data)
            finally:
                process.stdout.close()

        thread = threading.Thread(target=drain, daemon=True)
        try:
            thread.start()
        except BaseException:
            process.kill()
            process.wait(timeout=3)
            process.stdout.close()
            raise
        entry = (process, Path(state), tail, thread)
        if replace is None:
            self.processes.append(entry)
        else:
            self.processes[replace] = entry
        return process

    def observe(self, index, verb, result):
        # Token export is never retained, even if its command reports failure.
        if verb == "token":
            return
        self.recent.append({"worker": index, "verb": verb,
                            "exit": result.returncode,
                            "stdout": self._bounded(result.stdout),
                            "stderr": self._bounded(result.stderr)})

    @staticmethod
    def _bounded(data):
        if len(data) > CLI_LIMIT:
            return "[oversized CLI output omitted]"
        return data.decode("utf-8", errors="replace")

    def save_failure(self, error):
        directory = Path(tempfile.mkdtemp(prefix="orishu-formation-failure-"))
        os.chmod(directory, 0o700)
        secrets = []
        safe = True
        for _, state, _, _ in self.processes:
            try:
                secrets.extend(credential_secrets(state))
            except (OSError, ValueError, KeyError, TypeError):
                safe = False
        for index, (_, _, tail, _) in enumerate(self.processes):
            text = (redact(tail.snapshot(), secrets) if safe else
                    "[log omitted: credential redaction could not be established]\n")
            text = text.encode("utf-8")[:LOG_LIMIT].decode("utf-8", errors="ignore")
            self._write(directory / f"worker-{index}.log", text)
        report = {"schemaVersion": 1, "scenario": self.scenario,
                  "errorType": type(error).__name__,
                  "pids": [process.pid for process, _, _, _ in self.processes],
                  "exits": [process.poll() for process, _, _, _ in self.processes],
                  "credentialRedactionAvailable": safe,
                  "recentCli": self.recent if safe else []}
        report["recentCli"] = [dict(item, stdout=redact(item["stdout"], secrets),
                                    stderr=redact(item["stderr"], secrets))
                               for item in report["recentCli"]]
        self._write(directory / "report.json", json.dumps(report, indent=2) + "\n")
        return directory

    @staticmethod
    def _write(path, text):
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as stream:
            stream.write(text)

    def close(self):
        # Call after reaping children. Threads are bounded and daemonized so an
        # inherited pipe cannot indefinitely block harness cleanup.
        for _, _, tail, thread in self.processes:
            thread.join(timeout=1)
            tail.clear()
        self.recent.clear()
