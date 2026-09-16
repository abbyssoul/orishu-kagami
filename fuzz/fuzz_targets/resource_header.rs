#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_resource::ResourceHeader;

// The discriminator is the one field a hostile document is guaranteed to reach,
// because it is read before any body is trusted. Both JSON and CBOR reach it, so
// both encodings are fuzzed. A value that decodes must round-trip in its own
// encoding; decoding it is never domain acceptance.
fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }

    if let Ok(header) = serde_json::from_slice::<ResourceHeader>(data) {
        let bytes = serde_json::to_vec(&header).unwrap();
        assert!(serde_json::from_slice::<ResourceHeader>(&bytes).unwrap() == header);
    }

    if let Ok(header) = ciborium::from_reader::<ResourceHeader, _>(data) {
        let mut bytes = Vec::new();
        ciborium::into_writer(&header, &mut bytes).unwrap();
        let round: ResourceHeader = ciborium::from_reader(bytes.as_slice()).unwrap();
        assert!(round == header);
    }
});
