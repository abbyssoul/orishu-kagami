#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_workload::{
    Limits,
    canonical::{self, canonical_bytes, manifest_from_canonical_bytes, workload_digest},
};

// The canonical codec gives a workload its identity, and a receiver works from
// canonical bytes alone. Both the raw value decoder and the manifest recovery
// path decode untrusted bytes at the admission boundary. The codec must be
// injective: a decoded value re-encodes and decodes back unchanged, and a
// recovered manifest keeps a stable digest across a round trip.
fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();

    if let Ok(value) = canonical::decode(data, &limits) {
        // Re-encoding a value that decoded under these limits must succeed and
        // decode back to the same value.
        if let Ok(bytes) = canonical::encode(&value, &limits) {
            assert!(canonical::decode(&bytes, &limits).unwrap() == value);
        }
    }

    if let Ok(manifest) = manifest_from_canonical_bytes(data, &limits) {
        // Recovering, re-encoding, and recovering again must not change identity.
        let digest = workload_digest(&manifest, &limits).expect("recovered manifest re-encodes");
        let bytes = canonical_bytes(&manifest, &limits).expect("recovered manifest re-encodes");
        let again =
            manifest_from_canonical_bytes(&bytes, &limits).expect("canonical bytes recover");
        assert_eq!(digest, workload_digest(&again, &limits).unwrap());
    }
});
