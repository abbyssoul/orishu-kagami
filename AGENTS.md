# Orishu Kagami — agent guide

Orishu Kagami is one product for authoring, executing, and observing scientific
simulations. Orishu and Kagami are roles inside that product, not independent
platforms to reconnect with translation layers.

This file applies to the whole repository. A more specific `AGENTS.md` may add
or override guidance for its subtree.

## Start here

1. Read `README.md` for current product status and entry points.
2. Read `CONTEXT.md` for canonical terminology, ownership, and invariants.
3. Read `docs/architecture.md` and the relevant record in `docs/adr/` before
   changing an authority, protocol, persisted format, or module boundary.
4. Read the relevant task in `docs/tasks/` when implementing planned work.
   Tasks describe desired work, not necessarily current behavior.
5. Inspect the public types, callers, and tests in the affected crate. The
   source is authoritative about what is implemented today.
6. Follow `GUIDELINES.md`, `docs/Coding style.md`, and, for hot paths,
   `docs/high-performance-rust.md`.

Use `rg` and `rg --files` for repository discovery. Check `git status` before
editing: the worktree may contain ongoing user or agent work, which must be
preserved rather than reformatted or reverted incidentally.

## Product roles

- **Cluster operators** provision and operate Orishu workers with
  `orishuctl` and `orishu-monitor`.
- **Researchers** use Kagami to author experiments, submit immutable workloads,
  and inspect live or persisted runs.
- **Simulation plugin developers** use external IDEs and build tools to create
  declarative authoring schemas and sandboxed workload components. Kagami
  manages finished plugins; it is not a code editor or compiler.

One person or agent may perform several roles, but do not collapse their tools,
permissions, or authority boundaries. See `docs/simulation-plugins.md`.

## Repository map

- `apps/kagami`: native Iced authoring and visualization client. Its current
  scene tree is prototype UI state, not the authoritative experiment model.
- `apps/orishu-worker`: cluster node/runtime process.
- `apps/orishu-ctl`: scriptable cluster operator client; binary name
  `orishuctl`.
- `apps/orishu-monitor`: interactive terminal operator client; currently a
  placeholder.
- `crates/orishu`: shared Orishu domain models and client interfaces used by
  applications. Persisted and protocol types currently live here.
- `crates/orishu-identity`: the shared formation/node identity contract
  (`FormationId`, `NodeId`, labels, certificate fingerprints, version
  ordering, membership tombstones). Deliberately dependency-poor so that both
  the networking client and the sans-IO membership core can depend on it.
- `crates/orishu-resource`: the shared typed resource envelope — the
  `apiVersion`/`kind`/`metadata`/`spec`/optional-`status` shape and its
  bounded discriminator, used by both Orishu's resources and Kagami's object
  templates. Structural only: it owns no metadata schema, identity,
  validation, codec, or IO, and depends on `serde` alone; a test enforces that
  against the resolved dependency graph. See `docs/resource-envelope.md`.
- `crates/orishu-membership`: sans-IO functional core for cluster membership —
  admission, SWIM, gossip merge, and anti-entropy. It must never acquire a
  networking, async-runtime, clock, filesystem, TLS, or RNG dependency; a test
  enforces that against the resolved dependency graph.
- `crates/orishu-variables`: the shared idCVar-inspired variables and
  expressions engine — namespaced variables, retained expression source,
  stable handles, and cycle-detecting resolution. Deliberately generic: it
  knows nothing of documents, catalogs, dimensions, or units.
- `crates/kagami-catalog`: Kagami's object-template catalog — format, bounded
  loading, the read-only variable projection, safe writes, self-contained
  instantiation, and the catalog authority. UI and MCP are adapters over that
  authority; a test enforces that it acquires no UI, transport, or Orishu
  runtime dependency.
- `crates/kagami-renderer`: Kagami's Iced/wgpu rendering boundary. It owns GPU
  presentation mechanics, never authoritative experiment or simulation state.
- `docs/adr`: accepted costly-to-reverse decisions.
- `docs/tasks`: bounded implementation work derived from accepted design.
- `docs/user-stories`: user-facing outcomes, not low-level implementation
  specifications.
- `etc`: worker deployment configuration.
- `etc/catalogs`: the object catalogs shipped as examples. Kagami installation
  data, never Orishu workload resources; the catalog integration tests load
  these exact files.
- `scripts`: repository validation and automation.

The workspace currently discovers `apps/*` and `crates/*`. Do not create a new
crate merely to make a conceptual noun visible. Keep a module in its only real
caller until a second consumer or a deep interface justifies extraction.

## Non-negotiable authority boundaries

### Experiment authoring

- Kagami's document authority is the only writer of editable experiment
  intent. UI actions, MCP calls, undo/redo, file operations, and a future remote
  collaboration adapter submit the same typed commands.
- A command is proposed intent. The authority validates it completely and
  atomically accepts a new revision or rejects it with a domain reason.
  Successful transport is not command acceptance.
- The document authority is a logical boundary, not a UI object. Keep its
  model, commands, revisions, and outcomes serializable and independent of
  windows, widgets, renderer state, and process-local pointers so it can later
  run in a headless service.
- Draft collaboration is initially file-based. Do not introduce live
  multi-writer, shared undo, remote MCP, or CRDT semantics without a new
  protocol/security decision.

### Runs and observations

- A local solver or Orishu owns a run. Run observations never mutate the
  experiment. Adopting computed state is an explicit authoring command that
  records provenance and creates a normal revision.
- Orishu is client-server from outside and peer-to-peer inside. A client entry
  node relays the state collectively committed for a workload identity, epoch,
  and simulation boundary; it is not an independent source of truth.
- Multiple observers own independent subscriptions, baselines, cameras,
  playback cursors, and rates. Presentation state is client-local.
- Live observation deltas name explicit compatible baselines. An unusable
  baseline recovers through a complete snapshot. Only supersedable observation
  projections may be coalesced or skipped.
- Halo exchange, step votes/commits, commands and decisions, workload
  artifacts, checkpoints, and other correctness-bearing data use reliable,
  identified, bounded transfer. Never apply “latest state wins” to them.
- Interpolation or extrapolation is presentation-only. It cannot become a
  result, checkpoint, halo, simulation input, or authored value implicitly.

### Workloads and plugins

- A workload is the immutable root manifest plus the complete closure of
  digest-addressed code and inputs. Location, cache, registry, archive, peer,
  and compression details are distribution, not identity.
- A cluster runs at most one workload at a time. Orishu is a decentralized
  execution engine, not a general scheduler or job queue.
- Workload components are untrusted WebAssembly Component guests behind the
  versioned, capability-limited lifecycle. The runtime owns partitioning,
  networking, time, barriers, storage, provenance, and commit.
- A **simulation plugin** is an authoring-time package: declarative Kagami
  schemas plus a pinned workload component. It is not a native library loaded
  into Kagami or `orishu-worker`. Built-in and third-party plugins use the same
  public validation, compilation, sandbox, and workload contracts.
- A **numerical kernel** is implementation used inside a workload component.
  An **object template** is reusable authored data. Neither a template name,
  filename, plugin installation path, URL, nor mutable tag may secretly select
  executable physics.
- Orishu validates the accepted workload independently. Kagami's prior
  validation is useful feedback, never runtime authority.

### Time, quantities, and provenance

- Simulation time advances only through accepted fixed steps and is distinct
  from wall-clock time, render frames, network timing, and playback speed.
- Use dimension-aware SI quantities at persisted, protocol, workload, and
  numerical boundaries. Preserve authored expressions and units where the
  schema declares an expression-capable value.
- Observations identify workload/run, epoch, committed boundary, schema/model,
  precision, dimensions/units, completeness, validity, and relevant execution
  provenance. Never mix chunks or deltas across identities.

## Functional core and IO shell

Prefer pure domain transitions over hidden mutation. UI, network, MCP,
filesystem, timers, and solver integration are imperative adapters that decode
or perform IO and feed typed messages/effect outcomes into the core.

Use model/message/update when several IO sources mutate correctness-relevant
state and replayable transitions buy clarity. Do not impose it on a small
single-owner helper. Keep effects explicit, and test the same transition or
public interface that production uses.

## Security and hostile inputs

- Treat every client, peer, plugin manifest, schema, workload component,
  archive, expression, and artifact as untrusted even when authenticated or
  signed.
- Bound bytes, nesting, collection counts, decompression, expression work,
  queues, baseline history, diagnostics, guest execution, and allocations
  before accepting data or committing state.
- Check lengths, offsets, digests, identities, schema versions, dimensions, and
  finite numeric values before slicing, allocation, cache admission, or state
  adoption.
- Do not put credentials in manifests, workload identity, run references,
  experiment files, logs, or presentation state.
- Network-facing parsers and admission paths need malformed, oversized,
  duplicate, stale, corrupt, and cross-identity tests; fuzz them when practical.

## Numerical and performance work

- Correctness precedes speed. Numerical changes need analytic, reference,
  convergence, determinism, or CPU/GPU parity evidence appropriate to risk.
- State expected complexity in domain terms such as objects, cells, samples,
  partitions, observers, or bytes.
- Do not allocate per step, sample, or frame without measured justification.
  Reuse caller-owned buffers and separate hot numeric state from cold metadata
  where access patterns support it.
- Authoring, persistence, simulation, rendering, and networking do not need one
  in-memory layout. Convert explicitly at boundaries and amortize conversions
  outside hot loops.
- Measure claimed performance improvements on representative workloads and
  report the metric being improved; a faster result with changed scientific
  semantics is not an optimization.

## Rust and dependency conventions

- The workspace uses Rust edition 2024 and the compiler selected by
  `rust-toolchain.toml` (currently Rust 1.97).
- Prefer explicit domain/newtypes over primitive strings, integers, and
  unitless floats at public boundaries. Keep fields private when constructors
  enforce invariants.
- Persisted, command, and wire types require explicit versioning and serde
  support. Document public items and error modes.
- Follow the affected module's established error style; preserve structured
  domain and protocol errors instead of flattening them into display strings.
- Prefer enums, generics, and composition. Use dynamic dispatch only at a real
  open implementation boundary.
- Put a dependency in `[workspace.dependencies]` when it is genuinely shared.
  Pin and justify new dependencies, consider their security/maintenance cost,
  and keep `Cargo.lock` reproducible.
- Keep unit tests near the code they exercise and add integration/wire tests
  when serialization, transport, process, or adapter behavior is the contract.
- Avoid unsafe code unless the benefit and invariants are explicit and covered
  by focused tests. Never infer wire or persisted layout from Rust memory
  layout.

## Documentation and planning

- Update `CONTEXT.md` when canonical terminology, ownership, or invariants
  change.
- Record costly, cross-cutting decisions in `docs/adr/` with context, options or
  decision rationale, consequences, status, and links from `docs/adr/README.md`.
- Promote accepted implementation work into `docs/tasks/` with outcome,
  current gap, bounded slices, acceptance criteria, and non-goals; update the
  task index. Do not describe planned behavior as already implemented.
- Keep user-facing concepts in `README.md` and the appropriate focused design
  document. Keep protocol wire details in protocol documents rather than ADRs
  or user stories.
- Reconcile edits across affected ADRs, architecture, protocols, tasks, and
  stories so the same term does not acquire incompatible meanings.
- Field CAD and earlier Orishu repositories are historical references. Reuse
  proven ideas deliberately through `docs/migration.md`; do not bulk-copy code,
  APIs, package boundaries, or terminology and call them authoritative here.

## Validation and handoff

Use the smallest set while iterating, then validate in proportion to risk:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
python3 scripts/check-docs.py
```

The repository shortcuts are:

```sh
make check
make build
make docs
```

For Kagami renderer or UI changes, also run `make smoke-kagami` where graphics
hardware is available and state any manual window verification still needed.
For protocol changes, exercise the real serialized/wire path, not only direct
method calls. For documentation-only changes, run `make docs-check` and inspect
new links; distinguish existing repository failures from failures introduced by
the change.

At handoff, lead with the result. Name files changed, tests/checks run, known
failures, compatibility or migration effects, and any required manual or
follow-up work. Never claim a check passed when it was skipped or failed.
