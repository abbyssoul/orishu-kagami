//! Bounded endpoint accept and registered-connection IO. This adapter never
//! mutates membership or constructs trusted peer contexts from packet claims.

use super::{
    FRAME_DEADLINE,
    exchange::{ExchangeError, ExchangePool, Phase},
    ingress::{IngressEvent, IngressMetrics},
    wire::Transport,
};
use crate::driver::{Generation, Handle};
use orishu_membership::SessionId;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::Semaphore,
    task::{JoinHandle, JoinSet},
};

const MAX_CONNECTIONS: usize = 64;
const MAX_TLS_HANDSHAKES: usize = 16;
const MAX_STREAM_TASKS: usize = 16;

#[cfg(all(test, feature = "observability"))]
#[path = "server_metrics_tests.rs"]
mod metrics_tests;

struct EndpointLease(quinn::Endpoint);
impl Drop for EndpointLease {
    fn drop(&mut self) {
        self.0.close(0_u32.into(), b"peer listener stopped");
    }
}

pub(crate) struct ConnectionLease(pub(crate) quinn::Connection);
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        self.0.close(0_u32.into(), b"peer connection stopped");
    }
}

/// Own an already-bound peer endpoint. Stopping/aborting the task closes the
/// endpoint even when callers retain clones. Owner shutdown also stops it.
/// Binding/configuration and any outgoing dialing remain the caller's job.
pub fn spawn(endpoint: quinn::Endpoint, owner: Handle) -> JoinHandle<()> {
    spawn_observed(endpoint, owner, IngressMetrics::default())
}

pub(crate) fn spawn_observed(
    endpoint: quinn::Endpoint,
    owner: Handle,
    metrics: IngressMetrics,
) -> JoinHandle<()> {
    let endpoint = EndpointLease(endpoint);
    tokio::spawn(async move {
        let tls = Arc::new(Semaphore::new(MAX_TLS_HANDSHAKES));
        let serving = metrics.serving(&tls, MAX_CONNECTIONS);
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = owner.closed() => break,
                _ = connections.join_next(), if !connections.is_empty() => {
                    serving.connections(connections.len());
                },
                incoming = endpoint.0.accept() => {
                    let Some(incoming) = incoming else { break; };
                    let Ok(permit) = tls.clone().try_acquire_owned() else {
                        metrics.record(IngressEvent::TlsCapacityRefused);
                        incoming.refuse(); continue;
                    };
                    if connections.len() >= MAX_CONNECTIONS {
                        metrics.record(IngressEvent::ConnectionCapacityRefused);
                        incoming.refuse(); continue;
                    }
                    let Ok(view) = owner.view() else { incoming.refuse(); break; };
                    let owner = owner.clone();
                    let metrics = metrics.clone();
                    connections.spawn(async move {
                        let pending = metrics.tls();
                        let connection = tokio::time::timeout(FRAME_DEADLINE, incoming).await;
                        drop(permit);
                        let connection = match connection {
                            Ok(Ok(connection)) => {
                                pending.finish(IngressEvent::TlsCompleted);
                                connection
                            }
                            Err(_) | Ok(Err(quinn::ConnectionError::TimedOut)) => {
                                pending.finish(IngressEvent::TlsTimedOut);
                                return;
                            }
                            Ok(Err(_)) => {
                                pending.finish(IngressEvent::TlsFailed);
                                return;
                            }
                        };
                        let connection = ConnectionLease(connection);
                        let pool = owner.exchange_pool();
                        let pending = metrics.handshake();
                        let handshake = tokio::time::timeout(FRAME_DEADLINE, async {
                            let (send, receive) = connection.0.accept_bi().await.map_err(|_| ExchangeError::Owner)?;
                            if receive.id().index() != 0 { return Err(ExchangeError::Owner); }
                            let (session_send, session_receive) = tokio::sync::oneshot::channel();
                            pool.serve(send, receive, Phase::Handshake, |bytes| async {
                                let accepted = owner.accept_peer_handshake(view.generation, connection.0.clone(), bytes).map_err(|_| ExchangeError::Owner)?
                                    .await.map_err(|_| ExchangeError::Owner)?.map_err(|_| ExchangeError::Owner)?;
                                let _ = session_send.send(accepted.session);
                                Ok(accepted.bytes)
                            }).await?;
                            session_receive.await.map_err(|_| ExchangeError::Owner)
                        }).await;
                        let session = match handshake {
                            Ok(Ok(session)) => {
                                pending.finish(IngressEvent::HandshakeCompleted);
                                session
                            }
                            Err(_) | Ok(Err(ExchangeError::Transfer(super::TransferError::Timeout))) => {
                                pending.finish(IngressEvent::HandshakeTimedOut);
                                return;
                            }
                            Ok(Err(_)) => {
                                pending.finish(IngressEvent::HandshakeFailed);
                                return;
                            }
                        };
                        serve_registered(&connection.0, &owner, view.generation, session, &pool).await;
                    });
                    serving.connections(connections.len());
                }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), connections.shutdown()).await;
    })
}

/// Receive membership streams/datagrams after the owner established the binding.
/// Future outbound member connections can reuse this loop after their ACK is
/// validated. Each connection has at most 16 stream tasks; all share the owner's
/// global reliable pool, so byte allocation cannot grow with unbounded waiters.
pub(crate) async fn serve_registered(
    connection: &quinn::Connection,
    owner: &Handle,
    generation: Generation,
    session: SessionId,
    pool: &ExchangePool,
) {
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            _ = owner.closed() => break,
            _ = connection.closed() => break,
            _ = streams.join_next(), if !streams.is_empty() => {},
            accepted = connection.accept_bi() => {
                let Ok((mut send, mut receive)) = accepted else { break; };
                if streams.len() >= MAX_STREAM_TASKS {
                    owner.packet_io().stream_capacity_refused();
                    let _ = send.reset(0_u32.into());
                    let _ = receive.stop(0_u32.into());
                    continue;
                }
                let owner = owner.clone();
                let pool = pool.clone();
                streams.spawn(async move {
                    pool.serve(send, receive, Phase::Membership, |bytes| async {
                        let reply = owner.receive_peer_packet(generation, session, Transport::Stream, bytes).map_err(|_| ExchangeError::Owner)?;
                        reply.await.map_err(|_| ExchangeError::Owner)?.map_err(|_| ExchangeError::Owner)?
                            .map(|packet| packet.bytes).ok_or(ExchangeError::Owner)
                    }).await
                });
            }
            packet = connection.read_datagram() => {
                let Ok(packet) = packet else { break; };
                if !owner.packet_io().receive(packet.len()) { continue; }
                let Ok(reply) = owner.receive_peer_packet(generation, session, Transport::Datagram, packet.to_vec()) else { continue; };
                // Datagram overload/loss is permitted; an unresponsive owner is
                // not. Do not accumulate an unbounded queue of reply waiters.
                if tokio::time::timeout(FRAME_DEADLINE, reply).await.is_err() { break; }
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), streams.shutdown()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{
        handshake,
        session::AuthenticatedSession,
        tls::{PeerIdentity, SERVER_NAME},
    };
    use orishu_membership::{Membership, testing};

    pub(super) fn model(identity: &PeerIdentity, name: &str) -> Membership {
        let fixture = testing::standalone(name);
        let mut local = fixture.local().clone();
        local.cert_fingerprint = identity.fingerprint();
        Membership::standalone(
            format!("formation-{name}").parse().unwrap(),
            fixture.cluster_name().clone(),
            local,
            Default::default(),
            Default::default(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn dispatcher_handshake_and_owner_shutdown_close_all_connections() {
        let identity = PeerIdentity::generate().unwrap();
        let applicant = PeerIdentity::generate().unwrap();
        let local = model(&applicant, "applicant");
        let (handle, owner) = crate::driver::spawn_standalone(model(&identity, "server"));
        let target = handle.view().unwrap().summary.formation_id;
        let endpoint = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let address = endpoint.local_addr().unwrap();
        let dispatcher = spawn(endpoint.clone(), handle.clone());
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let exercise = async {
            let connection = client
                .connect_with(
                    applicant.client_config(identity.certificate()).unwrap(),
                    address,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            let bytes = handshake::request(local.local(), target.clone(), true).unwrap();
            let reply = ExchangePool::new(1)
                .unwrap()
                .request(&connection, &bytes, Phase::Handshake)
                .await
                .unwrap();
            let mut session = AuthenticatedSession::from_connection(
                &connection,
                SessionId(1),
                Generation(0),
                local.formation().clone(),
            )
            .unwrap();
            handshake::accept_reply(
                &reply,
                &mut session,
                &local,
                Generation(0),
                target,
                identity.fingerprint(),
            )
            .unwrap();
            assert_eq!(handle.view().unwrap().summary.alive_count, 1);
            // This second TLS connection never sends an application handshake.
            // Shutdown must close it too, despite the retained endpoint clone.
            let pending = client
                .connect_with(
                    applicant.client_config(identity.certificate()).unwrap(),
                    address,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            handle.shutdown().await.unwrap();
            assert_eq!(owner.await.unwrap(), Ok(()));
            dispatcher.await.unwrap();
            connection.closed().await;
            pending.closed().await;
        };
        tokio::time::timeout(Duration::from_secs(4), exercise)
            .await
            .expect("bounded dispatcher handshake and shutdown");
        client.close(0_u32.into(), b"test complete");
    }

    #[tokio::test]
    async fn silent_application_handshake_expires() {
        let identity = PeerIdentity::generate().unwrap();
        let applicant = PeerIdentity::generate().unwrap();
        let (handle, owner) = crate::driver::spawn_standalone(model(&identity, "server"));
        let endpoint = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let address = endpoint.local_addr().unwrap();
        let dispatcher = spawn(endpoint, handle.clone());
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        tokio::time::timeout(FRAME_DEADLINE + Duration::from_secs(2), async {
            let connection = client
                .connect_with(
                    applicant.client_config(identity.certificate()).unwrap(),
                    address,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            assert!(matches!(
                connection.closed().await,
                quinn::ConnectionError::ApplicationClosed(_)
            ));
        })
        .await
        .expect("first application stream has a connection-wide deadline");
        handle.shutdown().await.unwrap();
        assert_eq!(owner.await.unwrap(), Ok(()));
        dispatcher.await.unwrap();
        client.close(0_u32.into(), b"test complete");
    }
}
