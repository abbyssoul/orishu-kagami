# Engineering guidelines

## Design

- Keep Orishu authoritative for accepted workloads and simulation execution.
- Keep Kagami authoritative for editable experiment intent and presentation
  state only.
- Prefer deep modules: a small interface should hide meaningful implementation
  complexity and be the surface used by tests.
- Introduce a seam when at least two adapters really exist, not for hypothetical
  future variation.
- Keep shared numerical kernels independent of UI and cluster runtime code.
- Record costly design decisions in `docs/adr/`.

## Correctness

- Every behavioral change needs relevant tests.
- Preserve deterministic fixed-step simulation semantics.
- Use SI quantities at persisted, protocol, and numerical module interfaces.
- Attach provenance, precision, units, validity, and simulation identity to
  observations.
- Validate candidates completely before replacing authoritative state.
- Avoid allocation in per-step, per-sample, and per-frame hot paths unless
  measurements justify it.

## Security and reliability

- Treat all network data as hostile, including messages from known nodes.
- Bound payload size, collection counts, decompression, buffering, and work.
- Check lengths and offsets before allocation or slicing.
- Never mix chunks from different observation or artifact identities.
- Fuzz network-facing parsers and admission paths.

## Rust

- Format with `cargo fmt` and lint with Clippy using warnings as errors.
- Prefer explicit domain types over primitive identifiers and unitless numbers.
- Document public items and error modes.
- Do not add dependencies casually; explain their ownership and maintenance
  cost during review.
- Keep workspace dependency resolution reproducible through `Cargo.lock`.

See [development documentation](docs/development.md) for commands.
