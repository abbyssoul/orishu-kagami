#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_identity::{
    CertFingerprint, ClusterName, FormationId, Incarnation, MembershipTombstone, NodeId,
    VersionTuple, WorkerName,
};
use serde::{Serialize, de::DeserializeOwned};

// The identity types are the validating boundary on hostile identity strings.
// JSON exercises the public serde contract, not a network codec: a value that
// parses must re-serialize and parse back to itself.
fn check<T: Serialize + DeserializeOwned + PartialEq>(data: &[u8]) {
    if let Ok(value) = serde_json::from_slice::<T>(data) {
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(serde_json::from_slice::<T>(&bytes).unwrap() == value);
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    check::<FormationId>(data);
    check::<NodeId>(data);
    check::<ClusterName>(data);
    check::<WorkerName>(data);
    check::<CertFingerprint>(data);
    check::<VersionTuple>(data);
    check::<Incarnation>(data);
    check::<MembershipTombstone>(data);
});
