#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_plugin::{
    Limits,
    bundle::{self, BundleLimits},
};

// `bundle::read` establishes archive framing, root/blob closure, CRCs, and
// digest-verified declarations over caller-owned stored-ZIP bytes before
// returning anything. No path is ever extracted to a filesystem. Aggregate
// bytes and physical entry count are bounded before work or allocation, so
// malformed, truncated, oversized, and overlapping archives must be refused
// rather than crash.
fuzz_target!(|data: &[u8]| {
    let _ = bundle::read(data, &Limits::default(), BundleLimits::default());
});
