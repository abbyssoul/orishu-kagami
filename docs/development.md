# Development

## Prerequisites

- Rust 1.94, installed with `rustup`.
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
