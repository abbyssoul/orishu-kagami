//! Run optimized, separately from Criterion: DHAT changes allocator timing.
//! Arguments: decode | frame | gossip | collect | wire, then an iteration count.
use orishu_membership::{
    DeltaBody, GossipDelta, GossipQueue, Incarnation, Limits, OutboundBody, OutboundMessage,
    ProbeId, SenderIdentity, antientropy::MembershipTree, testing,
};
use orishu_worker::peer::{codec, wire};
use std::hint::black_box;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "decode".into());
    assert!(
        ["decode", "frame", "gossip", "collect", "wire"].contains(&mode.as_str()),
        "unknown allocation workload"
    );
    let iterations: usize = std::env::args().nth(2).map_or(1000, |s| s.parse().unwrap());
    let delta = GossipDelta {
        hops: 0,
        body: DeltaBody::MembershipUpdate(testing::member("node-0001", 1)),
    };
    let bytes = codec::encode(&delta).unwrap();
    let mut queue = GossipQueue::default();
    for i in 0..512 {
        queue.enqueue(
            DeltaBody::MembershipUpdate(testing::member(&format!("node-{i:04}"), i as u64)),
            &Limits::default(),
        );
    }
    let model = testing::model_with_members(999);
    let tree = MembershipTree::build(&model);
    let buckets: Vec<_> = (0..model.limits().anti_entropy_buckets() as u16).collect();
    let message = OutboundMessage {
        body: OutboundBody::Ping {
            probe: ProbeId(3),
            incarnation: Incarnation::INITIAL,
        },
        seq: 7,
        gossip: vec![delta.clone(); 10],
    };
    let _profile = dhat::Profiler::new_heap();
    for _ in 0..iterations {
        match mode.as_str() {
            "decode" => {
                black_box(codec::decode::<GossipDelta>(black_box(&bytes)).unwrap());
            }
            "frame" => {
                black_box(codec::encode_frame(black_box(&delta)).unwrap());
            }
            "gossip" => {
                black_box(queue.take(10, u32::MAX));
            }
            "collect" => {
                black_box(tree.collect(black_box(&buckets), None, 1000));
            }
            "wire" => {
                black_box(
                    wire::encode(
                        message.clone(),
                        model.formation().clone(),
                        SenderIdentity::Admitted(model.local_id().clone()),
                        None,
                    )
                    .unwrap(),
                );
            }
            _ => unreachable!(),
        }
    }
}
