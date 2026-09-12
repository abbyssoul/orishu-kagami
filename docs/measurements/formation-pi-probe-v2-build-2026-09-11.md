# ARM64 physical probe v2 build/staging — 2026-09-11

Status: **native build, four-node staging/startup and old-schema rejection
verified; no v2 warmup or timed-window result**.
Owner: [physical-host task](../tasks/implement-physical-formation-experiment.md).
Previous hardware profile: [v1 probe/warmup checkpoint](formation-pi-probe-2026-09-11.md).

Subsequent evidence: the [approved four-Pi pilot](formation-pi-pilot-2026-09-11.md)
verifies v2 warmup and a timed window with retained environment findings.
The build-only checkpoint below remains historical.

## Frozen build

Built natively on the first Pi 5 from HEAD
`9c11d9854dc22dd3f999571bed4424ac144a505a` plus the updated probe example.
Inspection confirmed that example was the only changed Rust/build input;
private inventories and Python/doc changes were not overlaid into the archive.
The archive digest matched after transfer before extraction into a new private
source directory, and the extracted probe source digest matched too.

| Artifact | SHA-256 |
| --- | --- |
| Source archive | `35e27d5661e29fba3a9439ee00b477ca484575fb13454d676811846064e6dbae` |
| Probe Rust source | `0d6a978ba5a1f9f00870e2b5fe9c965e48dc6fed8df58211db1e8d1c324973c4` |
| ARM64 v2 binary | `9e44762e7ae766881677fec865b23f7f76ed5025a3dc0e0a7913a16e0af1c9d3` |
| Cargo.lock | `861336bb203f3c45a23213194734bfea2b7eab1e89c5ed5b781705f34abef7ae` |
| Toolchain file | `9dee632f575fa73074c503286dbf3dda7c078b20c4dd55fa8b853eb6d6d6b210` |

Rust 1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`), LLVM 22.1.6,
target `aarch64-unknown-linux-gnu`. The command was the previous native
`cargo build --locked --offline --release -p orishu-worker --features
observability,otlp-tracing --example formation-telemetry-probe`, with
`CARGO_BUILD_JOBS=2` and the existing `target/pi-enabled` cache. Cargo reported
**8.82 seconds**. No compiler/package installation, remote checkout edit or
worker/load execution was involved.

The build source is retained at
`/home/soultaker/orishu-lab/probe-v2-build.2aCauS/source` on the build node.
Local archive, fetched binary, staging manifest and version-check receipts are
under `/tmp/orishu-pi-probe-v2.CO1wRa/`. These are lab artifacts, not a release.

## Installation and native checks

The harness now selects `bin/formation-telemetry-probe-node-v2`. On every Pi,
the binary was copied into a fresh private staging directory, its digest
verified, then installed with a no-overwrite hard link at that new filename.
The staging directories remain available and are recorded in the private
build manifest. Both older probe filenames remain intact with their original
hashes on all four nodes:

- Original `formation-telemetry-probe`: `651c59c31b521cd3371944a782de2af2b5315891306f12aa3b45b82b98965cf4`
- Earlier `formation-telemetry-probe-node`: `087e61e6af7e79f0afaf1e604742d91f7f1b190fa60f890362a74ff72868cbab`

`load-node --help` executed successfully on each Pi. A schema-1 configuration
then exited 1 on each node with the expected invalid-profile error and no
stdout/READY marker, proving rejection before warmup. No worker, client
request or timed load was started by this negative version check. The updated
input-2/output-5 profile still needs real warmup and timed execution evidence;
native startup alone does not verify the new clock/activity receipts.

The small harness filename change retains passing focused local tests and
documentation validation. The preceding source revision's 132 Python tests,
11 Rust probe tests and targeted Clippy remain its local validation checkpoint;
this build does not turn those tests into a hardware performance pass. M4,
reviewed timing policy and pilot allowance remain open.
