#![no_main]
use libfuzzer_sys::fuzz_target;
use kagami_session::{WireAcceptance, WireEnvelope, WireEvent, WireRejection, WireSessionCommand};
use serde::{Serialize, de::DeserializeOwned};

// Every UI gesture, MCP call, undo, and redo becomes one command envelope on
// the wire before the document server decides anything. JSON exercises the
// public serde contract of the envelope and its outcomes: a value that parses
// must re-serialize into bytes the decoder accepts. Equality is deliberately not
// asserted — an embedded authoring command carries geometry (`Rotation`) that
// normalises on deserialize, so a re-decode differs from the first decode only
// by a floating-point rounding error. Successful transport is never command
// acceptance.
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
    check::<WireEnvelope>(data);
    check::<WireSessionCommand>(data);
    check::<WireEvent>(data);
    check::<WireAcceptance>(data);
    check::<WireRejection>(data);
});
