//! Experimental fixed-profile serving: authentication, bounded framing, then
//! daemon ownership. No HTTP request owns a run or selects installed plugins.
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use orishu::model::{ApiResponse, ResponseData, run_load::LoadRequest};
use orishu_worker::{
    runtime::RunningWorker, workload_load::LoadError, workload_receipts::ReceiptStoreError,
};
use salvo::{
    http::{Body, ReqBody},
    prelude::*,
};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

const MAX_REQUEST: usize = orishu::model::run_load::MAX_LOAD_REQUEST_BYTES;
const MAX_UPLOAD: u64 = 128 * 1024 * 1024 + MAX_REQUEST as u64 + 4;
const MEDIA: &str = orishu::model::run_load::RUN_LOAD_MEDIA_TYPE;
const METADATA_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(65);

pub(super) fn routes(
    router: Router,
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
) -> Router {
    if runtime.load_coordinator().is_none() {
        return router;
    }
    // Shared by every scientific route/listener; independent of membership
    // mutation capacity. The coordinator also permits only one active load.
    router
        .push(
            Router::with_path("api/v1/run-loads").post(ScientificHandler {
                runtime: runtime.clone(),
                capacity: capacity.clone(),
                operation: Operation::Submit,
            }),
        )
        .push(
            Router::with_path("api/v1/run-loads/lookup").post(ScientificHandler {
                runtime: runtime.clone(),
                capacity: capacity.clone(),
                operation: Operation::Lookup,
            }),
        )
        .push(Router::with_path("api/v1/run").get(ScientificHandler {
            runtime,
            capacity,
            operation: Operation::Current,
        }))
}

#[derive(Clone, Copy)]
enum Operation {
    Submit,
    Lookup,
    Current,
}
struct ScientificHandler {
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
    operation: Operation,
}

fn header<'a>(req: &'a Request, name: &str) -> Result<Option<&'a str>, (StatusCode, &'static str)> {
    let mut values = req.headers().get_all(name).iter();
    let value = values
        .next()
        .map(|value| {
            value
                .to_str()
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidHeader"))
        })
        .transpose()?;
    if values.next().is_some() {
        return Err((StatusCode::BAD_REQUEST, "DuplicateHeader"));
    }
    Ok(value)
}
fn length(req: &Request, maximum: u64) -> Result<u64, (StatusCode, &'static str)> {
    let text =
        header(req, "content-length")?.ok_or((StatusCode::LENGTH_REQUIRED, "LengthRequired"))?;
    if text.is_empty()
        || text.len() > 20
        || !text.bytes().all(|b| b.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
    }
    let value = text
        .parse::<u64>()
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, "BodyLimit"))?;
    if value > maximum {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "BodyLimit"));
    }
    Ok(value)
}

#[handler]
impl ScientificHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        // Every scientific route authenticates, including local reads. Do not
        // parse metadata or poll a body with an unrecognized credential.
        let valid = header(req, "authorization")
            .ok()
            .flatten()
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|token| self.runtime.authorize_operator(token));
        if !valid {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            super::api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(_permit) = self.capacity.try_acquire() else {
            super::api_error(res, StatusCode::SERVICE_UNAVAILABLE, "Overloaded");
            return;
        };
        let Some(coordinator) = self.runtime.load_coordinator() else {
            super::api_error(
                res,
                StatusCode::SERVICE_UNAVAILABLE,
                "ScientificUnavailable",
            );
            return;
        };
        let mut response_status = StatusCode::OK;
        let result = async {
            if req.uri().query().is_some()
                || req.headers().contains_key("if-match")
                || req.headers().contains_key("content-encoding")
                || req.headers().contains_key("transfer-encoding")
                || req.headers().contains_key("trailer")
            {
                return Err((StatusCode::BAD_REQUEST, "UnsupportedFraming"));
            }
            if matches!(self.operation, Operation::Current) {
                if header(req, "content-length")?.is_some_and(|length| length != "0") {
                    return Err((StatusCode::BAD_REQUEST, "UnexpectedBody"));
                }
                // HTTP/2 may carry DATA without a Content-Length. Confirm real
                // EOF rather than treating an absent declaration as no body.
                let mut body = HttpBody::new(req.take_body());
                let mut probe = [0; 1];
                let count = tokio::time::timeout(METADATA_TIMEOUT, body.read(&mut probe))
                    .await
                    .map_err(|_| (StatusCode::REQUEST_TIMEOUT, "MetadataDeadline"))?
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
                if count != 0 {
                    return Err((StatusCode::BAD_REQUEST, "UnexpectedBody"));
                }
                return coordinator
                    .retained_descriptor()
                    .map(ResponseData::RetainedRun)
                    .map_err(load_error);
            }
            let expected_type = if matches!(self.operation, Operation::Submit) {
                MEDIA
            } else {
                "application/cbor"
            };
            if header(req, "content-type")? != Some(expected_type) {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "UnsupportedMediaType"));
            }
            let total = length(
                req,
                if matches!(self.operation, Operation::Submit) {
                    MAX_UPLOAD
                } else {
                    MAX_REQUEST as u64
                },
            )?;
            let mut body = HttpBody::new(req.take_body());
            if matches!(self.operation, Operation::Lookup) {
                let request = tokio::time::timeout(METADATA_TIMEOUT, async {
                    let mut bytes = Vec::new();
                    (&mut body)
                        .take(total + 1)
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
                    if bytes.len() as u64 != total {
                        return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
                    }
                    orishu_worker::peer::codec::decode::<LoadRequest>(&bytes)
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))
                })
                .await
                .map_err(|_| (StatusCode::REQUEST_TIMEOUT, "MetadataDeadline"))??;
                return coordinator
                    .lookup(&request)
                    .map_err(load_error)?
                    .map(ResponseData::RunLoadReceipt)
                    .ok_or((StatusCode::NOT_FOUND, "OperationNotFound"));
            }
            let (request, archive_length) = tokio::time::timeout(METADATA_TIMEOUT, async {
                if total < 5 {
                    return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
                }
                let size = body
                    .read_u32()
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?
                    as usize;
                if size == 0 || size > MAX_REQUEST || size as u64 + 4 >= total {
                    return Err((StatusCode::BAD_REQUEST, "InvalidRequestLength"));
                }
                let mut bytes = vec![0; size];
                body.read_exact(&mut bytes)
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
                let request = orishu_worker::peer::codec::decode::<LoadRequest>(&bytes)
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
                Ok((request, total - 4 - size as u64))
            })
            .await
            .map_err(|_| (StatusCode::REQUEST_TIMEOUT, "MetadataDeadline"))??;
            // This receiver is not the load's owner. Timeout/connection loss does
            // not abort already submitted admission or dispose a retained run.
            let (eof, received) = tokio::sync::oneshot::channel();
            body.eof = Some(eof);
            let submission = coordinator
                .submit(request.clone(), body, archive_length)
                .map_err(load_error)?;
            tokio::time::timeout(RESPONSE_TIMEOUT, async {
                let outcome = submission.outcome();
                tokio::pin!(outcome);
                tokio::select! {
                    result = &mut outcome => result.map(ResponseData::RunLoadReceipt).map_err(load_error),
                    result = received => {
                        if result.is_err() {
                            return outcome.await.map(ResponseData::RunLoadReceipt).map_err(load_error);
                        }
                        // Exact HTTP body EOF is observed before replying. This
                        // means received, not scientifically accepted: compilation
                        // continues under the daemon's independently retained job.
                        let receipt = coordinator.lookup(&request).map_err(load_error)?.ok_or((StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown"))?;
                        if receipt.state() == &orishu::model::run_load::LoadState::Pending { response_status = StatusCode::ACCEPTED; }
                        Ok(ResponseData::RunLoadReceipt(receipt))
                    }
                }
            }).await.map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "OutcomeUnknown"))?
        }
        .await;
        match result {
            Ok(data) => {
                res.status_code(response_status);
                super::write_api_response(res, &ApiResponse::Ok { data: Some(data) });
            }
            Err((status, code)) => super::api_error(res, status, code),
        }
    }
}

fn load_error(error: LoadError) -> (StatusCode, &'static str) {
    match error {
        LoadError::Busy => (StatusCode::CONFLICT, "WorkloadBusy"),
        LoadError::Receipt(ReceiptStoreError::Conflict) => {
            (StatusCode::CONFLICT, "OperationConflict")
        }
        LoadError::Receipt(ReceiptStoreError::Full) => {
            (StatusCode::SERVICE_UNAVAILABLE, "OperationHistoryFull")
        }
        LoadError::Receipt(_) | LoadError::OutcomeUnknown => {
            (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown")
        }
        _ => (StatusCode::SERVICE_UNAVAILABLE, "ScientificUnavailable"),
    }
}

/// One data frame retained at a time; no whole-body buffering or trailer erasure.
struct HttpBody {
    body: ReqBody,
    remaining: <ReqBody as Body>::Data,
    eof: Option<tokio::sync::oneshot::Sender<()>>,
}
impl HttpBody {
    fn new(body: ReqBody) -> Self {
        Self {
            body,
            remaining: Default::default(),
            eof: None,
        }
    }
}
impl AsyncRead for HttpBody {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        target: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if target.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        for _ in 0..16 {
            if !self.remaining.is_empty() {
                let count = target.remaining().min(self.remaining.len());
                target.put_slice(&self.remaining.split_to(count));
                return Poll::Ready(Ok(()));
            }
            match Pin::new(&mut self.body).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(bytes) => self.remaining = bytes,
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::other(
                            "workload trailers are not supported",
                        )));
                    }
                },
                Poll::Ready(Some(Err(_))) => {
                    return Poll::Ready(Err(io::Error::other("workload body IO failed")));
                }
                Poll::Ready(None) => {
                    if let Some(eof) = self.eof.take() {
                        let _ = eof.send(());
                    }
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo::http::body::Frame;
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };
    type Data = <ReqBody as Body>::Data;
    struct Frames {
        frames: VecDeque<Result<Frame<Data>, salvo::BoxedError>>,
        polls: Arc<AtomicUsize>,
    }
    impl Body for Frames {
        type Data = Data;
        type Error = salvo::BoxedError;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Data>, Self::Error>>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.frames.pop_front())
        }
    }
    fn body(frames: Vec<Result<Frame<Data>, salvo::BoxedError>>) -> (HttpBody, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let body = ReqBody::Boxed {
            inner: Box::pin(Frames {
                frames: frames.into(),
                polls: polls.clone(),
            }),
            fuse_config: None,
        };
        (HttpBody::new(body), polls)
    }
    #[tokio::test]
    async fn data_is_consumed_in_order_and_complete_eof_is_signalled_only_once() {
        let (mut input, polls) = body(vec![
            Ok(Frame::data(vec![1, 2, 3].into())),
            Ok(Frame::data(vec![4, 5].into())),
        ]);
        let (signal, mut eof) = tokio::sync::oneshot::channel();
        input.eof = Some(signal);
        assert_eq!(input.read_u8().await.unwrap(), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(eof.try_recv().is_err());
        let mut tail = Vec::new();
        input.read_to_end(&mut tail).await.unwrap();
        assert_eq!(tail, [2, 3, 4, 5]);
        eof.await.unwrap();
        assert_eq!(input.read(&mut [0]).await.unwrap(), 0);
    }
    #[tokio::test]
    async fn trailers_and_body_errors_are_not_mistaken_for_complete_input() {
        for frame in [
            Ok(Frame::trailers(Default::default())),
            Err(io::Error::other("untrusted stream detail").into()),
        ] {
            let (mut input, _) = body(vec![frame]);
            let (signal, mut eof) = tokio::sync::oneshot::channel();
            input.eof = Some(signal);
            let error = input.read(&mut [0]).await.unwrap_err();
            assert!(!error.to_string().contains("untrusted stream detail"));
            assert!(eof.try_recv().is_err());
        }
    }
    #[tokio::test]
    async fn empty_frames_yield_after_a_bounded_poll_quantum() {
        let (mut input, polls) = body((0..40).map(|_| Ok(Frame::data(Data::new()))).collect());
        let mut bytes = [0];
        let mut target = ReadBuf::new(&mut bytes);
        let mut context = Context::from_waker(std::task::Waker::noop());
        assert!(
            Pin::new(&mut input)
                .poll_read(&mut context, &mut target)
                .is_pending()
        );
        assert_eq!(polls.load(Ordering::SeqCst), 16);
        assert_eq!(input.read(&mut bytes).await.unwrap(), 0);
    }
}
