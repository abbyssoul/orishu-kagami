#!/usr/bin/env python3
"""Require a healthy normal release check and the precise fault-feature guard."""

import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    command = [args.cargo, "check", "--locked", "--release", "-p", "orishu-worker",
               "--message-format=json"]
    root = Path(__file__).resolve().parent.parent
    normal = subprocess.run(command, cwd=root, capture_output=True, timeout=600)
    if normal.returncode:
        raise AssertionError("normal release check failed; fault exclusion is not established")
    fault = subprocess.run(command + ["--features", "formation-fault-test"],
                           cwd=root, capture_output=True, timeout=600)
    errors = []
    for line in fault.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-message" and message["message"]["level"] == "error":
            errors.append((message["target"]["name"], message["message"]["message"]))
    assert fault.returncode != 0, "release accepted formation-fault-test"
    assert errors == [("orishu_worker", "formation-fault-test is forbidden in release builds")], \
        "release failed for an unexpected reason; fault exclusion is not established"
    print("PASS: normal release checks; fault release rejected by the intended compile-time guard")


if __name__ == "__main__":
    main()
