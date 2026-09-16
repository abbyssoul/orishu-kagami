#![no_main]
use libfuzzer_sys::fuzz_target;
use kagami_document::{WireCommand, WireSnapshot};
use serde::{Serialize, de::DeserializeOwned};

// The wire types are how an authoring command or a document snapshot crosses a
// file or transport boundary into Kagami's sans-IO core. JSON exercises the
// public serde contract: a value that parses must re-serialize into bytes the
// decoder accepts. Equality is deliberately not asserted — geometry types such
// as `Rotation` normalise on deserialize (parse, don't validate), so a decoded
// value's floats are unit-normalised and re-decoding divides by a norm that is
// `1.0` only to within a rounding error. A decoded `WireCommand` is still only
// proposed intent — acceptance runs through `into_command` and the document
// authority's validation.
fn check<T: Serialize + DeserializeOwned>(data: &[u8]) {
    if let Ok(value) = serde_json::from_slice::<T>(data) {
        let bytes = serde_json::to_vec(&value).unwrap();
        serde_json::from_slice::<T>(&bytes).unwrap();
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    check::<WireCommand>(data);
    check::<WireSnapshot>(data);
});
