//! Static client-service instrumentation; never captures headers or payloads.
use super::{Operation, Outcome, SpanQueue};
use salvo::prelude::*;

/// Client-only middleware. Handler completion is not domain acceptance or
/// socket delivery. Do not attach this to the diagnostics listener.
pub struct TraceRequests(pub SpanQueue);

#[handler]
impl TraceRequests {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let active = self.0.try_begin(Operation::ClientRequest);
        ctrl.call_next(req, depot, res).await;
        if let Some(active) = active {
            let status = res.status_code.unwrap_or(StatusCode::OK);
            active.finish(if status.is_server_error() {
                Outcome::Failed
            } else if status.is_client_error() {
                Outcome::Rejected
            } else {
                Outcome::Completed
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct Respond(StatusCode, Arc<AtomicUsize>);
    #[handler]
    impl Respond {
        async fn handle(&self, res: &mut Response) {
            self.1.fetch_add(1, Ordering::Relaxed);
            res.status_code(self.0);
            res.add_header("x-domain-result", "unchanged", true)
                .unwrap();
        }
    }

    async fn invoke(queue: SpanQueue, status: StatusCode, calls: Arc<AtomicUsize>) {
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(vec![Arc::new(Respond(status, calls))]);
        TraceRequests(queue)
            .handle(&mut req, &mut depot, &mut res, &mut ctrl)
            .await;
        assert_eq!(res.status_code, Some(status));
        assert_eq!(res.headers().get("x-domain-result").unwrap(), "unchanged");
    }

    #[tokio::test]
    async fn response_outcomes_and_telemetry_shedding_preserve_handler_results() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        for (status, expected) in [
            (StatusCode::OK, Outcome::Completed),
            (StatusCode::ACCEPTED, Outcome::Completed),
            (StatusCode::UNAUTHORIZED, Outcome::Rejected),
            (StatusCode::SERVICE_UNAVAILABLE, Outcome::Failed),
        ] {
            invoke(queue.clone(), status, calls.clone()).await;
            let record = receiver.try_recv().unwrap();
            assert_eq!(
                std::mem::discriminant(&record.outcome),
                std::mem::discriminant(&expected)
            );
        }
        invoke(queue.clone(), StatusCode::OK, calls.clone()).await;
        invoke(queue.clone(), StatusCode::OK, calls.clone()).await;
        assert_eq!(queue.stats().queue_full, 1);
        let held = queue.try_begin(Operation::Admission).unwrap();
        invoke(queue.clone(), StatusCode::OK, calls.clone()).await;
        assert_eq!(queue.stats().active_full, 1);
        drop(held);
        drop(receiver);
        invoke(queue.clone(), StatusCode::OK, calls.clone()).await;
        assert_eq!(queue.stats().closed, 1);
        let (zero, mut receiver) = SpanQueue::new(0, 1, 1).unwrap();
        invoke(zero.clone(), StatusCode::OK, calls.clone()).await;
        assert_eq!(zero.stats().sampled_out, 1);
        assert!(receiver.try_recv().is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 9);
    }

    struct Held(Arc<tokio::sync::Notify>);
    #[handler]
    impl Held {
        async fn handle(&self) {
            self.0.notify_one();
            std::future::pending::<()>().await;
        }
    }

    #[tokio::test]
    async fn cancelled_handler_releases_active_span_and_reports_cancellation() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let instrument = TraceRequests(queue.clone());
        let handler = Held(entered.clone());
        let task = tokio::spawn(async move {
            instrument
                .handle(
                    &mut Request::new(),
                    &mut Depot::new(),
                    &mut Response::new(),
                    &mut FlowCtrl::new(vec![Arc::new(handler)]),
                )
                .await;
        });
        tokio::time::timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(matches!(
            receiver.try_recv().unwrap().outcome,
            Outcome::Cancelled
        ));
        assert!(queue.try_begin(Operation::ClientRequest).is_some());
    }
}
