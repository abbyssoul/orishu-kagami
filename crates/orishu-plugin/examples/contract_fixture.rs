//! Emit reviewable test-only declarations and canonical vectors to stdout.
//! This does not pack plugins or compile/validate executable kernels.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;
use orishu_plugin::*;
use serde_json::json;
use support::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    let l = Limits::default();
    let mut items = declarations();
    let mut alternative = items[5].1.clone();
    if let Payload::FieldModels(p) = &mut alternative {
        p.scientific.name = "org.example.gravity.alternative".parse().unwrap();
        p.scientific.kernel = ArtifactDigest::sha256_of(b"alternative-kernel-fixture");
    }
    items.push(("alternative", alternative));
    let declarations:Vec<_>=items.iter().map(|(id,p)|{
        let bytes=p.canonical_bytes(&l).unwrap();
        let source:serde_json::Value=ciborium::from_reader(bytes.as_slice()).unwrap();
        json!({"id":id,"extensionPoint":p.point().as_str(),"source":source,
            "canonicalHex":hex(&bytes),"artifactDigest":ArtifactDigest::sha256_of(&bytes),
            "scientificHex":hex(&p.scientific_bytes(&l).unwrap()),"contract":p.contract_ref(&l).unwrap()})
    }).collect();
    let releases = [
        release("org.example.vocabulary", &items[..5], &[]),
        release(
            "org.example.solvers",
            &items[5..7],
            &[b"classical-kernel-fixture", b"euler-kernel-fixture"],
        ),
        release(
            "org.example.alternative",
            &items[7..],
            &[b"alternative-kernel-fixture"],
        ),
    ];
    let releases: Vec<_> = releases
        .iter()
        .map(|(r, _)| {
            let bytes = r.canonical_bytes(&l).unwrap();
            json!({"source":r,"canonicalHex":hex(&bytes),"releaseId":r.release_id(&l).unwrap()})
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"fixtureVersion":1,
        "note":"Kernel bytes are inert fixtures, not executable Wasm; no ABI or runtime proof.",
        "declarations":declarations,"releases":releases}))
        .unwrap()
    );
}
