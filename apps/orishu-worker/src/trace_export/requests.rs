//! Client-service instrumentation; retains only validated fixed-size context.
use super::{Operation, Outcome, SpanQueue};
use crate::trace_context::TraceParent;
use salvo::prelude::*;

struct RequestParent(TraceParent);

/// Current sampled local client span, not the raw incoming parent. An authorized
/// route may copy it into its bounded IO-owned job; never into a domain command.
pub fn request_parent(depot: &Depot) -> Option<TraceParent> {
    depot
        .get_typed::<RequestParent>()
        .ok()
        .map(|parent| parent.0)
}

/// Client-only middleware. Handler completion is not domain acceptance or
/// socket delivery. Do not attach this to the diagnostics listener.
pub struct TraceRequests<A> {
    queue: SpanQueue,
    authorize: A,
}

impl<A> TraceRequests<A> {
    /// Supply the worker's operator-credential check solely for adopting trace
    /// context. Route handlers must still enforce their own authorization;
    /// diagnostic ancestry neither grants permission nor accepts a command.
    pub fn new(queue: SpanQueue, authorize: A) -> Self {
        Self { queue, authorize }
    }
}

#[handler]
impl<A> TraceRequests<A>
where
    A: Fn(&str) -> bool + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let parent = self.parent(req);
        let active = self
            .queue
            .try_begin_with_parent(Operation::ClientRequest, parent);
        let _ = depot.remove_typed::<RequestParent>();
        if let Some(span) = &active {
            depot.insert_typed(RequestParent(span.context()));
        }
        ctrl.call_next(req, depot, res).await;
        let _ = depot.remove_typed::<RequestParent>();
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

impl<A: Fn(&str) -> bool> TraceRequests<A> {
    fn parent(&self, req: &Request) -> Option<TraceParent> {
        // Local unauthenticated reads remain allowed by their route, but that
        // does not authorize adoption of caller-supplied diagnostic identity.
        // Avoid a redundant credential check on ordinary context-free traffic.
        if !req.headers().contains_key("traceparent") {
            return None;
        }
        let mut credentials = req.headers().get_all("authorization").iter();
        let credential = credentials.next()?;
        if credentials.next().is_some() {
            return None;
        }
        let token = credential.to_str().ok()?.strip_prefix("Bearer ")?;
        if !(self.authorize)(token) {
            return None;
        }
        TraceParent::from_values(
            req.headers()
                .get_all("traceparent")
                .iter()
                .map(|value| value.as_bytes()),
        )
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
        TraceRequests::new(queue, |_: &str| false)
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

    #[tokio::test]
    async fn only_authenticated_single_context_is_adopted_without_changing_responses() {
        const PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let expected = TraceParent::parse(PARENT.as_bytes()).unwrap();
        let combined = format!("{PARENT},{PARENT}");
        let oversized = format!("01{}-{}", &PARENT[2..], "x".repeat(73));
        let unsampled = format!("{}00", &PARENT[..53]);
        let future = format!("01{}-opaque", &PARENT[2..]);
        let calls = Arc::new(AtomicUsize::new(0));
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 1, 1).unwrap();
        let instrument = TraceRequests::new(queue, |token: &str| token == "test-token");
        for (credentials, parents, adopted) in [
            (vec!["Bearer test-token"], vec![PARENT], true),
            (vec!["Bearer test-token"], vec![unsampled.as_str()], true),
            (vec!["Bearer test-token"], vec![future.as_str()], true),
            (vec![], vec![PARENT], false),
            (vec!["Bearer wrong"], vec![PARENT], false),
            (vec!["Basic test-token"], vec![PARENT], false),
            (
                vec!["Bearer test-token", "Bearer test-token"],
                vec![PARENT],
                false,
            ),
            (vec!["Bearer test-token"], vec![], false),
            (vec!["Bearer test-token"], vec![PARENT, PARENT], false),
            (vec!["Bearer test-token"], vec![combined.as_str()], false),
            (vec!["Bearer test-token"], vec![oversized.as_str()], false),
            (
                vec!["Bearer test-token"],
                vec!["invalid-secret-marker"],
                false,
            ),
        ] {
            let mut req = Request::new();
            for credential in credentials {
                req.headers_mut()
                    .append("authorization", credential.parse().unwrap());
            }
            for parent in parents {
                req.headers_mut()
                    .append("traceparent", parent.parse().unwrap());
            }
            req.headers_mut()
                .insert("baggage", "secret-marker".parse().unwrap());
            req.headers_mut()
                .insert("tracestate", "vendor=secret-marker".parse().unwrap());
            let mut res = Response::new();
            instrument
                .handle(
                    &mut req,
                    &mut Depot::new(),
                    &mut res,
                    &mut FlowCtrl::new(vec![Arc::new(Respond(
                        StatusCode::ACCEPTED,
                        calls.clone(),
                    ))]),
                )
                .await;
            assert_eq!(res.status_code, Some(StatusCode::ACCEPTED));
            assert_eq!(res.headers().len(), 1);
            assert_eq!(res.headers().get("x-domain-result").unwrap(), "unchanged");
            let record = receiver.try_recv().unwrap();
            if adopted {
                assert_eq!(record.trace, expected.trace_id());
                assert_eq!(record.parent, Some(expected.span_id()));
            } else {
                assert_ne!(record.trace, expected.trace_id());
                assert!(record.parent.is_none());
            }
            assert_ne!(record.span, expected.span_id());
            assert!(matches!(record.outcome, Outcome::Completed));
        }
        assert_eq!(calls.load(Ordering::Relaxed), 12);

        // A context-free request must not add a credential-check dependency.
        let instrument = TraceRequests::new(instrument.queue, |_: &str| -> bool {
            panic!("context-free request consulted credentials")
        });
        assert!(instrument.parent(&Request::new()).is_none());
    }

    struct Held(Arc<tokio::sync::Notify>);

    struct Child(SpanQueue);
    #[handler]
    impl Child {
        async fn handle(&self, depot: &Depot) {
            if let Some(parent) = request_parent(depot) {
                self.0
                    .try_begin_with_parent(Operation::PeerExchange, Some(parent))
                    .unwrap()
                    .finish(Outcome::Completed);
            }
        }
    }

    #[tokio::test]
    async fn downstream_context_is_local_span_and_cannot_survive_request_or_zero_sampling() {
        let (queue, mut receiver) = SpanQueue::new(1_000_000, 2, 2).unwrap();
        let mut depot = Depot::new();
        TraceRequests::new(queue.clone(), |_: &str| false)
            .handle(
                &mut Request::new(),
                &mut depot,
                &mut Response::new(),
                &mut FlowCtrl::new(vec![Arc::new(Child(queue.clone()))]),
            )
            .await;
        let child = receiver.try_recv().unwrap();
        let root = receiver.try_recv().unwrap();
        assert_eq!(child.parent, Some(root.span));
        assert_eq!(child.trace, root.trace);
        assert!(root.parent.is_none());
        assert!(request_parent(&depot).is_none());
        depot.insert_typed(RequestParent(
            TraceParent::new([1; 16], [2; 8], true).unwrap(),
        ));
        let (zero, mut zero_receiver) = SpanQueue::new(0, 1, 1).unwrap();
        TraceRequests::new(zero, |_: &str| false)
            .handle(
                &mut Request::new(),
                &mut depot,
                &mut Response::new(),
                &mut FlowCtrl::new(vec![Arc::new(Child(queue))]),
            )
            .await;
        assert!(request_parent(&depot).is_none());
        assert!(receiver.try_recv().is_err());
        assert!(zero_receiver.try_recv().is_err());
    }

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
        let instrument = TraceRequests::new(queue.clone(), |_: &str| false);
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
