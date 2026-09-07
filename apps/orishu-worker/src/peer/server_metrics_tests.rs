//! Real QUIC acceptance for the inbound metric boundary, not domain admission.
use super::*;
use crate::peer::{
    handshake,
    tls::{PeerIdentity, SERVER_NAME},
};
use IngressEvent::*;

struct Fixture {
    identity: PeerIdentity,
    applicant: PeerIdentity,
    client: quinn::Endpoint,
    address: std::net::SocketAddr,
    handle: Handle,
    owner: JoinHandle<Result<(), crate::driver::DriverError>>,
    dispatcher: JoinHandle<()>,
    metrics: IngressMetrics,
}

impl Fixture {
    fn new() -> Self {
        let identity = PeerIdentity::generate().unwrap();
        let applicant = PeerIdentity::generate().unwrap();
        let endpoint = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let address = endpoint.local_addr().unwrap();
        let (handle, owner) =
            crate::driver::spawn_standalone(super::tests::model(&identity, "metrics"));
        let metrics = IngressMetrics::enabled();
        let dispatcher = spawn_observed(endpoint, handle.clone(), metrics.clone());
        Self {
            identity,
            applicant,
            client: quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap(),
            address,
            handle,
            owner,
            dispatcher,
            metrics,
        }
    }

    async fn connect(&self) -> quinn::Connection {
        self.client
            .connect_with(
                self.applicant
                    .client_config(self.identity.certificate())
                    .unwrap(),
                self.address,
                SERVER_NAME,
            )
            .unwrap()
            .await
            .unwrap()
    }

    async fn good(&self) -> quinn::Connection {
        let connection = self.connect().await;
        let local = super::tests::model(&self.applicant, "applicant");
        let request = handshake::request(
            local.local(),
            self.handle.view().unwrap().summary.formation_id,
            true,
        )
        .unwrap();
        ExchangePool::new(1)
            .unwrap()
            .request(&connection, &request, Phase::Handshake)
            .await
            .unwrap();
        connection
    }

    async fn count(&self, event: IngressEvent, expected: u64) {
        tokio::time::timeout(FRAME_DEADLINE + Duration::from_secs(2), async {
            loop {
                let value = self.metrics.snapshot().unwrap().get(event);
                assert!(value <= expected, "{event:?}: {value} > {expected}");
                if value == expected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("exact adapter event must become visible");
    }

    async fn pressure(&self, tls: usize, connections: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let value = self.metrics.snapshot().unwrap().pressure();
                if value == crate::peer::ingress::IngressPressure::default() {
                    tokio::task::yield_now().await;
                    continue;
                }
                assert_eq!(value.tls_slots_capacity, MAX_TLS_HANDSHAKES);
                assert_eq!(value.connection_slots_capacity, MAX_CONNECTIONS);
                assert!(value.tls_slots_in_use <= MAX_TLS_HANDSHAKES);
                assert!(value.connection_slots_in_use <= MAX_CONNECTIONS);
                if (value.tls_slots_in_use, value.connection_slots_in_use) == (tls, connections) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("actual ingress budget occupancy");
    }

    async fn control_while_occupied(&self) {
        let original = self.handle.view().unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            for locked in [true, false] {
                let result = self
                    .handle
                    .set_membership_lock(
                        original.generation,
                        original.summary.formation_id.clone(),
                        locked,
                    )
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(result.membership_locked, locked);
                assert_eq!(result.formation_id, original.summary.formation_id);
            }
        })
        .await
        .expect("real owner control must progress before ingress expiry");
    }

    /// Forward real client datagrams, but withhold all server replies. Seeing
    /// a server response proves its TLS task started; no timer mocking is used.
    async fn held_tls(&self) -> (quinn::Connecting, JoinHandle<()>) {
        let relay = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = relay.local_addr().unwrap();
        let target = self.address;
        let (seen, received) = tokio::sync::oneshot::channel();
        let relay_task = tokio::spawn(async move {
            let mut seen = Some(seen);
            let mut bytes = [0; 2048];
            loop {
                let (length, source) = relay.recv_from(&mut bytes).await.unwrap();
                if source == target {
                    if let Some(seen) = seen.take() {
                        let _ = seen.send(());
                    }
                } else {
                    relay.send_to(&bytes[..length], target).await.unwrap();
                }
            }
        });
        let connecting = self
            .client
            .connect_with(
                self.applicant
                    .client_config(self.identity.certificate())
                    .unwrap(),
                address,
                SERVER_NAME,
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), received)
            .await
            .unwrap()
            .unwrap();
        (connecting, relay_task)
    }

    async fn stop(self) -> IngressMetrics {
        self.handle.shutdown().await.unwrap();
        assert_eq!(self.owner.await.unwrap(), Ok(()));
        self.dispatcher.await.unwrap();
        self.client.close(0_u32.into(), b"fixture complete");
        self.metrics
    }
}

fn assert_counts(metrics: &IngressMetrics, expected: &[(IngressEvent, u64)]) {
    let snapshot = metrics.snapshot().unwrap();
    assert_eq!(
        snapshot.pressure(),
        crate::peer::ingress::IngressPressure::default()
    );
    for event in IngressEvent::ALL {
        let wanted = expected
            .iter()
            .find(|(key, _)| *key == event)
            .map_or(0, |(_, value)| *value);
        assert_eq!(snapshot.get(event), wanted, "{event:?}");
    }
}

#[tokio::test]
async fn adapter_abort_with_pending_tls_and_handshake_withdraws_pressure() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut f = Fixture::new();
        let pending = f.connect().await;
        let (held, relay) = f.held_tls().await;
        f.pressure(1, 2).await;
        f.dispatcher.abort();
        assert!((&mut f.dispatcher).await.unwrap_err().is_cancelled());
        assert_eq!(
            f.metrics.snapshot().unwrap().pressure(),
            crate::peer::ingress::IngressPressure::default()
        );
        pending.closed().await;
        f.count(TlsCancelled, 1).await;
        f.count(HandshakeCancelled, 1).await;
        f.handle.shutdown().await.unwrap();
        assert_eq!(f.owner.await.unwrap(), Ok(()));
        drop(held);
        relay.abort();
        assert!(relay.await.unwrap_err().is_cancelled());
        f.client.close(0_u32.into(), b"fixture complete");
    })
    .await
    .expect("adapter abort must withdraw pressure and close real connections");
}

#[tokio::test]
async fn errors_success_and_shutdown_cancellation_are_distinct() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let f = Fixture::new();
        f.pressure(0, 0).await;
        let original = f.handle.view().unwrap().summary.formation_id;
        // A real client without a certificate fails before the application stage.
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                f.identity.certificate().to_vec(),
            ))
            .unwrap();
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        tls.alpn_protocols = vec![crate::peer::tls::ALPN.to_vec()];
        let config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        ));
        if let Ok(connection) = f
            .client
            .connect_with(config, f.address, SERVER_NAME)
            .unwrap()
            .await
        {
            connection.closed().await;
        }
        f.count(TlsFailed, 1).await;
        let malformed = f.connect().await;
        // Valid bounded CBOR, wrong application schema. The failure is not a
        // membership packet decode refusal or core admission rejection.
        assert!(
            ExchangePool::new(1)
                .unwrap()
                .request(&malformed, &[0xf5], Phase::Handshake)
                .await
                .is_err()
        );
        f.count(HandshakeFailed, 1).await;
        let good = f.good().await;
        f.count(HandshakeCompleted, 1).await;
        let pending = f.connect().await;
        f.count(TlsCompleted, 3).await;
        let (held, relay) = f.held_tls().await;
        f.pressure(1, 3).await;
        let view = f.handle.view().unwrap();
        assert_eq!(view.summary.formation_id, original);
        assert_eq!(view.summary.member_count, 1);
        assert_eq!(view.peer_decode_rejections, 0);
        assert_eq!(view.admissions.accepted, 0);
        assert_eq!(view.admissions.rejected, 0);
        let metrics = f.stop().await;
        good.closed().await;
        pending.closed().await;
        drop(held);
        relay.abort();
        assert!(relay.await.unwrap_err().is_cancelled());
        assert_counts(
            &metrics,
            &[
                (TlsFailed, 1),
                (TlsCompleted, 3),
                (TlsCancelled, 1),
                (HandshakeFailed, 1),
                (HandshakeCompleted, 1),
                (HandshakeCancelled, 1),
            ],
        );
    })
    .await
    .expect("bounded real handshake outcome matrix");
}

#[tokio::test]
async fn tls_capacity_refusal_and_deadline_release_are_observable() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let f = Fixture::new();
        let mut held = Vec::with_capacity(MAX_TLS_HANDSHAKES);
        for _ in 0..MAX_TLS_HANDSHAKES {
            held.push(f.held_tls().await);
        }
        f.pressure(16, 16).await;
        f.control_while_occupied().await;
        let extra = f
            .client
            .connect_with(
                f.applicant.client_config(f.identity.certificate()).unwrap(),
                f.address,
                SERVER_NAME,
            )
            .unwrap()
            .await;
        assert!(
            extra.is_err(),
            "TLS capacity must refuse an additional connection"
        );
        f.count(TlsCapacityRefused, 1).await;
        f.count(TlsTimedOut, MAX_TLS_HANDSHAKES as u64).await;
        f.pressure(0, 0).await;
        for (connecting, relay) in held {
            drop(connecting);
            relay.abort();
            assert!(relay.await.unwrap_err().is_cancelled());
        }
        let good = f.good().await;
        f.count(HandshakeCompleted, 1).await;
        f.pressure(0, 1).await;
        let silent = f.connect().await;
        silent.closed().await;
        f.count(HandshakeTimedOut, 1).await;
        let metrics = f.stop().await;
        good.closed().await;
        assert_counts(
            &metrics,
            &[
                (TlsCapacityRefused, 1),
                (TlsTimedOut, 16),
                (TlsCompleted, 2),
                (HandshakeCompleted, 1),
                (HandshakeTimedOut, 1),
            ],
        );
    })
    .await
    .expect("TLS capacity, expiry, healthy reuse and application expiry");
}

#[tokio::test]
async fn connection_capacity_refusal_and_reuse_are_observable() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let f = Fixture::new();
        let mut held = Vec::with_capacity(MAX_CONNECTIONS);
        // Complete TLS but retain all initial application handshakes. TLS slots
        // are free; this reaches the separate 64-connection task limit.
        for _ in 0..MAX_CONNECTIONS {
            held.push(f.connect().await);
        }
        f.count(TlsCompleted, 64).await;
        f.pressure(0, 64).await;
        f.control_while_occupied().await;
        let extra = f
            .client
            .connect_with(
                f.applicant.client_config(f.identity.certificate()).unwrap(),
                f.address,
                SERVER_NAME,
            )
            .unwrap()
            .await;
        assert!(extra.is_err());
        f.count(ConnectionCapacityRefused, 1).await;
        held.pop().unwrap().close(0_u32.into(), b"release one slot");
        f.count(HandshakeFailed, 1).await;
        f.pressure(0, 63).await;
        let good = f.good().await;
        f.count(HandshakeCompleted, 1).await;
        f.pressure(0, 64).await;
        let metrics = f.stop().await;
        good.closed().await;
        for connection in held {
            connection.closed().await;
        }
        assert_counts(
            &metrics,
            &[
                (ConnectionCapacityRefused, 1),
                (TlsCompleted, 65),
                (HandshakeFailed, 1),
                (HandshakeCompleted, 1),
                (HandshakeCancelled, 63),
            ],
        );
    })
    .await
    .expect("connection capacity gate and reclaimed slot");
}
