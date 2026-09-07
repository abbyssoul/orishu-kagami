//! Fixed-size client service accounting. No request data is retained.
use salvo::prelude::*;
use std::{
    fmt::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

// Microsecond resolution, matching the retained cumulative duration counter.
const BOUNDS: [(u64, &str); 7] = [
    (1_000, "0.001"),
    (5_000, "0.005"),
    (25_000, "0.025"),
    (100_000, "0.1"),
    (500_000, "0.5"),
    (1_000_000, "1"),
    (5_000_000, "5"),
];

/// Process-lifetime aggregate counters shared by client services and scrapes.
#[derive(Default)]
pub(crate) struct Requests {
    active: AtomicU64,
    completed: AtomicU64,
    rejected: AtomicU64,
    failed: AtomicU64,
    cancelled: AtomicU64,
    micros: AtomicU64,
    // Exclusive buckets: one atomic update per observation. The final slot
    // covers every duration beyond the largest finite boundary.
    durations: [AtomicU64; 8],
}

fn add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(value))
    });
}

impl Requests {
    fn complete(&self, status: StatusCode, micros: u64) {
        add(&self.completed, 1);
        if status.is_client_error() {
            add(&self.rejected, 1);
        }
        if status.is_server_error() {
            add(&self.failed, 1);
        }
        add(&self.micros, micros);
        let bucket = BOUNDS
            .iter()
            .position(|(upper, _)| micros <= *upper)
            .unwrap_or(BOUNDS.len());
        add(&self.durations[bucket], 1);
    }

    fn begin(&self) -> Active<'_> {
        self.active.fetch_add(1, Ordering::Relaxed);
        Active {
            metrics: self,
            started: Instant::now(),
            completed: false,
        }
    }

    pub(super) fn write(&self, body: &mut String) {
        for (name, kind, help, value) in [
            (
                "client_requests_in_flight",
                "gauge",
                "Client service handlers currently executing.",
                &self.active,
            ),
            (
                "client_requests_completed_total",
                "counter",
                "Client service handlers completed, not delivered responses.",
                &self.completed,
            ),
            (
                "client_requests_rejected_total",
                "counter",
                "Completed client service responses with HTTP 4xx.",
                &self.rejected,
            ),
            (
                "client_requests_failed_total",
                "counter",
                "Completed client service responses with HTTP 5xx.",
                &self.failed,
            ),
            (
                "client_requests_cancelled_total",
                "counter",
                "Client service futures dropped before completion.",
                &self.cancelled,
            ),
        ] {
            writeln!(body, "# HELP orishu_worker_{name} {help}\n# TYPE orishu_worker_{name} {kind}\norishu_worker_{name} {}", value.load(Ordering::Relaxed)).unwrap();
        }
        let micros = self.micros.load(Ordering::Relaxed);
        writeln!(body, "# HELP orishu_worker_client_request_duration_seconds_total Cumulative completed client handler time, excluding socket delivery.\n# TYPE orishu_worker_client_request_duration_seconds_total counter\norishu_worker_client_request_duration_seconds_total {}.{:06}", micros / 1_000_000, micros % 1_000_000).unwrap();
        const NAME: &str = "orishu_worker_client_request_duration_seconds";
        writeln!(body, "# HELP {NAME} Completed client handler duration, excluding cancellation and socket delivery.\n# TYPE {NAME} histogram").unwrap();
        let mut count = 0_u64;
        for (index, bucket) in self.durations.iter().enumerate() {
            count = count.saturating_add(bucket.load(Ordering::Relaxed));
            let le = BOUNDS.get(index).map_or("+Inf", |(_, label)| *label);
            writeln!(body, "{NAME}_bucket{{le=\"{le}\"}} {count}").unwrap();
        }
        writeln!(
            body,
            "{NAME}_sum {}.{:06}\n{NAME}_count {count}",
            micros / 1_000_000,
            micros % 1_000_000
        )
        .unwrap();
    }
}

struct Active<'a> {
    metrics: &'a Requests,
    started: Instant,
    completed: bool,
}

impl Active<'_> {
    fn finish(mut self, status: StatusCode) {
        self.metrics.complete(
            status,
            self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        );
        self.completed = true;
    }
}

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.metrics.active.fetch_sub(1, Ordering::Relaxed);
        if !self.completed {
            add(&self.metrics.cancelled, 1);
        }
    }
}

/// Client-only service middleware; never attach it to the diagnostics service.
pub(crate) struct Accounting(pub(crate) Arc<Requests>);

#[handler]
impl Accounting {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let active = self.0.begin();
        ctrl.call_next(req, depot, res).await;
        active.finish(res.status_code.unwrap_or(StatusCode::OK));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Held(Arc<tokio::sync::Notify>);
    #[handler]
    impl Held {
        async fn handle(&self) {
            self.0.notify_one();
            std::future::pending::<()>().await;
        }
    }

    #[tokio::test]
    async fn cancelled_service_future_releases_in_flight_without_completing() {
        let metrics = Arc::new(Requests::default());
        let entered = Arc::new(tokio::sync::Notify::new());
        let accounting = Accounting(metrics.clone());
        let handler = Held(entered.clone());
        let task = tokio::spawn(async move {
            let mut req = Request::new();
            let mut depot = Depot::new();
            let mut res = Response::new();
            let mut ctrl = FlowCtrl::new(vec![Arc::new(handler)]);
            accounting
                .handle(&mut req, &mut depot, &mut res, &mut ctrl)
                .await;
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        assert_eq!(metrics.active.load(Ordering::Relaxed), 1);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(metrics.active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.completed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.cancelled.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.micros.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn completion_cancellation_and_saturation_keep_distinct_meanings() {
        let metrics = Requests::default();
        for status in [
            StatusCode::OK,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let active = metrics.begin();
            assert_eq!(metrics.active.load(Ordering::Relaxed), 1);
            active.finish(status);
        }
        drop(metrics.begin());
        assert_eq!(metrics.active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.completed.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.rejected.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.failed.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.cancelled.load(Ordering::Relaxed), 1);
        metrics.completed.store(u64::MAX, Ordering::Relaxed);
        metrics.micros.store(u64::MAX, Ordering::Relaxed);
        metrics.begin().finish(StatusCode::OK);
        assert_eq!(metrics.completed.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(metrics.micros.load(Ordering::Relaxed), u64::MAX);
        for field in [
            &metrics.active,
            &metrics.rejected,
            &metrics.failed,
            &metrics.cancelled,
        ] {
            field.store(u64::MAX, Ordering::Relaxed);
        }
        let mut text = String::new();
        metrics.write(&mut text);
        assert!(text.len() < 4096);
        assert!(text.contains("18446744073709.551615"));
    }

    #[test]
    fn duration_histogram_includes_boundaries_and_excludes_cancelled_work() {
        let metrics = Requests::default();
        for (upper, _) in BOUNDS {
            metrics.complete(StatusCode::OK, upper);
            metrics.complete(StatusCode::BAD_REQUEST, upper + 1);
        }
        drop(metrics.begin());
        let counts: Vec<_> = metrics
            .durations
            .iter()
            .map(|value| value.load(Ordering::Relaxed))
            .collect();
        assert_eq!(counts, [1, 2, 2, 2, 2, 2, 2, 1]);
        assert_eq!(metrics.completed.load(Ordering::Relaxed), 14);
        let mut text = String::new();
        metrics.write(&mut text);
        let prefix = "orishu_worker_client_request_duration_seconds";
        assert!(text.contains(&format!("{prefix}_bucket{{le=\"+Inf\"}} 14\n")));
        assert!(text.contains(&format!("{prefix}_count 14\n")));
        assert!(text.contains(&format!("{prefix}_sum 13.262007\n")));
        for bucket in &metrics.durations {
            bucket.store(u64::MAX, Ordering::Relaxed);
        }
        metrics.complete(StatusCode::OK, 0);
        text.clear();
        metrics.write(&mut text);
        assert!(text.contains(&format!("{prefix}_count {}\n", u64::MAX)));
        assert!(
            metrics
                .durations
                .iter()
                .all(|bucket| bucket.load(Ordering::Relaxed) == u64::MAX)
        );
    }
}
