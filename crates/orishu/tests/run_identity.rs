use orishu::model::run::*;
use orishu_workload::WorkloadDigest;

fn root(byte: &str) -> WorkloadDigest {
    format!("sha256:{}", byte.repeat(32)).parse().unwrap()
}
fn descriptor(formation: &str, workload: WorkloadDigest, epoch: u64) -> RunDescriptor {
    RunDescriptor::new(RunIdentity::new(
        formation.parse().unwrap(),
        workload,
        WorkloadEpoch::new(epoch),
    ))
}

#[test]
fn canonical_identity_has_independent_golden_bytes_and_digest() {
    let record = descriptor("formation-a", root("00"), 1);
    let bytes = record.canonical_bytes().unwrap();
    // Independently encoded from the documented definite/sorted CBOR shape.
    let golden = concat!(
        "a26372756ea36a776f726b6c6f6164496478477368613235363a",
        "3030303030303030303030303030303030303030303030303030303030303030",
        "3030303030303030303030303030303030303030303030303030303030303030",
        "6b666f726d6174696f6e49646b666f726d6174696f6e2d616d776f726b6c6f616445706f636801",
        "6a61706956657273696f6e78186f72697368752e72756e2d64657363726970746f722f7631"
    );
    assert_eq!(
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        golden
    );
    assert_eq!(
        record.digest().unwrap().to_string(),
        "sha256:32fe2f909357a90b8e27384bd128e81b1f664265f0dd2931f7e254b2d2d46d53"
    );
    assert_eq!(RunDescriptor::from_canonical_bytes(&bytes).unwrap(), record);
    assert_eq!(
        ciborium::de::from_reader::<RunDescriptor, _>(&bytes[..]).unwrap(),
        record
    );
    let json = serde_json::to_vec_pretty(&record).unwrap();
    assert_eq!(RunDescriptor::from_json(&json).unwrap(), record);
    let reordered = format!(
        r#"{{"run":{{"workloadEpoch":1,"workloadId":"{}","formationId":"formation-a"}},"apiVersion":"{}"}}"#,
        root("00"),
        RUN_DESCRIPTOR_VERSION
    );
    assert_eq!(
        RunDescriptor::from_json(reordered.as_bytes())
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        bytes
    );
    let mut generic = vec![];
    ciborium::ser::into_writer(&record, &mut generic).unwrap();
    assert_ne!(generic, bytes, "serde field order must not become identity");
    assert!(RunDescriptor::from_canonical_bytes(&generic).is_err());
}

#[test]
fn every_identity_axis_changes_the_digest_and_extreme_values_stay_bounded() {
    let original = descriptor("formation-a", root("00"), 1).digest().unwrap();
    for different in [
        descriptor("formation-b", root("00"), 1),
        descriptor("formation-a", root("01"), 1),
        descriptor("formation-a", root("00"), 2),
    ] {
        assert_ne!(original, different.digest().unwrap());
    }
    for epoch in [0, 23, 24, 255, 256, u32::MAX as u64, u64::MAX] {
        let record = descriptor(&"f".repeat(128), root("ff"), epoch);
        let bytes = record.canonical_bytes().unwrap();
        assert!(bytes.len() <= MAX_RUN_DESCRIPTOR_BYTES);
        assert_eq!(RunDescriptor::from_canonical_bytes(&bytes).unwrap(), record);
    }
}

#[test]
fn malformed_oversized_duplicate_and_noncanonical_descriptors_are_refused() {
    let record = descriptor("formation-a", root("00"), 1);
    let bytes = record.canonical_bytes().unwrap();
    for n in 0..bytes.len() {
        assert!(RunDescriptor::from_canonical_bytes(&bytes[..n]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(RunDescriptor::from_canonical_bytes(&trailing).is_err());
    let mut version = bytes.clone();
    *version.last_mut().unwrap() = b'2';
    assert_eq!(
        RunDescriptor::from_canonical_bytes(&version),
        Err(RunDescriptorError::Version)
    );
    let mut depth = vec![0x81; 32];
    depth.push(0);
    for hostile in [
        vec![0; MAX_RUN_DESCRIPTOR_BYTES + 1],
        vec![0x7b, 255, 255, 255, 255, 255, 255, 255, 255],
        depth,
        vec![0xbf, 0xff],
    ] {
        assert!(RunDescriptor::from_canonical_bytes(&hostile).is_err());
    }
    assert_eq!(
        RunDescriptor::from_json(&vec![b' '; MAX_RUN_DESCRIPTOR_BYTES + 1]),
        Err(RunDescriptorError::Limit)
    );
    let json = serde_json::to_string(&record).unwrap();
    for bad in [
        json.replace("formation-a", " "),
        json.replace("sha256:", "SHA256:"),
        json.replace("\"workloadEpoch\":1", "\"workloadEpoch\":-1"),
        json.replace(
            "\"workloadEpoch\":1",
            "\"workloadEpoch\":1,\"workloadEpoch\":2",
        ),
        json.replace(
            "\"workloadEpoch\":1",
            "\"workloadEpoch\":1,\"nodeId\":\"x\"",
        ),
        json.replace("\"run\":", "\"connectionHints\":[],\"run\":"),
        json.replace("/v1", "/v2"),
        format!("{json}{{}}"),
    ] {
        assert!(RunDescriptor::from_json(bad.as_bytes()).is_err(), "{bad}");
    }
    // Replace the shortest epoch encoding with an equivalent wider integer.
    let marker = b"workloadEpoch";
    let at = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap()
        + marker.len();
    let mut wider = bytes;
    wider.splice(at..at + 1, [0x18, 0x01]);
    assert!(RunDescriptor::from_canonical_bytes(&wider).is_err());
}
