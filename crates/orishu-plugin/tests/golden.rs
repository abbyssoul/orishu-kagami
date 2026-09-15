//! Fixed byte vectors, independently decoded by ciborium (test-only dependency).
use orishu_plugin::*;
use serde_json::Value;

fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn checked_in_release_and_scientific_vectors_are_exact() {
    let f: Value = serde_json::from_str(include_str!("fixtures/contract-v1.json")).unwrap();
    let l = Limits::default();
    for d in f["declarations"].as_array().unwrap() {
        let point =
            KnownPoint::from_id(&d["extensionPoint"].as_str().unwrap().parse().unwrap()).unwrap();
        let p = payload_from_json(point, &serde_json::to_vec(&d["source"]).unwrap(), &l).unwrap();
        let canonical = bytes(d["canonicalHex"].as_str().unwrap());
        assert_eq!(p.canonical_bytes(&l).unwrap(), canonical, "{}", d["id"]);
        assert_eq!(
            p.scientific_bytes(&l).unwrap(),
            bytes(d["scientificHex"].as_str().unwrap())
        );
        assert_eq!(
            serde_json::to_value(p.contract_ref(&l).unwrap()).unwrap(),
            d["contract"]
        );
        assert_eq!(
            ArtifactDigest::sha256_of(&canonical).to_string(),
            d["artifactDigest"].as_str().unwrap()
        );
        let independent: Value = ciborium::from_reader(canonical.as_slice()).unwrap();
        assert_eq!(independent, d["source"]);
        assert_eq!(
            payload_from_cbor(point, &canonical, &l)
                .unwrap()
                .canonical_bytes(&l)
                .unwrap(),
            canonical
        );
    }
    for r in f["releases"].as_array().unwrap() {
        let parsed = release_from_json(&serde_json::to_vec(&r["source"]).unwrap(), &l).unwrap();
        let canonical = bytes(r["canonicalHex"].as_str().unwrap());
        assert_eq!(parsed.canonical_bytes(&l).unwrap(), canonical);
        assert_eq!(
            parsed.release_id(&l).unwrap().to_string(),
            r["releaseId"].as_str().unwrap()
        );
        let independent: Value = ciborium::from_reader(canonical.as_slice()).unwrap();
        let parsed_independent =
            release_from_json(&serde_json::to_vec(&independent).unwrap(), &l).unwrap();
        assert_eq!(parsed_independent.canonical_bytes(&l).unwrap(), canonical);
        assert_eq!(
            release_from_cbor(&canonical, &l)
                .unwrap()
                .canonical_bytes(&l)
                .unwrap(),
            canonical
        );
    }
}

#[test]
fn every_known_payload_refuses_unknown_fields_and_null_presentation() {
    let f: Value = serde_json::from_str(include_str!("fixtures/contract-v1.json")).unwrap();
    for d in f["declarations"].as_array().unwrap() {
        let point =
            KnownPoint::from_id(&d["extensionPoint"].as_str().unwrap().parse().unwrap()).unwrap();
        for (key, value) in [
            ("unexpected", serde_json::json!(true)),
            ("presentation", Value::Null),
        ] {
            let mut source = d["source"].clone();
            source[key] = value;
            assert!(
                payload_from_json(
                    point,
                    &serde_json::to_vec(&source).unwrap(),
                    &Limits::default()
                )
                .is_err()
            );
        }
        let mut source = d["source"].clone();
        source["scientific"]["unexpected"] = serde_json::json!(true);
        assert!(
            payload_from_json(
                point,
                &serde_json::to_vec(&source).unwrap(),
                &Limits::default()
            )
            .is_err()
        );
    }
}

#[test]
fn structural_schema_covers_root_and_all_six_payloads() {
    let schema: Value =
        serde_json::from_str(include_str!("../schema/plugin-v1.schema.json")).unwrap();
    for name in [
        "release",
        "components",
        "fields",
        "observables",
        "constants",
        "fieldModels",
        "integrators",
    ] {
        assert_eq!(schema["$defs"][name]["type"], "object");
        assert_eq!(schema["$defs"][name]["additionalProperties"], false);
    }
}
