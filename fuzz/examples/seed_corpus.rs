//! Deterministic public fixtures only; never seed with captured credentials.
use orishu_membership::{
    DeltaBody, GossipDelta, Incarnation, OutboundBody, OutboundMessage, ProbeId, SenderIdentity,
    antientropy::MembershipTree, testing,
};
use orishu_worker::peer::{codec, wire};
use std::{fs, path::Path};

fn seed(target: &str, name: &str, bytes: &[u8]) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(target);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(name), bytes).unwrap();
}

fn main() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/orishu-membership/tests/fixtures");
    for entry in fs::read_dir(fixtures).unwrap() {
        let entry = entry.unwrap();
        let bytes = fs::read(entry.path()).unwrap();
        let name = entry.file_name();
        seed("membership_messages", name.to_str().unwrap(), &bytes);
        if let Ok(delta) = serde_json::from_slice::<GossipDelta>(&bytes) {
            seed(
                "worker_frames",
                name.to_str().unwrap(),
                &codec::encode(&delta).unwrap(),
            );
            seed(
                "worker_frames",
                &format!("frame-{}", name.to_str().unwrap()),
                &codec::encode_frame(&delta).unwrap(),
            );
        }
    }
    let model = testing::model_with_members(5);
    let node: orishu_membership::NodeId = "node-0000".parse().unwrap();
    let digest = MembershipTree::build(&model).digest();
    for (name, body) in [
        (
            "ping",
            OutboundBody::Ping {
                probe: ProbeId(3),
                incarnation: Incarnation::INITIAL,
            },
        ),
        (
            "ack",
            OutboundBody::Ack {
                probe: ProbeId(3),
                incarnation: Incarnation::INITIAL,
            },
        ),
        (
            "pull",
            OutboundBody::PullRequest {
                round: 1,
                digest,
                buckets: vec![0],
                cursor: None,
            },
        ),
    ] {
        for count in [0, 1, 10] {
            let message = OutboundMessage {
                body: body.clone(),
                seq: 7,
                gossip: (0..count)
                    .map(|_| GossipDelta {
                        hops: 0,
                        body: DeltaBody::MembershipUpdate(testing::member("node-0001", 2)),
                    })
                    .collect(),
            };
            let packet = wire::encode(
                message,
                model.formation().clone(),
                SenderIdentity::Admitted(node.clone()),
                None,
            )
            .unwrap();
            seed("worker_peer", &format!("{name}-{count}"), &packet.bytes);
        }
    }
    let request = orishu_worker::peer::catchup::wire::Request::new(
        model.formation().clone(),
        "node-0000".parse().unwrap(),
        "fuzz-request".parse().unwrap(),
        orishu_worker::peer::catchup::wire::Action::Begin,
    );
    seed("worker_catchup", "begin", &request.encode().unwrap());
    let frozen = orishu_worker::peer::catchup::Frozen::capture(&model, 1).unwrap();
    seed("worker_catchup", "page", frozen.page(0).unwrap());
    let reply: orishu_worker::peer::catchup::wire::Reply =
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1, "type": "AdmissionStateReply",
            "formationId": model.formation(), "senderId": "node-0000", "requestId": "fuzz-request",
            "outcome": { "outcome": "rejected", "reason": "notReady" }
        }))
        .unwrap();
    seed("worker_catchup", "reply", &codec::encode(&reply).unwrap());
    for target in ["worker_frames", "worker_peer", "worker_catchup"] {
        for (name, bytes) in [
            ("duplicate", b"\xa2\x61x\x00\x78\x01x\x01".as_slice()),
            (
                "huge-array",
                b"\x9b\xff\xff\xff\xff\xff\xff\xff\xff".as_slice(),
            ),
            ("huge-frame", b"\xff\xff\xff\xff".as_slice()),
            ("truncated", b"\xbf\x61x".as_slice()),
            ("indefinite", b"\xbf\x61x\x00\xff".as_slice()),
        ] {
            seed(target, name, bytes);
        }
        seed(target, "too-deep", &[vec![0x81; 25], vec![0]].concat());
    }
}
