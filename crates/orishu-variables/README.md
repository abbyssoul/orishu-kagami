# orishu-variables

The shared variables and expressions engine, in the idCVarSystem tradition. It
lets callers define namespaced variables bound to parsed math expressions, and
lets expressions reference other variables by their fully qualified
`namespace.name`. The crate knows nothing of CAD, solvers, documents, catalogs,
or schemas, and is deliberately generic. Both Kagami's catalog and Orishu's
workload fields depend on it.

## Public contract

| Type | Role |
| --- | --- |
| `CompiledExpression` | A parsed expression. `parse` uses the default bounds; `parse_bounded` takes caller-declared `Limits`. |
| `VariablesSystem`, `VariablesError` | The namespaced variable environment with cycle-detecting resolution. `default` uses the default bounds; `with_limits` takes the caller's. |
| `Quantity`, `Dimension`, `Unit`, `UnitError` | Evaluation produces a canonical-SI magnitude plus the dimension the arithmetic derived, so `1 kg + 1 m` is an error. A pure number is a dimensionless quantity. |
| `Name`, `Namespace`, `FQName`, `IntoName`, `InvalidName` | Validated name segments and fully qualified names. |
| `VariableId`, `VariableOptions` | Variable handles and declaration options. |
| `rewrite_symbols`, `rewrite_symbols_bounded` | Symbol rewriting over expression source. |
| `Limits`, `LimitKind`, `LimitError` | The caller-owned resource bounds and the structured refusal. |

## Bounded, and bounded by the caller

Every expression reaching this crate is untrusted: it may come from an MCP
client, a hand-edited catalog file, or a workload manifest submitted by a peer.
Source length, parse depth, node count, variable count, dependency depth, and
total evaluation work are all bounded by an explicit `Limits` value. A refusal
is a structured `LimitError` naming the dimension, the value reached, and the
bound configured. The bounds are the caller's, never the process's — the
convenient entry points apply `Limits::DEFAULT`, and the bounded siblings take
whatever the caller declares.

## Testing and fuzzing

Contract testing: **has.** `tests/limits.rs` exercises every bound as an
observable refusal, and `tests/quantities.rs` covers dimensioned evaluation.
The module doctests exercise the default-bounded and caller-bounded entry
points.

Fuzzing: **has.** The `variables_expression` target in [`fuzz/`](../../fuzz)
drives `CompiledExpression::parse` and `parse_bounded` over untrusted text under
default and tight caller bounds. Its first campaign found a real defect: the
lexer classified UTF-8 continuation bytes as identifier characters and sliced a
`&str` at a non-character boundary, panicking on hostile input. The parser now
lexes ASCII only — every valid symbol, unit, and operator is ASCII — so
non-ASCII input is a structured syntax error, guarded by a regression test.
