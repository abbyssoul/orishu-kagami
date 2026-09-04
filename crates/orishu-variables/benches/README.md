# `orishu-variables` benchmarks

Two complementary measurements, following the split used by
`fieldcad-bench`'s `profile_scene` example: Criterion for wall-clock
throughput, a `dhat`-gated example for allocation counts. Criterion has no
notion of allocations; a counting/profiling allocator has no notion of
statistically sound timing. Neither substitutes for the other.

## Throughput: `cargo bench -p orishu-variables`

Criterion microbenchmarks in `variables.rs`, four groups:

- `define` — cost of [`VariablesSystem::define`] for `count` independent
  constant-expression variables in one namespace (the `flat` shape below).
- `value_flat` — cost of [`VariablesSystem::value`] over `count` independent
  variables, one call each, throughput in evals/sec.
- `eval_adhoc` — cost of [`VariablesSystem::eval`] parsing and evaluating a
  fresh two-token expression string per variable, `count` times — the
  REPL's actual per-line cost (parse + resolve), not just resolve.
- `value_chain` — cost of `value()` on the tail of a `depth`-long dependency
  chain (`v1 = v0 + 1`, `v2 = v1 + 1`, ...), isolating the cost of
  `value`'s recursive resolution (and its per-call `visiting: Vec`
  allocation) as chain depth grows.

HTML reports land in `target/criterion/`.

### Counts and depths

Defaults sweep `10, 100, 1_000, 10_000`, overridable via env vars:

| Env var                                   | Default            | Format                   | Applies to                        |
|--------------------------------------------|---------------------|---------------------------|------------------------------------|
| `ORISHU_VARIABLES_BENCH_COUNTS`         | `10,100,1000,10000` | comma-separated `usize`s | `define`, `value_flat`, `eval_adhoc` |
| `FIELDCAD_VARIABLES_BENCH_CHAIN_DEPTHS`   | `10,100,1000,10000` | comma-separated `usize`s | `value_chain`                     |

```sh
ORISHU_VARIABLES_BENCH_COUNTS=50,5000 cargo bench -p orishu-variables
```

A malformed value panics rather than falling back silently. Variables are
generated deterministically from their index — no `rand` dependency, and the
same count/depth always produces the same system, so results are comparable
across commits.

## Allocations: `examples/profile_variables.rs`

```sh
cargo run --release -p orishu-variables --example profile_variables --features dhat
```

Runs the same flat-define, flat-value, chain-define, and chain-value-tail
phases as the Criterion groups above (fixed at 10,000 variables/chain depth),
printing each phase's wall-clock time plus, with `--features dhat`, the
allocation delta ([`dhat::HeapStats`] snapshot before/after) normalized per
unit of work — one `(blocks, bytes)` figure per variable defined, or per
`value()` call. Also writes a full `dhat-heap.json` call-site profile,
viewable at <https://nnethercote.github.io/dh_view/dh_view.html>, for
tracking down *which* allocation dominates rather than just how many there
are.

Without `--features dhat` it still runs (useful as a quick smoke check) but
reports zero allocations, since no counting allocator is installed.

## Adding a new benchmark

For a Criterion throughput group, follow the existing shape: a
deterministic `build_*_system(count)` helper, a `group.throughput(...)` call
sized to what one iteration actually does (definitions, evals, or chain
depth — pick whichever number the group's cost scales with), and
`group.bench_with_input` keyed by that count. Prefer plain `b.iter(...)`
over `b.iter_batched` when the operation doesn't mutate the system (`value`,
`eval`); reach for `iter_batched` only when it does (`define`), so setup
isn't counted as part of the timed cost.

For a new allocation phase, add a `report_phase(label, unit_count, || {
...})` call in `profile_variables.rs` — it prints the wall-clock time
unconditionally and the allocation delta only under `--features dhat`.
