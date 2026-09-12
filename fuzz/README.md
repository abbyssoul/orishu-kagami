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

Membership is sans-IO and has no network codec. Its JSON target exercises the
public type contract; worker targets exercise the actual hostile network bytes.
Successful deserialization alone never establishes domain acceptance. No target
catches panics: a panic, sanitizer error, timeout or allocation failure is a
failure to investigate. A passing finite campaign cannot prove all messages safe.

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
