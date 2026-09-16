#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_variables::{CompiledExpression, Limits};

// A recursive-descent parser over untrusted expression text is a primary fuzz
// target: expressions arrive from MCP clients, hand-edited catalogs, and peer
// manifests. The convenient `parse` path is bounded too, so neither entry point
// may recurse without limit. A parsed expression retains its source, which must
// re-parse stably.
fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(expr) = CompiledExpression::parse(source) {
        let reparsed = CompiledExpression::parse(expr.source())
            .expect("the retained source of a parsed expression re-parses");
        assert_eq!(reparsed.source(), expr.source());
    }

    // A tight caller bound must be enforced, never silently exceeded.
    let strict = Limits {
        max_expression_bytes: 64,
        ..Limits::DEFAULT
    };
    let _ = CompiledExpression::parse_bounded(source, &strict);
});
