//! Finite optional logging counter projection; never waits for the sink.
use orishu_worker::operational_log::Stats;
use std::fmt::Write;

pub(super) fn write(body: &mut String, counters: &Stats) {
    for (name, help, value) in [
        (
            "accepted",
            "Records queued, not delivered.",
            counters.accepted,
        ),
        (
            "written",
            "Complete writes and flushes acknowledged, not durable.",
            counters.written,
        ),
        (
            "queue_full",
            "Records shed at queued capacity.",
            counters.queue_full,
        ),
        (
            "contended",
            "Records shed without waiting for the queue lock.",
            counters.contended,
        ),
        (
            "encoding_failed",
            "Records refused by bounded encoding.",
            counters.encoding_failed,
        ),
        (
            "invalid_source",
            "Lifecycle records shed on timestamp failure.",
            counters.invalid_source,
        ),
        (
            "output_failed",
            "Records not fully written after terminal sink failure.",
            counters.output_failed,
        ),
        (
            "closed",
            "Records refused after closure or sink failure.",
            counters.closed,
        ),
        (
            "shutdown_dropped",
            "Never-started records discarded at shutdown cutoff.",
            counters.shutdown_dropped,
        ),
    ] {
        let _ = writeln!(
            body,
            "# HELP orishu_worker_log_{name}_total {help}\n# TYPE orishu_worker_log_{name}_total counter\norishu_worker_log_{name}_total {value}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_counters_have_nine_unlabelled_series_with_bounded_bytes() {
        let mut body = String::new();
        write(
            &mut body,
            &Stats {
                accepted: u64::MAX,
                written: u64::MAX,
                queue_full: u64::MAX,
                contended: u64::MAX,
                encoding_failed: u64::MAX,
                invalid_source: u64::MAX,
                output_failed: u64::MAX,
                closed: u64::MAX,
                shutdown_dropped: u64::MAX,
            },
        );
        assert_eq!(
            body.lines()
                .filter(|line| line.starts_with("orishu_worker_log_"))
                .count(),
            9
        );
        assert!(!body.contains('{'));
        assert!(body.len() < 3072);
    }
}
