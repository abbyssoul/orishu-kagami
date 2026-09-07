use super::*;
use crate::peer::{
    exchange_metrics::{ExchangeOutcome, ExchangeRole},
    tls::{PeerIdentity, SERVER_NAME},
};
use ExchangeOutcome::*;
use ExchangeRole::*;

struct Pair {
    client: quinn::Endpoint,
    server: quinn::Endpoint,
    outgoing: quinn::Connection,
    incoming: quinn::Connection,
}

impl Pair {
    async fn new(small_windows: bool) -> Self {
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let mut server_config = server_identity.server_config().unwrap();
        let mut client_config = client_identity
            .client_config(server_identity.certificate())
            .unwrap();
        if small_windows {
            let mut send_limits = quinn::TransportConfig::default();
            send_limits.send_window(64);
            server_config.transport_config(Arc::new(send_limits));
            let mut receive_limits = quinn::TransportConfig::default();
            receive_limits
                .stream_receive_window(64_u32.into())
                .receive_window(64_u32.into());
            client_config.transport_config(Arc::new(receive_limits));
        }
        let server =
            quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connecting = client
            .connect_with(client_config, server.local_addr().unwrap(), SERVER_NAME)
            .unwrap();
        let (outgoing, incoming) =
            tokio::join!(connecting, async { server.accept().await.unwrap().await });
        Self {
            client,
            server,
            outgoing: outgoing.unwrap(),
            incoming: incoming.unwrap(),
        }
    }

    async fn exchange(&self, client_pool: &ExchangePool, server_pool: &ExchangePool) {
        let (received, served) = tokio::join!(
            client_pool.request(&self.outgoing, &[0xf5], Phase::Handshake),
            async {
                let (send, receive) = self.incoming.accept_bi().await.unwrap();
                server_pool
                    .serve(send, receive, Phase::Handshake, |bytes| async move {
                        assert_eq!(bytes, [0xf5]);
                        Ok(vec![0xf4])
                    })
                    .await
            }
        );
        assert_eq!(received.unwrap(), [0xf4]);
        served.unwrap();
    }
}

impl Drop for Pair {
    fn drop(&mut self) {
        self.client.close(0_u32.into(), b"fixture complete");
        self.server.close(0_u32.into(), b"fixture complete");
    }
}

fn pool() -> (ExchangePool, ExchangeMetrics) {
    let metrics = ExchangeMetrics::enabled();
    (
        ExchangePool::with_metrics(1, metrics.clone()).unwrap(),
        metrics,
    )
}

fn outcomes(metrics: &ExchangeMetrics, role: ExchangeRole, expected: &[(ExchangeOutcome, u64)]) {
    let snapshot = metrics.snapshot().unwrap();
    let role = snapshot.role(role);
    for outcome in ExchangeOutcome::ALL {
        let wanted = expected
            .iter()
            .find(|(key, _)| *key == outcome)
            .map_or(0, |(_, value)| *value);
        assert_eq!(role.outcome(outcome), wanted, "{outcome:?}");
    }
    assert_eq!(
        role.buckets.iter().sum::<u64>(),
        expected.iter().map(|(_, value)| value).sum::<u64>()
    );
}

#[tokio::test]
async fn real_io_counts_success_and_rejected_partial_frames() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let pair = Pair::new(false).await;
        let (client_pool, client_metrics) = pool();
        let (server_pool, server_metrics) = pool();
        pair.exchange(&client_pool, &server_pool).await;
        assert!(
            client_pool
                .request(&pair.outgoing, &[0xff], Phase::Handshake)
                .await
                .is_err()
        );
        outcomes(&client_metrics, Request, &[(Completed, 1), (Failed, 1)]);
        outcomes(&server_metrics, Serve, &[(Completed, 1)]);
        let snapshot = client_metrics.snapshot().unwrap();
        assert_eq!((snapshot.bytes_sent, snapshot.bytes_received), (5, 5));
        // Incomplete prefix, truncated body, trailing byte, and excessive length.
        // Every byte actually consumed is retained despite the failed frame.
        let cases = [
            vec![0, 0],
            vec![0, 0, 0, 10, 0xf5, 0xf4],
            vec![0, 0, 0, 1, 0xf5, 0xff],
            ((handshake::MAX_HANDSHAKE_BYTES + 1) as u32)
                .to_be_bytes()
                .to_vec(),
        ];
        let mut expected_bytes = 5;
        for (index, bytes) in cases.iter().enumerate() {
            let (mut send, receive) = pair.outgoing.open_bi().await.unwrap();
            send.write_all(bytes).await.unwrap();
            send.finish().unwrap();
            let (send, incoming) = pair.incoming.accept_bi().await.unwrap();
            let result = server_pool
                .serve(send, incoming, Phase::Handshake, |_| async {
                    panic!("bad frame must not reach the owner callback");
                })
                .await;
            assert!(result.is_err());
            drop(receive);
            expected_bytes += bytes.len() as u64;
            let snapshot = server_metrics.snapshot().unwrap();
            assert_eq!(snapshot.bytes_received, expected_bytes);
            assert_eq!(snapshot.bytes_sent, 5);
            outcomes(
                &server_metrics,
                Serve,
                &[(Completed, 1), (Failed, index as u64 + 1)],
            );
            assert_eq!(server_pool.pressure(), (0, 1));
        }
        pair.exchange(&client_pool, &server_pool).await;
        outcomes(&server_metrics, Serve, &[(Completed, 2), (Failed, 4)]);
    })
    .await
    .expect("bounded frame accounting and healthy reuse");
}

#[tokio::test]
async fn real_pool_refusal_cancellation_and_reuse_keep_exact_outcomes() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let pair = Pair::new(false).await;
        let (client_pool, client_metrics) = pool();
        let (server_pool, server_metrics) = pool();
        let connection = pair.outgoing.clone();
        let requests = client_pool.clone();
        let request = tokio::spawn(async move {
            requests
                .request(&connection, &[0xf5], Phase::Handshake)
                .await
        });
        let (send, receive) = pair.incoming.accept_bi().await.unwrap();
        let (entered, observed) = tokio::sync::oneshot::channel();
        let serves = server_pool.clone();
        let serving = tokio::spawn(async move {
            serves
                .serve(send, receive, Phase::Handshake, |_| async {
                    entered.send(()).unwrap();
                    std::future::pending().await
                })
                .await
        });
        observed.await.unwrap();
        assert_eq!(client_pool.pressure(), (1, 1));
        assert_eq!(server_pool.pressure(), (1, 1));
        assert!(matches!(
            client_pool
                .request(&pair.outgoing, &[0xf5], Phase::Handshake)
                .await,
            Err(ExchangeError::Overloaded)
        ));
        let (mut send, receive) = pair.outgoing.open_bi().await.unwrap();
        super::super::write_payload(&mut send, &[0xf5])
            .await
            .unwrap();
        let (send, incoming) = pair.incoming.accept_bi().await.unwrap();
        assert!(matches!(
            server_pool
                .serve(send, incoming, Phase::Handshake, |_| async {
                    panic!("capacity refusal must not invoke callback");
                })
                .await,
            Err(ExchangeError::Overloaded)
        ));
        drop(receive);
        // Abort requester first so its result is cancellation, not the response
        // reset emitted when the serving future is subsequently disposed.
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        serving.abort();
        assert!(serving.await.unwrap_err().is_cancelled());
        outcomes(
            &client_metrics,
            Request,
            &[(CapacityRefused, 1), (Cancelled, 1)],
        );
        outcomes(
            &server_metrics,
            Serve,
            &[(CapacityRefused, 1), (Cancelled, 1)],
        );
        let client = client_metrics.snapshot().unwrap();
        let server = server_metrics.snapshot().unwrap();
        assert_eq!((client.bytes_sent, client.bytes_received), (5, 0));
        assert_eq!((server.bytes_sent, server.bytes_received), (0, 5));
        assert_eq!(client_pool.pressure(), (0, 1));
        assert_eq!(server_pool.pressure(), (0, 1));
        pair.exchange(&client_pool, &server_pool).await;
        outcomes(
            &client_metrics,
            Request,
            &[(CapacityRefused, 1), (Cancelled, 1), (Completed, 1)],
        );
        outcomes(
            &server_metrics,
            Serve,
            &[(CapacityRefused, 1), (Cancelled, 1), (Completed, 1)],
        );
    })
    .await
    .expect("bounded pool pressure and cancellation accounting");
}

#[tokio::test]
async fn real_request_and_partial_input_deadlines_are_counted() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let pair = Pair::new(false).await;
        let (client_pool, client_metrics) = pool();
        let (server_pool, server_metrics) = pool();
        let (result, retained) = tokio::join!(
            client_pool.request(&pair.outgoing, &[0xf5], Phase::Handshake),
            async {
                let (send, mut receive) = pair.incoming.accept_bi().await.unwrap();
                assert_eq!(
                    super::super::read_payload(&mut receive).await.unwrap(),
                    [0xf5]
                );
                (send, receive)
            }
        );
        assert!(matches!(
            result,
            Err(ExchangeError::Transfer(TransferError::Timeout))
        ));
        drop(retained);
        outcomes(&client_metrics, Request, &[(TimedOut, 1)]);
        assert_eq!(client_metrics.snapshot().unwrap().bytes_sent, 5);
        let (mut send, receive) = pair.outgoing.open_bi().await.unwrap();
        send.write_all(&[0, 0]).await.unwrap(); // No FIN or remaining prefix.
        let (reply, incoming) = pair.incoming.accept_bi().await.unwrap();
        let result = server_pool
            .serve(reply, incoming, Phase::Handshake, |_| async {
                panic!("incomplete input cannot reach the callback");
            })
            .await;
        assert!(matches!(
            result,
            Err(ExchangeError::Transfer(TransferError::Timeout))
        ));
        drop((send, receive));
        outcomes(&server_metrics, Serve, &[(TimedOut, 1)]);
        assert_eq!(server_metrics.snapshot().unwrap().bytes_received, 2);
        assert_eq!(server_metrics.snapshot().unwrap().bytes_sent, 0);
        pair.exchange(&client_pool, &server_pool).await;
        assert_eq!(client_pool.pressure(), (0, 1));
        assert_eq!(server_pool.pressure(), (0, 1));
    })
    .await
    .expect("two real five-second deadlines and healthy reuse");
}

#[tokio::test]
async fn real_write_backpressure_keeps_partial_sent_bytes_on_timeout() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let pair = Pair::new(true).await;
        let (server_pool, metrics) = pool();
        let (mut send, receive) = pair.outgoing.open_bi().await.unwrap();
        super::super::write_payload(&mut send, &[0xf5])
            .await
            .unwrap();
        let (send, incoming) = pair.incoming.accept_bi().await.unwrap();
        let reply = codec::encode(&"bounded-response".repeat(256)).unwrap();
        let full_bytes = reply.len() as u64 + 4;
        let result = server_pool
            .serve(send, incoming, Phase::Membership, |_| async { Ok(reply) })
            .await;
        assert!(matches!(
            result,
            Err(ExchangeError::Transfer(TransferError::Timeout))
        ));
        let snapshot = metrics.snapshot().unwrap();
        assert_eq!(snapshot.bytes_received, 5);
        assert!(
            snapshot.bytes_sent >= 4 && snapshot.bytes_sent < full_bytes,
            "must reach real partial write backpressure"
        );
        outcomes(&metrics, Serve, &[(TimedOut, 1)]);
        assert_eq!(server_pool.pressure(), (0, 1));
        drop(receive);
    })
    .await
    .expect("bounded response backpressure accounting");
}
