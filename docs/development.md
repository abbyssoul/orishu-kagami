# Development

## Prerequisites

- Rust 1.97, installed with `rustup`.
- GNU Make.
- Linux window-system development packages listed in the root README when
  building Kagami on Linux.
- Podman or Docker only when building Orishu container images. Make uses
  Podman by default; pass `DOCKER=docker` to use Docker.

## Root commands

The Makefile is the stable developer and CI interface:

| Command | Purpose |
| --- | --- |
| `make build` | Build every workspace member using the lockfile. |
| `make test` | Test every target in every workspace member. |
| `make fmt-check` | Verify Rust formatting. |
| `make lint` | Run Clippy with warnings denied. |
| `make docs` | Build Rust documentation without dependencies. |
| `make docs-check` | Check required public files and local Markdown links. |
| `make check` | Run formatting, linting, tests, doctests, and docs checks. |
| `make ci` | Run the complete local equivalent of the main CI gate. |

Use `ARGS` to pass runtime arguments, for example:

```sh
make run-kagami ARGS="--exit-after 5"
make run-worker ARGS="--help"
```

## Benchmarks

Each performance-bearing crate has a synthetic, IO-free Criterion benchmark.
`make bench` runs every crate microbenchmark; each also has an individual target:

| Command | Crate benchmark |
| --- | --- |
| `make bench` | All of the microbenchmarks below. |
| `make bench-workload` | Canonical encode, digest, decode, and closure verification. |
| `make bench-plugin` | Bulk-IO packet encode, read, and iterate. |
| `make bench-runtime` | The deterministic `ForceReducer` reduction. |
| `make bench-document` | The validated `update` transition. |
| `make bench-catalog` | Catalog parse, resolve, evaluate, and materialise. |
| `make bench-variables` | Variable definition and expression evaluation. |
| `make bench-membership` | The membership transition core. |

`make bench-formation` stays separate. It runs the membership benchmark and the
heavier `orishu-worker` formation integration benchmark.

Use `ARGS` to pass Criterion flags or a benchmark-name filter, for example:

```sh
make bench-plugin ARGS="read"
make bench ARGS="--warm-up-time 0.5 --measurement-time 2"
```

Per-benchmark sizes are overridable through each crate's own environment
variables, which each crate README documents (for example
`ORISHU_WORKLOAD_BENCH_COMPONENTS` or `KAGAMI_DOCUMENT_BENCH_BATCH`).

Criterion measures wall-clock time only. To measure allocation counts and bytes,
run the paired `dhat`-gated profiling example. Each crate that has one documents
the exact command in its README, for example:

```sh
cargo run --release -p orishu-workload --example profile_workload --features dhat
cargo run --release -p orishu-plugin --example profile_plugin --features dhat
```

## Change expectations

- Add tests through the module interface for every behavioural change.
- Update `CONTEXT.md` when introducing or sharpening product language.
- Record expensive-to-reverse design decisions in `docs/adr/`.
- Keep the root lockfile updated and commit it with dependency changes.
- Test network parsers and allocation paths with malformed and oversized input.
- Keep GUI presentation frames independent from simulation steps.

Kagami windowed behaviour still needs manual verification on a real graphics
stack. Use `make smoke-kagami` first to exercise device creation, shaders, the
render pipeline, and offscreen submission without opening a window.
