#!/usr/bin/env bash
# Native debug binaries; no GUI, plugin inventory or external service required.
set -euo pipefail
task_cargo=${CARGO:-cargo}
task_worker=$("$task_cargo" build --locked --offline -p orishu-worker --bin orishu-worker --message-format=json-render-diagnostics | python3 -c '
import json, sys
executables = []
for line in sys.stdin:
    item = json.loads(line)
    if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "orishu-worker" and item.get("executable"):
        executables.append(item["executable"])
if len(executables) != 1:
    sys.exit("expected exactly one built worker executable")
print(executables[0])
')
ORISHU_TEST_WORKER="$task_worker" "$task_cargo" test --locked --offline -p kagami --test workload_cli actual_worker_accepts_kagami_cli_upload_and_preserves_history_after_restart -- --ignored --exact
ORISHU_TEST_WORKER="$task_worker" "$task_cargo" test --locked --offline -p kagami --test scientific_initialization physics_form_creates_saves_and_exports_a_new_experiment -- --exact
