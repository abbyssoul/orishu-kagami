use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};

fn unknown_root() -> orishu_workload::WorkloadDigest {
    format!("sha256:{}", "00".repeat(32)).parse().unwrap()
}
struct NeverRead;
impl AsyncRead for NeverRead {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        panic!("policy must refuse before polling input")
    }
}
struct Broken;
impl AsyncRead for Broken {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("sensitive transport detail")))
    }
}

struct Stall {
    first: bool,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    dropped: Arc<AtomicBool>,
}
impl AsyncRead for Stall {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.first {
            self.first = false;
            buffer.put_slice(b"x");
            return Poll::Ready(Ok(()));
        }
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
        }
        Poll::Pending
    }
}
impl Drop for Stall {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}
fn stall() -> (Stall, tokio::sync::oneshot::Receiver<()>, Arc<AtomicBool>) {
    let (entered, observed) = tokio::sync::oneshot::channel();
    let dropped = Arc::new(AtomicBool::new(false));
    (
        Stall {
            first: true,
            entered: Some(entered),
            dropped: dropped.clone(),
        },
        observed,
        dropped,
    )
}

#[tokio::test]
async fn delivery_policy_refuses_before_reading_and_releases_its_reservation() {
    let (mut service, handle, owner) = worker().await;
    for length in [0, 9, u64::MAX] {
        let job = prepared(&service, control()).await;
        let fence = job.fence();
        assert!(matches!(
            job.receive(
                NeverRead,
                length,
                unknown_root(),
                DeliveryLimits {
                    bytes: 8,
                    ..Default::default()
                }
            )
            .await,
            Err(AdmissionError::Limit)
        ));
        assert!(!fence.is_current());
    }
    service.archive.max_bytes = 1;
    assert!(matches!(
        prepared(&service, control())
            .await
            .receive(NeverRead, 2, unknown_root(), Default::default())
            .await,
        Err(AdmissionError::Limit)
    ));
    assert!(matches!(
        prepared(&service, control())
            .await
            .receive(
                NeverRead,
                1,
                unknown_root(),
                DeliveryLimits {
                    timeout: Duration::ZERO,
                    ..Default::default()
                }
            )
            .await,
        Err(AdmissionError::Delivery(DeliveryError::Deadline))
    ));
    let mut job = prepared(&service, control()).await;
    job.control = OperationControl::new(Duration::ZERO)
        .unwrap()
        .with_parent(job.fence().cancellation())
        .unwrap();
    assert!(matches!(
        job.receive(NeverRead, 1, unknown_root(), Default::default())
            .await,
        Err(AdmissionError::Interrupted(Interruption::DeadlineExceeded))
    ));
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn truncation_excess_transport_and_invalid_closure_never_enter_scientific_state() {
    let (service, handle, owner) = worker().await;
    for announced in [2, 4] {
        let job = prepared(&service, control()).await;
        let fence = job.fence();
        assert!(matches!(
            job.receive(&b"abc"[..], announced, unknown_root(), Default::default())
                .await,
            Err(AdmissionError::Delivery(DeliveryError::Length))
        ));
        assert!(!fence.is_current());
    }
    let error = prepared(&service, control())
        .await
        .receive(Broken, 1, unknown_root(), Default::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(error, AdmissionError::Delivery(DeliveryError::Io)));
    assert!(!error.to_string().contains("sensitive"));
    assert!(matches!(
        prepared(&service, control())
            .await
            .receive(&b"abc"[..], 3, unknown_root(), Default::default())
            .await,
        Err(AdmissionError::Closure(_))
    ));
    assert!(handle.view().unwrap().scientific.is_none());
    released(&service).await;
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn stalled_eof_holds_exclusivity_and_observes_cancel_abort_and_shutdown() {
    for mode in 0..3 {
        let (service, handle, owner) = worker().await;
        let cancel = control();
        let job = prepared(&service, cancel.clone()).await;
        let fence = job.fence();
        let (input, observed, dropped) = stall();
        let pending = tokio::spawn(job.receive(input, 1, unknown_root(), Default::default()));
        tokio::time::timeout(Duration::from_secs(2), observed)
            .await
            .unwrap()
            .unwrap();
        let view = handle.view().unwrap();
        assert!(matches!(
            service
                .prepare(view.generation, view.summary.formation_id, control())
                .await,
            Err(AdmissionError::Reservation(ExecutionError::Busy))
        ));
        match mode {
            0 => cancel.cancel(),
            1 => pending.abort(),
            _ => handle.shutdown().await.unwrap(),
        }
        let outcome = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap();
        if mode == 1 {
            assert!(matches!(outcome, Err(e) if e.is_cancelled()));
        } else {
            assert!(matches!(
                outcome.unwrap(),
                Err(AdmissionError::Interrupted(Interruption::Cancelled))
            ));
        }
        assert!(dropped.load(Ordering::SeqCst));
        assert!(!fence.is_current());
        if mode != 2 {
            released(&service).await;
            handle.shutdown().await.unwrap();
        }
        owner.await.unwrap().unwrap();
    }
}

struct Trickle(tokio::time::Interval);
impl AsyncRead for Trickle {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.0.poll_tick(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                buffer.put_slice(b"x");
                Poll::Ready(Ok(()))
            }
        }
    }
}
#[tokio::test]
async fn absolute_delivery_deadline_is_not_renewed_by_progress_or_waiting_for_eof() {
    let (service, handle, owner) = worker().await;
    let budget = DeliveryLimits {
        timeout: Duration::from_millis(100),
        ..Default::default()
    };
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        prepared(&service, control()).await.receive(
            Trickle(tokio::time::interval(Duration::from_millis(20))),
            1024,
            unknown_root(),
            budget,
        ),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(AdmissionError::Delivery(DeliveryError::Deadline))
    ));
    let (input, _, dropped) = stall();
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            prepared(&service, control())
                .await
                .receive(input, 1, unknown_root(), budget)
        )
        .await
        .unwrap(),
        Err(AdmissionError::Delivery(DeliveryError::Deadline))
    ));
    assert!(dropped.load(Ordering::SeqCst));
    released(&service).await;
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

struct Quantum<R> {
    inner: R,
    maximum: Arc<AtomicUsize>,
}
impl<R: AsyncRead + Unpin> AsyncRead for Quantum<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.maximum.fetch_max(buffer.remaining(), Ordering::SeqCst);
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}
struct OwnedBody {
    bytes: Arc<[u8]>,
    position: usize,
    dropped: Arc<AtomicBool>,
}
impl AsyncRead for OwnedBody {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let count = buffer.remaining().min(self.bytes.len() - self.position);
        buffer.put_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Poll::Ready(Ok(()))
    }
}
impl Drop for OwnedBody {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn socket_body_reaches_real_worker_run_and_wrong_root_cannot_allocate_an_epoch() {
    use crate::workload_receipts::{BeginLoad, ReceiptStore};
    use orishu::model::run_load::{LoadOutcome, LoadRequest, LoadState};
    use std::os::unix::fs::PermissionsExt;
    let _slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let (service, handle, owner) = worker().await;
    assert!(
        matches!(prepared(&service, control()).await.receive(bytes.as_ref(), bytes.len() as u64, unknown_root(), Default::default()).await, Err(AdmissionError::UnexpectedWorkload { expected, actual }) if expected == unknown_root() && actual == root)
    );
    let (mut sender, reader) = tokio::net::UnixStream::pair().unwrap();
    let maximum = Arc::new(AtomicUsize::new(0));
    let input = Quantum {
        inner: reader,
        maximum: maximum.clone(),
    };
    let job = prepared(&service, control()).await;
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.path().to_owned();
    let request = LoadRequest::new(
        "socket-load".parse().unwrap(),
        job.fence().formation().clone(),
        root,
    );
    let source = job.fence().node().clone();
    let (store, ticket) = tokio::task::spawn_blocking(move || {
        let mut store = ReceiptStore::open(&path).unwrap();
        let BeginLoad::Started(ticket) = store.begin(request, source).unwrap() else {
            panic!("new operation");
        };
        (store, ticket)
    })
    .await
    .unwrap();
    let len = bytes.len() as u64;
    let send = tokio::spawn(async move {
        for chunk in bytes.chunks(4093) {
            sender.write_all(chunk).await.unwrap();
        }
        sender.shutdown().await.unwrap();
    });
    let candidate = job
        .receive(input, len, root, Default::default())
        .await
        .unwrap();
    send.await.unwrap();
    assert!(maximum.load(Ordering::SeqCst) <= 64 * 1024);
    assert_eq!(
        candidate.scope().epoch,
        1,
        "wrong-root delivery did not consume a run epoch"
    );
    assert_eq!(candidate.scope().workload, root);
    assert!(
        handle.view().unwrap().scientific.is_none(),
        "delivery/admission is not publication"
    );
    assert_eq!(
        store
            .lookup(ticket.receipt().request())
            .unwrap()
            .unwrap()
            .state(),
        &LoadState::Pending
    );
    let descriptor = candidate.descriptor().clone();
    let run = candidate
        .confirm()
        .await
        .unwrap()
        .start(control())
        .await
        .unwrap();
    assert_eq!(run.view().unwrap().boundary(), 0);
    let receipt = tokio::task::spawn_blocking(move || {
        let mut store = store;
        // Only successful initial publication justifies a terminal acceptance.
        store
            .finish(&ticket, LoadOutcome::Accepted { descriptor })
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(run.step(control()).await.unwrap().boundary(), 1);
    run.unload();
    released(&service).await;
    let path = directory.path().to_owned();
    tokio::task::spawn_blocking(move || {
        let mut reopened = ReceiptStore::open(&path).unwrap();
        let BeginLoad::Replay(replayed) = reopened
            .begin(receipt.request().clone(), "restarted-node".parse().unwrap())
            .unwrap()
        else {
            panic!("accepted request must never execute twice");
        };
        assert_eq!(replayed, receipt, "acceptance is history even after unload");
    })
    .await
    .unwrap();
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn abort_after_delivery_keeps_the_native_job_lease_until_actual_exit() {
    let _slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let (service, handle, owner) = worker().await;
    let cancel = control();
    let mut job = prepared(&service, cancel.clone()).await;
    let fence = job.fence();
    let (observed, release) = hold(&mut job);
    let dropped = Arc::new(AtomicBool::new(false));
    let length = bytes.len() as u64;
    let reader = OwnedBody {
        bytes,
        position: 0,
        dropped: dropped.clone(),
    };
    let pending = tokio::spawn(job.receive(reader, length, root, Default::default()));
    entered(observed).await;
    assert!(
        dropped.load(Ordering::SeqCst),
        "transport reader is closed before native validation starts"
    );
    pending.abort();
    assert!(matches!(pending.await, Err(e) if e.is_cancelled()));
    assert!(cancel.check().is_err());
    assert!(
        fence.is_current(),
        "native work still owns its lease after receiver loss"
    );
    let view = handle.view().unwrap();
    assert!(matches!(
        service
            .prepare(view.generation, view.summary.formation_id, control())
            .await,
        Err(AdmissionError::Reservation(ExecutionError::Busy))
    ));
    release.send(()).unwrap();
    released(&service).await;
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}
