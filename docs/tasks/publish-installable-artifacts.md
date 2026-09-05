# Publish installable Orishu Kagami artifacts

Status: **ready for packaging hardening and test scaffolds; publication is
gated by Milestone 8 product readiness**

Roadmap package: **P-INSTALL**

Decision: [ADR 0003](../adr/0003-single-binary-distribution.md)

User contract: [Installing Orishu Kagami](../install.md)

## Outcome

Operators, researchers, and plugin developers can obtain versioned Orishu
Kagami programs through explicitly supported distribution channels, verify
their integrity, install them on clean supported systems, and complete a tested
first-run journey without repository-private knowledge.

The public installation guide reports exactly which artifacts are published,
which platforms and versions are supported, what each package installs, and
how upgrades and removal affect configuration, state, credentials, plugins,
and running services. Packaging does not silently change security defaults or
create a second runtime/configuration model.

## Current state

The repository already contains useful but incomplete scaffolding:

- a tag/manual GitHub Actions workflow builds one Linux x86-64 tar archive;
- a multi-target Dockerfile builds `orishu-worker`, `orishuctl`, and the
  placeholder `orishu-monitor`;
- `cargo-deb` metadata exists for those three binaries;
- a hardened systemd unit and example worker configuration exist;
- `make install-worker` and `make install-ctl` install from a checkout; and
- ADR 0003 requires self-sufficient binaries and one worker runtime binary per
  node.

None of these is currently a supported published installation method. Known
gaps include:

- release archives have no published checksum/signature/SBOM contract or clean
  installation journey;
- the worker container defaults to a TCP listener without provisioning the TLS
  material that runtime validation requires;
- no registry image, Homebrew formula, crates.io package set, or Kagami native
  installer is published;
- `orishu-monitor` is included in archive/container/Debian scaffolding while
  remaining a placeholder;
- Debian metadata currently enables and starts the worker automatically, while
  historical user documentation promised an explicit first start; and
- upgrade, rollback, removal, persisted-schema, credential, protocol, plugin,
  and rolling-cluster compatibility are not yet defined and tested.

The Rust 1.97 source/CI/container version alignment and current repository links
are baseline corrections, not completion of this task.

## Required decisions before publication

Record the following choices in this task or a focused ADR if their operational
cost proves difficult to reverse:

1. Supported operating systems, architectures, minimum platform versions, and
   which artifacts are release guarantees versus best-effort development
   outputs.
2. Preferred native installation path per supported platform and the role of
   release archives, Cargo, Homebrew, Debian, and OCI images.
3. Whether Debian installation enables and starts `orishu-worker`, only enables
   it, or leaves both explicit. Reconcile `package.metadata.deb.systemd-units`
   with the documented first-run workflow and test fresh install, upgrade,
   failure, and unattended installation.
4. Whether `orishu-monitor` has reached a useful release surface. If not,
   remove it from every published bundle/package/image rather than shipping a
   misleading placeholder.
5. Artifact naming, version mapping across workspace packages, checksums,
   signatures/attestations, SBOM format, provenance, retention, and release
   rollback policy.
6. Supported upgrade and rolling-cluster compatibility windows for protocols,
   persisted state, workload schemas, experiment/catalog files, and plugins.
7. Kagami packaging formats, desktop integration, GPU/runtime prerequisites,
   signing/notarization, and supported launch environments.

Snap is not in the current roadmap. Do not restore historical Snap instructions
unless it is deliberately added as a supported channel with an owner and tests.

## Implementation slices

### 1. Define and continuously verify the release matrix

- Turn `docs/install.md` into the source of truth for supported channels,
  platforms, architectures, artifacts, capabilities, and known exclusions.
- Keep a machine-readable release matrix consumed by CI where practical; avoid
  duplicating versions and artifact names across shell snippets.
- Pin the compiler to `rust-toolchain.toml` or derive it consistently in CI and
  containers so another toolchain drift cannot recur.
- Add checks for stale repository URLs, required package files, artifact names,
  licenses, notices, and public documentation links.
- Distinguish build success from install, launch, first-run, upgrade, and
  uninstall success.

### 2. Harden release archives

- Produce one deterministic, versioned archive per supported target with only
  binaries that meet their release gate.
- Include README/install guidance, license/notices, checksums, build provenance,
  and the selected SBOM/signature material.
- Test extraction into a path containing spaces, execution without the source
  tree, `--version`/`--help`, config-free startup where ADR 0003 requires it,
  read-only/no-home behavior where applicable, and clean failure on an
  unsupported platform.
- Ensure tag versions, Rust package versions, displayed versions, archive
  names, and release metadata agree.

### 3. Make Cargo installation real

- Decide which crates are independently published and in what order; verify all
  path dependencies, package metadata, README links, included files, and
  licenses with `cargo package` before publication.
- Test `cargo install orishu-worker`, `orishuctl`, and every other promised
  binary from the packaged crate rather than the workspace checkout.
- Preserve ADR 0003: installed binaries cannot require files that Cargo does
  not install. Optional service/config examples belong in native packages and
  documentation, not as hidden runtime prerequisites.
- Do not imply that the existing `orishu` crates.io badge proves publication of
  every application package.

### 4. Complete Linux native packaging

- Resolve the Debian enable/start policy before changing it, then align
  `Cargo.toml`, maintainer scripts, systemd unit, install guide, and tests.
- Test package install in a clean supported Debian/Ubuntu environment, file
  ownership/modes, dynamic service user, state/runtime directories, config
  validation, credential bootstrap, start/restart/stop, upgrade, purge, and
  failed configuration.
- Never overwrite user configuration or private state silently. Define conffile
  behavior and which credentials/state survive uninstall versus purge.
- Validate local-socket permissions and remote TLS/operator-auth paths. Package
  defaults must not expose an unauthenticated network listener.
- Add the accepted packages to release CI and publish them with the same
  integrity and provenance material as archives.
- Add and test the planned Homebrew path only for binaries/platforms the
  support matrix actually promises.

### 5. Publish a secure worker container

- Replace the current invalid default (`0.0.0.0` TCP without TLS material) with
  a tested secure first-run contract. Prefer a useful local/config-driven mode
  or require explicit TLS/secret mounts with an actionable startup message; do
  not weaken runtime TLS validation for container convenience.
- Keep the runtime non-root, minimal, pinned by digest in tests, and free of
  embedded credentials. Define writable state/config mount points and
  read-only-root behavior.
- Add health behavior only after the observability/probe contract lands; do not
  invent a container-only authority or readiness definition.
- Test signal handling, graceful termination, restart with persisted identity,
  read-only config, volume ownership, resource limits, and a multi-container
  formation journey.
- Publish version and immutable-digest tags to the selected registry with SBOM,
  provenance, and vulnerability scanning. Mutable convenience tags are never
  workload or scientific provenance.

### 6. Package Kagami for researchers

- Select and implement native packages for supported desktop platforms,
  including renderer/window-system runtime dependencies, icons, desktop entry,
  file associations only where accepted, signing/notarization, and clean
  uninstall behavior.
- Test first launch on clean systems and software-rendering/unsupported-GPU
  diagnostics. Packaging must not claim remote runs, bundled plugins, or file
  formats before their product milestones are accepted.
- Ship built-in and third-party plugins through the same accepted plugin
  contract; installation layout must not grant built-ins hidden privileges.

### 7. Prove persona-based installation journeys

- Operator: install the preferred worker and CLI artifacts on clean nodes, form
  and inspect a cluster, exercise least-privilege administration, restart, and
  follow the documented recovery/uninstall path.
- Researcher: install Kagami, author/open a supported experiment, submit a
  workload, and inspect live and persisted results once those milestones land.
- Plugin developer: install the public SDK/conformance inputs, build/package a
  plugin without repository-private paths, install it into Kagami, and execute
  its pinned workload component through Orishu.
- Run small installation smoke journeys in release CI. Larger multi-node,
  numerical, scaling, and observability evidence may invoke their owning tasks
  but must use the exact candidate artifacts being released.

### 8. Publish and document releases

- Make the release workflow build once and promote the verified bytes; do not
  rebuild different bytes for each channel without explicit provenance.
- Require all platform/package tests, security scanning, compatibility checks,
  integrity metadata, and release notes before publication.
- Update `docs/install.md` from planned to supported only for artifacts that
  passed their clean-environment journey.
- Publish upgrade, rollback, removal, vulnerability-reporting, and support
  policy beside each release.

## Acceptance criteria

- The Rust compiler version, package versions, displayed versions, CI images,
  Docker builder, lockfile expectations, and documentation agree.
- Every artifact named as supported in `docs/install.md` is actually published,
  integrity-verifiable, installable in a clean supported environment, and
  exercised through its documented first-run journey.
- Unsupported or merely scaffolded methods remain visibly labelled and contain
  no copy-paste command implying a nonexistent download or registry package.
- A fresh operator can install the preferred worker/CLI distribution and form
  and inspect a cluster without the source repository.
- The worker binary remains one self-contained runtime. Archive, Cargo, native,
  and container forms expose the same configuration and authority semantics.
- Debian enable/start behavior is decided, implemented, documented, and tested;
  package installation never creates an unintended insecure listener.
- The worker OCI image starts according to its documented secure contract and
  passes non-root, secret-mount, persistence, signal, and formation tests.
- `orishu-monitor` is either useful and tested or absent from all published
  artifacts and install instructions.
- Kagami installs and launches on every claimed platform and completes the
  accepted researcher journey with truthful GPU/runtime diagnostics.
- Upgrades preserve or explicitly migrate supported configuration, state,
  credentials, artifacts, experiments, catalogs, and plugins; unsupported
  versions fail with actionable guidance.
- Release archives and packages carry checksums, provenance, the selected
  signature/attestation and SBOM, license/notices, and no credentials or private
  local paths.
- The release workflow cannot publish when artifact, package, install,
  compatibility, security, documentation, or required persona-journey checks
  fail.

## Parallel work and dependencies

- Archive reproducibility, Cargo packaging checks, Debian lifecycle tests,
  container hardening, Kagami packaging research, and installation-document
  scaffolding can proceed in parallel before feature freeze.
- Publication waits for the stable binaries and persona journeys named by
  Milestone 8. Do not use packaging completion to imply simulation or UI
  feature completion.
- Coordinate worker packages with configuration, N-FORMATION,
  P-OBSERVABILITY/P-OBS-DOCS, and compatibility work.
- Coordinate Kagami installers with K-DOCUMENT, K-RUN, V-LIVE/V-REPLAY,
  X-PLUGIN/X-BUILTINS, and platform renderer tests.
- Release engineering owns shared versioning, artifact naming, signing, and
  promotion so parallel packaging changes cannot publish incompatible bytes.

## Non-goals

- Publishing an artifact before its documented product behavior is accepted.
- Treating a locally built image or package as a supported release.
- Weakening TLS, authentication, filesystem permissions, sandboxing, or
  workload verification to simplify installation.
- Restoring Snap solely because the historical repository mentioned it.
- Turning Orishu into a Kubernetes operator or requiring Kubernetes to deploy
  a worker.
- Shipping `orishu-monitor` while it remains a misleading placeholder.
