//! CPU costs of the production worker adapters; network capacity is measured
//! separately by scripts/pi-lab-capacity.py.
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use orishu_membership::{
    DeltaBody, GossipDelta, Incarnation, OutboundBody, OutboundMessage, ProbeId, SenderIdentity,
    testing,
};
use orishu_worker::peer::{codec, wire};
use std::hint::black_box;

fn ping(count: usize) -> OutboundMessage {
    OutboundMessage {
        body: OutboundBody::Ping {
            probe: ProbeId(3),
            incarnation: Incarnation::INITIAL,
        },
        seq: 7,
        gossip: (0..count)
            .map(|i| GossipDelta {
                hops: 0,
                body: DeltaBody::MembershipUpdate(testing::member(&format!("node-{i:04}"), 1)),
            })
            .collect(),
    }
}

fn bench_codec(c: &mut Criterion) {
    let mut group = c.benchmark_group("worker_codec");
    for count in [1, 10, 100] {
        let value = ping(count).gossip;
        let bytes = codec::encode(&value).unwrap();
        let frame = codec::encode_frame(&value).unwrap();
        assert_eq!(codec::decode::<Vec<GossipDelta>>(&bytes).unwrap(), value);
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("decode_members", count),
            &bytes,
            |b, bytes| {
                b.iter(|| black_box(codec::decode::<Vec<GossipDelta>>(black_box(bytes)).unwrap()));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("decode_frame", count),
            &frame,
            |b, bytes| {
                b.iter(|| {
                    black_box(codec::decode_frame::<Vec<GossipDelta>>(black_box(bytes)).unwrap())
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("encode_frame", count),
            &value,
            |b, value| {
                b.iter(|| black_box(codec::encode_frame(black_box(value)).unwrap()));
            },
        );
    }
    group.finish();
    let mut group = c.benchmark_group("worker_reject");
    for (name, bytes) in [
        ("duplicate", b"\xa2\x61x\x00\x61x\x01".to_vec()),
        ("declared_size", b"\x9a\xff\xff\xff\xff".to_vec()),
        ("depth", [vec![0x81; 25], vec![0]].concat()),
        ("oversized", vec![0; codec::MAX_FRAME_BYTES + 1]),
    ] {
        assert!(codec::decode::<Vec<GossipDelta>>(&bytes).is_err());
        group.bench_function(name, |b| {
            b.iter(|| black_box(codec::decode::<Vec<GossipDelta>>(black_box(&bytes))))
        });
    }
    group.finish();
}

fn bench_wire(c: &mut Criterion) {
    let mut group = c.benchmark_group("worker_wire");
    let model = testing::model_with_members(20);
    let node = model.local_id().clone();
    let context = testing::peer_context(&model, &node, 0);
    for count in [0, 1, 10] {
        let message = ping(count);
        let encode = |message| {
            wire::encode(
                message,
                model.formation().clone(),
                SenderIdentity::Admitted(node.clone()),
                None,
            )
            .unwrap()
        };
        let packet = encode(message.clone());
        assert!(wire::decode(&packet.bytes, &context, packet.transport).is_ok());
        group.throughput(Throughput::Elements(1));
        group.bench_function(BenchmarkId::new("encode_ping", count), |b| {
            b.iter_batched(
                || message.clone(),
                |message| black_box(encode(message)),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("decode_ping", count), |b| {
            b.iter(|| {
                black_box(
                    wire::decode(black_box(&packet.bytes), &context, packet.transport).unwrap(),
                )
            });
        });
    }
    group.finish();
}

fn bench_summary(c: &mut Criterion) {
    // The published view + actual client CBOR serialization, with one long-lived
    // owner/runtime. This isolates CPU work; it does not measure TLS/HTTP queues.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let mut group = c.benchmark_group("worker_summary");
    for count in [1, 5, 20, 1000] {
        let (handle, owner) =
            orishu_worker::driver::spawn_standalone(testing::model_with_members(count - 1));
        group.bench_function(BenchmarkId::new("view_encode", count), |b| {
            b.iter(|| {
                let view = handle.view().unwrap();
                let response = orishu::model::ApiResponse::Ok {
                    data: Some(orishu::model::ResponseData::ClusterSummary(view.summary)),
                };
                black_box(codec::encode(black_box(&response)).unwrap())
            });
        });
        runtime.block_on(async {
            handle.shutdown().await.unwrap();
            owner.await.unwrap().unwrap();
        });
    }
    group.finish();
}

fn bench_catchup(c: &mut Criterion) {
    use orishu_worker::peer::catchup::{Frozen, Receiver};
    let model = testing::golden_model();
    let frozen = Frozen::capture(&model, 7).unwrap();
    let mut group = c.benchmark_group("worker_catchup");
    group.bench_function("capture", |b| {
        b.iter(|| black_box(Frozen::capture(black_box(&model), 7).unwrap()));
    });
    group.bench_function("receive_complete", |b| {
        b.iter(|| {
            let mut receiver = Receiver::new(frozen.descriptor().clone()).unwrap();
            for i in 0..frozen.descriptor().pages {
                receiver = receiver.push(black_box(frozen.page(i).unwrap())).unwrap();
            }
            black_box(receiver.finish_baseline().unwrap())
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_codec,
    bench_wire,
    bench_summary,
    bench_catchup
);
criterion_main!(benches);
