//! Real Unix transport pressure through the production server and summary handler.
use super::*;
use salvo::conn::{Accepted, Acceptor, Holding, tcp::TcpCoupler, unix::UnixAcceptor};
use salvo::fuse::ArcFusePolicy;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

#[derive(Default)]
pub(super) struct WriteEvidence {
    pub(super) pending: AtomicBool,
    pub(super) timed_out: AtomicBool,
    pub(super) dropped: AtomicBool,
    pub(super) bytes: AtomicUsize,
}

impl WriteEvidence {
    fn record(&self, result: &Poll<io::Result<usize>>) {
        match result {
            Poll::Pending => {
                self.pending.store(true, Ordering::SeqCst);
            }
            Poll::Ready(Ok(count)) => {
                self.bytes.fetch_add(*count, Ordering::SeqCst);
            }
            Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::TimedOut => {
                self.timed_out.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

pub(super) struct CountRequests(pub(super) Arc<AtomicUsize>);

#[handler]
impl CountRequests {
    async fn handle(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) struct Observed<S> {
    inner: S,
    evidence: Arc<WriteEvidence>,
}

impl<S> Drop for Observed<S> {
    fn drop(&mut self) {
        self.evidence.dropped.store(true, Ordering::SeqCst);
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Observed<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Observed<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, bytes);
        self.evidence.record(&result);
        result
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        self.evidence.record(&result);
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub(super) struct ObserveFirst {
    pub(super) inner: UnixAcceptor,
    pub(super) first: Option<Arc<WriteEvidence>>,
}

impl Acceptor for ObserveFirst {
    type Stream = Observed<<UnixAcceptor as Acceptor>::Stream>;
    type Coupler = TcpCoupler<Self::Stream>;
    fn holdings(&self) -> &[Holding] {
        self.inner.holdings()
    }
    async fn accept(
        &mut self,
        policy: Option<ArcFusePolicy>,
    ) -> io::Result<Accepted<Self::Coupler, Self::Stream>> {
        let accepted = self.inner.accept(policy).await?;
        let evidence = self.first.take().unwrap_or_default();
        Ok(accepted.map_into(|_| TcpCoupler::new(), |inner| Observed { inner, evidence }))
    }
}

#[tokio::test]
async fn assembly_expiry_reclaims_the_only_accepted_connection_slot() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("assembly.sock");
        let evidence = Arc::new(WriteEvidence::default());
        let acceptor = ObserveFirst {
            inner: UnixListener::new(socket.clone()).bind().await,
            first: Some(evidence.clone()),
        };
        let server = client_server(acceptor).max_connections(1);
        let handle = server.handle();
        let calls = Arc::new(AtomicUsize::new(0));
        let router = Router::new().get(CountRequests(calls.clone()));
        let task = tokio::spawn(server.try_serve(router));
        let mut held = tokio::net::UnixStream::connect(&socket).await.unwrap();
        held.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0")
            .await
            .unwrap();
        let mut header = [0; 9];
        held.read_exact(&mut header).await.unwrap();
        assert_eq!(header[3], 4);
        let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
        assert!(length <= 16384);
        held.read_exact(&mut vec![0; length]).await.unwrap();
        held.write_all(&[0, 0, 0, 4, 1, 0, 0, 0, 0, 0, 0x40, 0, 1, 5, 0, 0, 0, 1])
            .await
            .unwrap();

        let mut queued = tokio::net::UnixStream::connect(&socket).await.unwrap();
        queued
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_secs(1), queued.read(&mut byte))
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!evidence.dropped.load(Ordering::SeqCst));
        let mut response = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(6),
            (&mut queued).take(8193).read_to_end(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 ") && response.len() <= 8192);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(evidence.dropped.load(Ordering::SeqCst));
        // The test has never closed the stalled client to release capacity.
        let mut tail = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(1),
            held.take(8193).read_to_end(&mut tail),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(tail.len() <= 8192);
        handle.stop_graceful(Some(Duration::from_secs(1)));
        task.await.unwrap().unwrap();
    })
    .await
    .expect("assembly reclamation fixture budget");
}

#[tokio::test]
async fn stalled_summary_reader_times_out_without_blocking_control() {
    tokio::time::timeout(Duration::from_secs(12), async {
        let root = tempfile::tempdir().unwrap();
        let credentials = WorkerCredentials::load_or_create(&root.path().join("state")).unwrap();
        let runtime = WorkerRuntime::standalone(
            credentials,
            "reader-test".parse().unwrap(),
            "reader-test".parse().unwrap(),
            vec![],
        )
        .unwrap();
        let (runtime, owner) = runtime.start();
        let runtime = Arc::new(runtime);
        let socket = root.path().join("api.sock");
        let evidence = Arc::new(WriteEvidence::default());
        let acceptor = ObserveFirst {
            inner: UnixListener::new(socket.clone()).bind().await,
            first: Some(evidence.clone()),
        };
        let server = client_server(acceptor);
        let handle = server.handle();
        let calls = Arc::new(AtomicUsize::new(0));
        let router = Router::new()
            .push(
                Router::with_path("api/v1/cluster")
                    .hoop(CountRequests(calls.clone()))
                    .get(ClusterSummaryHandler {
                        runtime: runtime.clone(),
                        local: true,
                    }),
            )
            .push(
                Router::with_path("api/v1/cluster/lock").post(MembershipMutationHandler {
                    kind: MembershipMutation::Lock,
                    runtime: runtime.clone(),
                    capacity: Arc::new(tokio::sync::Semaphore::new(16)),
                }),
            );
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(server.try_serve(router));
        let mut reader = tokio::net::UnixStream::connect(&socket).await.unwrap();
        const GET: &[u8] = b"GET /api/v1/cluster HTTP/1.1\r\nHost: localhost\r\n\r\n";
        // Finite valid requests, no partial header/body and no fabricated large
        // response. Unread pipelined summaries fill the real transport buffers.
        let requests = GET.repeat(2048);
        tokio::time::timeout(Duration::from_secs(1), reader.write_all(&requests))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !evidence.pending.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("must observe a real pending transport write");
        assert!(evidence.bytes.load(Ordering::SeqCst) > 0);
        assert!(!evidence.dropped.load(Ordering::SeqCst));
        use orishu::client::{ClientApi, ClusterApi, MembershipApi};
        let client = orishu::client::http_client::HttpClusterClient::new(
            orishu::client::ClusterAddress::UnixSocket(socket),
            orishu::client::http_client::HttClientOptions {
                credentials: Some(orishu::client::Credentials::Token(
                    std::fs::read_to_string(root.path().join("state/operator.token"))
                        .unwrap()
                        .trim()
                        .to_owned(),
                )),
                ..Default::default()
            },
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            let summary = client.cluster().summary().await.unwrap();
            assert_eq!(summary.member_count, 1);
            assert!(
                client
                    .membership()
                    .set_lock(&orishu::model::cluster::LockRequest {
                        schema_version: 1,
                        operation_id: "slow-reader-lock".parse().unwrap(),
                        formation_id: summary.formation_id,
                        locked: true,
                    })
                    .await
                    .unwrap()
                    .locked
            );
            assert!(runtime.summary().unwrap().membership_locked);
        })
        .await
        .expect("unrelated real client and owner remain responsive");
        let stalled_calls = calls.load(Ordering::SeqCst);
        let stalled_bytes = evidence.bytes.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            stalled_calls,
            "pipelined response work must stop under backpressure"
        );
        assert_eq!(evidence.bytes.load(Ordering::SeqCst), stalled_bytes);
        assert!(stalled_calls < 2048, "entire response batch was buffered");
        assert!(stalled_bytes < 1_048_576);
        tokio::time::timeout(Duration::from_secs(7), async {
            while !evidence.dropped.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("stalled client must be reclaimed without reading or shutdown");
        assert!(
            evidence.timed_out.load(Ordering::SeqCst),
            "write timeout must cause reclamation"
        );
        assert_eq!(client.cluster().summary().await.unwrap().member_count, 1);
        // Only after server-side reclamation may the fixture drain/close the
        // client. Require real HTTP responses rather than a pre-dispatch close.
        let mut response = Vec::new();
        let result = reader.take(1_048_577).read_to_end(&mut response).await;
        if let Err(error) = result {
            assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        }
        assert!(response.starts_with(b"HTTP/1.1 200 "));
        assert!(response.len() <= 1_048_576);
        runtime.shutdown().await.unwrap();
        owner.await.unwrap().unwrap();
        handle.stop_graceful(Some(Duration::from_secs(1)));
        tasks.join_next().await.unwrap().unwrap().unwrap();
    })
    .await
    .expect("slow-reader fixture deadline");
}
