# Formation fuzzing

This isolated cargo-fuzz workspace follows the `avahi-tui/fuzz` organization:
one target per contract, an independent lockfile, and no libFuzzer dependency in
product crates. Use the repository compiler for normal tests and a nightly
compiler for sanitizer/coverage instrumentation. `libfuzzer-sys` is pinned;
update it deliberately with the fuzz lockfile. See the
[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz/guide.html).

| Target | Production surface and assertions |
| --- | --- |
| `membership_messages` | Core serde parsing of gossip, tombstones, Merkle digests and admission baselines; successful JSON values round-trip |
| `worker_frames` | Actual allocation-bounded CBOR decoder and four-byte stream framing; membership binary representations and round trips |
| `worker_peer` | Authenticated profile-5 stream/datagram decoding for member/applicant contexts, followed by the real core transition; formation/sender binding and member bounds |
| `worker_catchup` | Reliable admission request framing and reply schema/formation/sender/request binding |
| `identity_types` | Validating deserialization of `orishu-identity` formations, nodes, labels, fingerprints, version tuples and tombstones; successful JSON values round-trip |
| `resource_header` | `orishu-resource` discriminator decode in JSON and CBOR — the first field a hostile document reaches; successful headers round-trip in their own encoding |
| `variables_expression` | `orishu-variables` expression lexer/parser over untrusted text under default and tight caller bounds; a parsed expression's retained source re-parses |
| `workload_canonical` | `orishu-workload` canonical value decode and `manifest_from_canonical_bytes`; codec injectivity (decode/re-encode/decode) and stable digest across a round trip |
| `plugin_declarations` | `orishu-plugin` bounded JSON and CBOR readers for releases and all six payload extension points |
| `plugin_bundle` | `orishu-plugin` stored-ZIP `.okplugin` archive reader: framing, root/blob closure, CRC and digest verification, with no filesystem extraction |
| `plugin_execution` | `orishu-plugin` scientific bulk-IO packet decode for object/dynamic/coupled/force/entity records: framing, bounds and strict identity ordering |
| `document_wire` | `kagami-document` authoring command and snapshot wire types; successful JSON values round-trip |
| `session_wire` | `kagami-session` command envelope and outcome wire types; successful JSON values round-trip |
| `catalog_documents` | `kagami-catalog` bounded multi-document loader over untrusted UTF-8 catalog text |

Membership is sans-IO and has no network codec. Its JSON target exercises the
public type contract; worker targets exercise the actual hostile network bytes.
The remaining targets exercise each pure crate's own decode boundary — the point
at which it accepts hostile bytes — so a defect is attributed to the crate that
owns it rather than only to a consumer. `orishu-runtime`'s Wasm-component
admission path needs a component harness rather than a byte-slice target and is
covered by integration tests, not here; its pure bulk-IO packet contract is
`plugin_execution`. The `orishu` client crate's model types are exercised on the
wire through the `worker_*` targets that depend on it.

Successful deserialization alone never establishes domain acceptance. No target
catches panics: a panic, sanitizer error, timeout or allocation failure is a
failure to investigate. A passing finite campaign cannot prove all messages safe.
`variables_expression` found one such defect on its first campaign — the
expression lexer classified UTF-8 continuation bytes as identifier characters and
sliced a `&str` at a non-character boundary. The fix restricts lexing to ASCII
(every valid symbol, unit and operator is ASCII) so non-ASCII input is a
structured syntax error; `crates/orishu-variables` carries the regression test.

From the repository root:

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
rustup toolchain install nightly
cargo run --locked --manifest-path fuzz/Cargo.toml --example seed_corpus
cargo +nightly fuzz run worker_peer -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=1048581
cargo +nightly fuzz run worker_peer -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=1048581 -len_control=0
```

Repeat with each target. `make fuzz-smoke` runs two passes per target for 30
seconds each; `FUZZ_SECONDS=300 make fuzz-smoke` extends the campaign.
Build/seed time is additional. Never benchmark concurrently with fuzzing or
compilation. Set `CARGO_NET_OFFLINE=true` after dependencies are cached for
offline operation.

`-max_len=1048581` only *permits* inputs just above the one-MiB frame ceiling;
it does not produce them. libFuzzer's default `-len_control` raises the length
limit slowly, so a short campaign stays far below the cap: a 180-second pass
peaked at a 12,548-byte limit for `worker_peer` and 81,513 for `worker_frames`,
one to eight percent of the ceiling. Reaching the oversize and declared-length
paths needs `-len_control=0`, which pins generation at `-max_len`; that is the
second smoke pass. Run both, since the ramp finds small structural cases that
full-length generation rarely produces. Declared oversize prefixes, duplicates,
malformed UTF-8, unsupported encodings and deep nesting remain useful even in
short campaigns.

The seed generator uses only deterministic synthetic identities and the
checked-in public fixtures. It preserves discoveries when rerun. The working
corpus, sanitizer binaries, coverage and crash artifacts are ignored; retain
campaign output externally and promote minimized regressions into ordinary
tests. Never commit real tokens or captured private Pi traffic.

```sh
cargo +nightly fuzz run worker_peer fuzz/artifacts/worker_peer/crash-HASH
cargo +nightly fuzz tmin worker_peer fuzz/artifacts/worker_peer/crash-HASH
cargo +nightly fuzz cmin worker_peer
cargo +nightly fuzz coverage worker_peer
```

Record toolchain, source revision/diff, duration, executions, corpus size,
coverage and failure artifacts. Replay/minimize on the same toolchain first.
Fix through the production parser/transition, add an ordinary regression test,
and replay all seeds before extending the campaign. Live QUIC/HTTP survival,
TLS, slow streams, queue pressure and shutdown remain integration-test concerns;
these CPU-only fuzz targets do not replace those tests.
