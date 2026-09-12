#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_membership::{
    GossipDelta, MembershipTombstone, antientropy::MerkleDigest, baseline::AdmissionBaseline,
};
use serde::{Serialize, de::DeserializeOwned};

fn check<T: Serialize + DeserializeOwned + PartialEq>(data: &[u8]) {
    if let Ok(value) = serde_json::from_slice::<T>(data) {
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(serde_json::from_slice::<T>(&bytes).unwrap() == value);
    }
}

fuzz_target!(|data: &[u8]| {
    // JSON tests the core's public serde contracts, not a network codec. Keep
    // serde's recursion limit enabled; the worker target tests CBOR preflight.
    if data.len() > 1_048_576 {
        return;
    }
    check::<GossipDelta>(data);
    check::<MembershipTombstone>(data);
    check::<MerkleDigest>(data);
    check::<AdmissionBaseline>(data);
});
