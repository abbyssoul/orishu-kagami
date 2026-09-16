# Runtime ABI fixtures

`field.component.wasm` and `dynamics.component.wasm` are generated from the
`../guest` workspace (root field package and `dynamics` member) with Rust 1.97 and pinned
`wit-bindgen` 0.57.1, wrapped using `wit-component` 0.254.0 by the host's
`componentize` example. They are ABI/adversarial fixtures, not Newtonian gravity
or a physical integrator. The Dynamics fixture's byte IDs and history counters are
test-only schemas; they must not enter workload/admission formats.
The host tests must run against actual Component bytes, not a native substitute.

Rebuild to a new output path and review the byte change before replacing this
fixture. Run both host tests and the standalone guest's format/lint checks.
The nested Cargo.lock pins the independently developed plugin toolchain. Build and
lint with `--workspace` to include both independently compiled kernel artifacts.

`euler.component.wasm` is different: it is a real classical symplectic Euler
implementation built from the separate [reference plugin workspace](../../../../plugins/reference/README.md).
Its numerical tests use the standard scientific packets, not the test-only
byte-counter schema. Follow that workspace's locked build/wrapping instructions
when regenerating it. This artifact is not yet an installable plugin release or
a complete admitted workload, and it makes no Newtonian/relativistic claim.
