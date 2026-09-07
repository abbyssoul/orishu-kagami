//! Optional finite trace-loss catalogue; never reads the collector or owner.
use orishu_worker::trace_export::{DeliveryCounters, ExportStats, QueueStats, SpanQueue};
use std::fmt::Write;

#[derive(Clone)]
pub(crate) struct Traces(pub SpanQueue, pub DeliveryCounters);

impl Traces {
    pub(super) fn write(&self, body: &mut String) {
        let q = self.0.stats();
        let d = self.1.snapshot();
        Self::write_counts(body, &q, &d);
    }

    fn write_counts(body: &mut String, q: &QueueStats, d: &ExportStats) {
        for (name, help, value) in [
            ("sampled_out", "Operations not sampled.", q.sampled_out),
            (
                "active_full",
                "Sampled operations shed at active capacity.",
                q.active_full,
            ),
            (
                "queue_full",
                "Completed spans shed at queue capacity.",
                q.queue_full,
            ),
            (
                "closed",
                "Spans or operations shed after consumer closure.",
                q.closed,
            ),
            (
                "invalid_source",
                "Spans shed on local identity or clock failure.",
                q.invalid_source,
            ),
            (
                "enqueued",
                "Completed spans queued, not delivered.",
                q.enqueued,
            ),
            ("accepted", "Spans collector reported accepted.", d.accepted),
            ("rejected", "Spans collector reported rejected.", d.rejected),
            (
                "failed",
                "Spans in failed attempts; acceptance may be unknown.",
                d.failed,
            ),
            (
                "encoding_dropped",
                "Records shed on encoding failure.",
                d.encoding_dropped,
            ),
            (
                "shutdown_dropped",
                "Records abandoned by shutdown.",
                d.shutdown_dropped,
            ),
            (
                "warnings",
                "Responses carrying discarded collector warnings.",
                d.warnings,
            ),
        ] {
            writeln!(body, "# HELP orishu_worker_trace_{name}_total {help}\n# TYPE orishu_worker_trace_{name}_total counter\norishu_worker_trace_{name}_total {value}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn maximum_trace_catalogue_is_finite_and_bounded() {
        let q = QueueStats {
            sampled_out: u64::MAX,
            active_full: u64::MAX,
            queue_full: u64::MAX,
            closed: u64::MAX,
            invalid_source: u64::MAX,
            enqueued: u64::MAX,
        };
        let d = ExportStats {
            accepted: u64::MAX,
            rejected: u64::MAX,
            failed: u64::MAX,
            encoding_dropped: u64::MAX,
            shutdown_dropped: u64::MAX,
            warnings: u64::MAX,
        };
        let mut body = String::new();
        Traces::write_counts(&mut body, &q, &d);
        assert!(
            body.len() < 4096,
            "maximum trace exposition: {}",
            body.len()
        );
        let samples: Vec<_> = body.lines().filter(|line| !line.starts_with('#')).collect();
        assert_eq!(samples.len(), 12);
        let names: std::collections::BTreeSet<_> = samples
            .iter()
            .map(|line| {
                let (name, value) = line.split_once(' ').unwrap();
                assert!(!name.contains(['{', '}']));
                assert_eq!(value.parse::<u64>().unwrap(), u64::MAX);
                assert!(body.contains(&format!("# TYPE {name} counter\n")));
                name
            })
            .collect();
        assert_eq!(names.len(), 12);
        if let Some(path) = std::env::var_os("ORISHU_TEST_PROMTOOL") {
            use std::{process::Stdio, time::Duration};
            use tokio::io::AsyncWriteExt;
            let mut child = tokio::process::Command::new(path)
                .args(["check", "metrics"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(body.as_bytes())
                .await
                .unwrap();
            let result = tokio::time::timeout(Duration::from_secs(2), child.wait_with_output())
                .await
                .unwrap()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
