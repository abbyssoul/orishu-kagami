#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_plugin::{
    KnownPoint, Limits, payload_from_cbor, payload_from_json, release_from_cbor, release_from_json,
};

// The bounded JSON and CBOR readers are the untrusted-input acceptance API for
// plugin declarations; the raw serde derives are not. Every reader takes an
// explicit `Limits` and must refuse, not panic, on hostile bytes. A decode is
// integrity/bounds only — never activation or workload admission.
const POINTS: [KnownPoint; 6] = [
    KnownPoint::Components,
    KnownPoint::Fields,
    KnownPoint::Observables,
    KnownPoint::Constants,
    KnownPoint::FieldModels,
    KnownPoint::Integrators,
];

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();

    let _ = release_from_json(data, &limits);
    let _ = release_from_cbor(data, &limits);

    for point in POINTS {
        let _ = payload_from_json(point, data, &limits);
        let _ = payload_from_cbor(point, data, &limits);
    }
});
