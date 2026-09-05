//! Microbenchmarks for the `orishu-membership` transition core.
//!
//! Deliberately synthetic, mirroring the shape of
//! `orishu-variables/benches/variables.rs`: no network behind any of this,
//! just deterministically generated formations, so results are comparable
//! across commits at a fixed member count.
//!
//! The point is to profile the *functional core in isolation*. A worker's
//! observed membership latency will include CBOR encoding, QUIC, and a timer
//! wheel, none of which live in this crate; what these groups measure is the
//! part that is pure computation, where a regression is a design problem
//! rather than a network problem.
//!
//! Five groups, chosen because each scales with a different thing:
//!
//! - **`merge`** — folding one gossip delta into a formation of `count`
//!   members. Should be `O(log n)` (one `BTreeMap` lookup plus one insert), so
//!   a linear curve here means something started scanning the member map.
//! - **`probe_cycle`** — a full correlated probe round trip: start round,
//!   supply a peer, receive the `Ack`. Four `update` calls, dominated by probe
//!   bookkeeping rather than by formation size.
//! - **`digest`** — building the canonical anti-entropy tree and its digest.
//!   Genuinely `O(n)` in members, with a SHA-256 per leaf; this is the most
//!   expensive thing the core does and the reason anti-entropy runs on a long
//!   timer rather than per message.
//! - **`admission`** — the local gates plus identity insertion for one
//!   applicant. Bounds how expensive a join flood can be made.
//! - **`gossip_queue`** — selecting a piggyback batch from a full retirement
//!   queue, which sorts candidates on every send.

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_membership::{
    Command, DeltaBody, Effect, EffectOutcome, GossipDelta, Incarnation, Limits, LimitsSpec,
    Membership, Message, NodeId, OutboundBody, PeerBody, PeerInput, ProbeId, SessionId,
    antientropy::MembershipTree,
    message::AdmissionEvidence,
    model::{Accepts, Endpoints, NodeCapacity},
    testing, update,
};

const DEFAULT_MEMBER_COUNTS: [usize; 4] = [10, 100, 1_000, 10_000];
const DEFAULT_QUEUE_DEPTHS: [usize; 3] = [16, 128, 512];

/// Parses a comma-separated list of `usize`s from `name`, falling back to
/// `default` when the variable is unset and panicking on a malformed entry
/// rather than silently ignoring it.
fn env_list(name: &str, default: &[usize]) -> Vec<usize> {
    match std::env::var(name) {
        Ok(raw) => raw
            .split(',')
            .map(|entry| {
                entry
                    .trim()
                    .parse()
                    .unwrap_or_else(|error| panic!("invalid {name} entry {entry:?}: {error}"))
            })
            .collect(),
        Err(_) => default.to_vec(),
    }
}

/// Formation sizes for the `merge`, `probe_cycle`, `digest`, and `admission`
/// groups, overridable via `ORISHU_MEMBERSHIP_BENCH_MEMBERS`.
fn member_counts() -> Vec<usize> {
    env_list("ORISHU_MEMBERSHIP_BENCH_MEMBERS", &DEFAULT_MEMBER_COUNTS)
}

/// Queue depths for the `gossip_queue` group, overridable via
/// `ORISHU_MEMBERSHIP_BENCH_QUEUE_DEPTHS`.
fn queue_depths() -> Vec<usize> {
    env_list(
        "ORISHU_MEMBERSHIP_BENCH_QUEUE_DEPTHS",
        &DEFAULT_QUEUE_DEPTHS,
    )
}

/// Limits large enough that a benchmark formation is not rejected for
/// exceeding the default member cap.
fn spacious_limits(count: usize) -> Limits {
    Limits::try_from(LimitsSpec {
        max_members: count.max(4096) * 2,
        max_snapshot_members: count.max(4096) * 2,
        max_gossip_queue: 8192,
        ..LimitsSpec::default()
    })
    .expect("benchmark limits are valid")
}

fn formation(count: usize) -> Membership {
    testing::model_with_limits(count, spacious_limits(count))
}

/// A gossip delta describing an existing member at a newer version, so the
/// merge does real work rather than short-circuiting on staleness.
fn newer_member_delta(index: usize) -> DeltaBody {
    let mut member = testing::member(&format!("node-{index:04}"), 99);
    member.capabilities.cpu_cores = 64;
    DeltaBody::MembershipUpdate(member)
}

/// Wraps `deltas` as piggybacked gossip on a probe from `node-0000`.
fn gossip_message(model: &Membership, seq: u64, deltas: Vec<DeltaBody>) -> Message {
    let sender = NodeId::new("node-0000").unwrap();
    let mut context = testing::peer_context(model, &sender, seq);
    context.gossip = deltas
        .into_iter()
        .map(|body| GossipDelta { hops: 0, body })
        .collect();
    Message::Peer(PeerInput {
        context,
        body: PeerBody::Ping {
            probe: ProbeId(1),
            incarnation: Incarnation(1),
        },
    })
}

fn bench_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge");
    for count in member_counts() {
        let model = formation(count);
        let message = gossip_message(&model, 1, vec![newer_member_delta(1)]);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("one_delta", count), &count, |b, _| {
            // The model is consumed by `update`, so each iteration needs its
            // own copy. Cloning happens in setup and is not timed.
            b.iter_batched(
                || (model.clone(), message.clone()),
                |(model, message)| std::hint::black_box(update(model, message)),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_merge_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_batch");
    for count in member_counts() {
        let model = formation(count);
        let batch = model.limits().max_gossip_per_inbound_message();
        let deltas: Vec<DeltaBody> = (0..batch).map(newer_member_delta).collect();
        let message = gossip_message(&model, 1, deltas);
        group.throughput(Throughput::Elements(batch as u64));
        group.bench_with_input(BenchmarkId::new("full_piggyback", count), &count, |b, _| {
            b.iter_batched(
                || (model.clone(), message.clone()),
                |(model, message)| std::hint::black_box(update(model, message)),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_probe_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("probe_cycle");
    for count in member_counts() {
        let model = formation(count);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("direct_ack", count), &count, |b, _| {
            b.iter_batched(
                || model.clone(),
                |model| {
                    let started = update(model, Message::Local(Command::StartProbeRound));
                    let request = started
                        .effects
                        .iter()
                        .find_map(|effect| match effect {
                            Effect::SelectPeers { request, .. } => Some(*request),
                            _ => None,
                        })
                        .expect("a probe round selects a peer");
                    let sent = update(
                        started.model,
                        Message::Outcome(EffectOutcome::PeersSelected {
                            request,
                            peers: vec![NodeId::new("node-0001").unwrap()],
                        }),
                    );
                    let probe = sent
                        .effects
                        .iter()
                        .find_map(|effect| match effect {
                            Effect::Send { message, .. } => match &message.body {
                                OutboundBody::Ping { probe, .. } => Some(*probe),
                                _ => None,
                            },
                            _ => None,
                        })
                        .expect("a probe round sends a Ping");
                    let sender = NodeId::new("node-0001").unwrap();
                    let context = testing::peer_context(&sent.model, &sender, 1);
                    std::hint::black_box(update(
                        sent.model,
                        Message::Peer(PeerInput {
                            context,
                            body: PeerBody::Ack {
                                probe,
                                incarnation: Incarnation(1),
                            },
                        }),
                    ))
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_digest(c: &mut Criterion) {
    let mut group = c.benchmark_group("digest");
    for count in member_counts() {
        let model = formation(count);
        // One leaf hash per replicated entity is what this scales with.
        group.throughput(Throughput::Elements(model.members().len() as u64));
        group.bench_with_input(BenchmarkId::new("build_and_root", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(MembershipTree::build(&model).digest()))
        });
    }
    group.finish();
}

fn bench_anti_entropy_collect(c: &mut Criterion) {
    let mut group = c.benchmark_group("anti_entropy_collect");
    for count in member_counts() {
        let model = formation(count);
        let tree = MembershipTree::build(&model);
        let buckets: Vec<u16> = (0..model.limits().anti_entropy_buckets() as u16).collect();
        let max = model.limits().max_anti_entropy_entries();
        group.throughput(Throughput::Elements(max.min(model.members().len()) as u64));
        group.bench_with_input(BenchmarkId::new("one_batch", count), &count, |b, _| {
            b.iter(|| std::hint::black_box(tree.collect(&buckets, None, max)))
        });
    }
    group.finish();
}

fn bench_admission(c: &mut Criterion) {
    let mut group = c.benchmark_group("admission");
    for count in member_counts() {
        let model = formation(count);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("full_handshake", count), &count, |b, _| {
            b.iter_batched(
                || model.clone(),
                |model| {
                    let session = SessionId(1);
                    let context = testing::applicant_context(
                        model.formation(),
                        "worker-applicant",
                        session,
                        0xA1,
                    );
                    let requested = update(
                        model,
                        Message::Peer(PeerInput {
                            context,
                            body: PeerBody::JoinRequest {
                                endpoints: Endpoints::default(),
                                accepts: Accepts::default(),
                                capacity: NodeCapacity::default(),
                                capabilities: testing::capabilities(),
                            },
                        }),
                    );
                    let verification = requested
                        .effects
                        .iter()
                        .find_map(|effect| match effect {
                            Effect::VerifyCredential { request, .. } => Some(*request),
                            _ => None,
                        })
                        .expect("admission requests credential verification");
                    let verified = update(
                        requested.model,
                        Message::Outcome(EffectOutcome::CredentialVerified {
                            request: verification,
                            session,
                            evidence: AdmissionEvidence {
                                transport_authenticated: true,
                                token_valid: true,
                                source_network_blocked: false,
                            },
                        }),
                    );
                    let allocation = verified
                        .effects
                        .iter()
                        .find_map(|effect| match effect {
                            Effect::AllocateNodeId { request, .. } => Some(*request),
                            _ => None,
                        })
                        .expect("verified evidence requests an identity");
                    std::hint::black_box(update(
                        verified.model,
                        Message::Outcome(EffectOutcome::NodeIdAllocated {
                            request: allocation,
                            session,
                            node_id: NodeId::new("node-freshly-admitted").unwrap(),
                        }),
                    ))
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_gossip_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("gossip_queue");
    let limits = spacious_limits(1_000);
    for depth in queue_depths() {
        let mut queue = orishu_membership::GossipQueue::default();
        for index in 0..depth {
            queue.enqueue(newer_member_delta(index), &limits);
        }
        let batch = limits.max_gossip_per_message();
        group.throughput(Throughput::Elements(batch as u64));
        group.bench_with_input(BenchmarkId::new("take_batch", depth), &depth, |b, _| {
            b.iter_batched(
                || queue.clone(),
                |mut queue| std::hint::black_box(queue.take(batch, 32)),
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_merge,
    bench_merge_batch,
    bench_probe_cycle,
    bench_digest,
    bench_anti_entropy_collect,
    bench_admission,
    bench_gossip_queue
);
criterion_main!(benches);
