#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_plugin::execution::{
    Batch, BulkLimits, CoupledEntity, DynamicEntity, EntityId, Force, ObjectState,
};

// The scientific bulk-IO packets are the borrowed, allocation-free data
// contracts the fixed-run owner and independent workload admission consume.
// `Batch::read` refuses byte/count excess and framing before scanning, then
// validates every record and strict identity ordering before exposing any
// record — so decoding a hostile packet must refuse rather than crash, and a
// packet that validates is fully validated (its length is safe to read).
fn check<R: orishu_plugin::execution::BulkRecord>(data: &[u8]) {
    if let Ok(batch) = Batch::<R>::read(data, BulkLimits::default()) {
        let _ = batch.len();
    }
}

fuzz_target!(|data: &[u8]| {
    check::<ObjectState>(data);
    check::<DynamicEntity>(data);
    check::<CoupledEntity>(data);
    check::<Force>(data);
    check::<EntityId>(data);
});
