#![no_main]
use libfuzzer_sys::fuzz_target;
use kagami_catalog::{Limits, load::parse_stream};
use std::path::Path;

// A catalog file can arrive from a hand edit or an untrusted MCP client. The
// bounded loader parses one or more `---`-separated documents from text under an
// explicit `Limits`, reporting per-document diagnostics rather than trusting the
// input. Parsing is not availability or validity: it must refuse, not panic, on
// hostile UTF-8.
fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // `parse_stream` takes the text directly; the path is only a diagnostic
    // label, so no filesystem access occurs here.
    let _ = parse_stream(Path::new("fuzz.catalog.yaml"), text, &Limits::default());
});
