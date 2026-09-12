#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_membership::{GossipDelta, antientropy::MerkleDigest, baseline::AdmissionBaseline};
use orishu_worker::peer::codec;
use serde::{Serialize, de::DeserializeOwned};

fn check<T: Serialize + DeserializeOwned + PartialEq>(data: &[u8]) {
    let _ = codec::decode_frame::<T>(data);
    if let Ok(value) = codec::decode::<T>(data) {
        // Successful parsing is not domain acceptance. Re-encoding can reject
        // a value whose serde representation changes its structural budget.
        if let Ok(encoded) = codec::encode(&value) {
            assert!(codec::decode::<T>(&encoded).unwrap() == value);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Some(prefix) = data.get(..4) {
        if let Ok(length) = codec::frame_length(prefix.try_into().unwrap()) {
            assert!((1..=codec::MAX_FRAME_BYTES).contains(&length));
        }
    }
    check::<GossipDelta>(data);
    check::<Vec<GossipDelta>>(data);
    check::<MerkleDigest>(data);
    check::<AdmissionBaseline>(data);
});
