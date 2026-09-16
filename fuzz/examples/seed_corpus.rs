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

/// Copy every regular file in a public fixtures directory into a target corpus.
/// Missing directories are skipped so the generator never fails on a partial
/// checkout.
fn seed_dir(target: &str, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Ok(bytes) = fs::read(&path) {
                seed(target, entry.file_name().to_str().unwrap(), &bytes);
            }
        }
    }
}

/// Decode a whitespace-tolerant hex fixture into raw bytes. Some checked-in
/// binary contracts (canonical CBOR, stored-ZIP bundles) are stored hex-encoded.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let hex: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
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

    // Seed the contract targets from checked-in public fixtures. These are
    // deterministic and carry no credentials.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    // JSON/YAML text contracts: the fixtures decode in the target's own encoding.
    seed_dir(
        "resource_header",
        &root.join("crates/orishu/tests/fixtures/resource-wire"),
    );
    seed_dir(
        "document_wire",
        &root.join("crates/kagami-document/tests/fixtures"),
    );
    seed_dir(
        "session_wire",
        &root.join("crates/kagami-session/tests/fixtures"),
    );
    seed_dir("catalog_documents", &root.join("etc/catalogs"));
    if let Ok(bytes) = fs::read(root.join("crates/orishu-plugin/tests/fixtures/contract-v1.json")) {
        seed("plugin_declarations", "contract-v1.json", &bytes);
    }

    // Binary contracts stored hex-encoded: decode to the raw bytes the target reads.
    let canonical = root.join("crates/orishu-workload/tests/fixtures/canonical");
    if let Ok(entries) = fs::read_dir(&canonical) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("hex") {
                if let Some(bytes) = fs::read_to_string(&path).ok().and_then(|t| decode_hex(&t)) {
                    seed(
                        "workload_canonical",
                        path.file_stem().unwrap().to_str().unwrap(),
                        &bytes,
                    );
                }
            }
        }
    }
    if let Some(bytes) = fs::read_to_string(
        root.join("crates/orishu-plugin/tests/fixtures/python-stored-empty.hex"),
    )
    .ok()
    .and_then(|t| decode_hex(&t))
    {
        seed("plugin_bundle", "python-stored-empty", &bytes);
    }

    // Small literal seeds for the text/serde targets that have no byte fixtures.
    for (name, expression) in [
        ("literal", "2.7 g / cm^3"),
        ("nested", "((1 + 2) * 3)"),
        ("reference", "planets.sun.mass * scale"),
    ] {
        seed("variables_expression", name, expression.as_bytes());
    }
    for (name, json) in [("node", "\"node-0000\""), ("formation", "\"formation-abc\"")] {
        seed("identity_types", name, json.as_bytes());
    }
}
