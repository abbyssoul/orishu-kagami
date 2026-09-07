//! Bounded QUIC request/reply IO. No future here owns or locks membership;
//! a server callback can submit to the owner and await its correlated reply.

use super::exchange_metrics::{ExchangeMetrics, ExchangeRole};
use super::{FRAME_DEADLINE, TransferError, codec, handshake, wire};
use std::{future::Future, sync::Arc};
use tokio::{sync::Semaphore, time::Instant};

#[cfg(all(test, feature = "observability"))]
#[path = "exchange_metrics_tests.rs"]
mod metrics_tests;

/// A narrower handshake limit applies before normal membership exchange begins.
#[derive(Debug, Clone, Copy)]
pub enum Phase {
    Handshake,
    Membership,
    /// Admission-state replies have a smaller allocation ceiling than gossip.
    AdmissionState,
}

impl Phase {
    fn maximum(self) -> usize {
        match self {
            Self::Handshake => handshake::MAX_HANDSHAKE_BYTES,
            Self::Membership => codec::MAX_FRAME_BYTES,
            Self::AdmissionState => 131_072,
        }
    }
}

/// Bounded transport failure without bytes, credentials or peer-provided text.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// The current owner rejected the input or could not produce its reply.
    #[error("peer owner could not complete the exchange")]
    Owner,
    /// No IO slot is available; callers must not build an unbounded waiter queue.
    #[error("peer exchange capacity exhausted")]
    Overloaded,
    /// Local configuration is outside the supported bounds.
    #[error("peer exchange capacity must be in 1..=64")]
    Configuration,
    /// Stream, codec, or total operation deadline failed.
    #[error(transparent)]
    Transfer(#[from] TransferError),
    /// A datagram was unsupported, oversized, or submitted for a stream-only body.
    #[error("peer datagram could not be submitted")]
    Datagram,
}

/// Shared worker-wide cap on inbound and outbound reliable operations. Permits
/// include owner-reply waiting, so pending decoded frames remain bounded too.
#[derive(Clone)]
pub struct ExchangePool {
    slots: Arc<Semaphore>,
    #[cfg(feature = "observability")]
    capacity: usize,
    metrics: ExchangeMetrics,
}

impl ExchangePool {
    #[cfg(test)]
    pub(crate) fn available_exchanges(&self) -> usize {
        self.slots.available_permits()
    }

    /// Build one pool for the worker, not one per peer. No hidden waiter queue is
    /// created: attempts above this capacity fail immediately when polled.
    pub fn new(capacity: usize) -> Result<Self, ExchangeError> {
        Self::with_metrics(capacity, ExchangeMetrics::default())
    }

    /// Attach startup-selected accounting without changing the exchange budget.
    pub(crate) fn with_metrics(
        capacity: usize,
        metrics: ExchangeMetrics,
    ) -> Result<Self, ExchangeError> {
        if !(1..=64).contains(&capacity) {
            return Err(ExchangeError::Configuration);
        }
        Ok(Self {
            slots: Arc::new(Semaphore::new(capacity)),
            #[cfg(feature = "observability")]
            capacity,
            metrics,
        })
    }

    /// Fixed capacity and current occupancy, without reserving a slot.
    #[cfg(feature = "observability")]
    pub(crate) fn pressure(&self) -> (usize, usize) {
        (
            self.capacity - self.slots.available_permits(),
            self.capacity,
        )
    }

    #[cfg(feature = "observability")]
    pub(crate) fn counters(&self) -> Option<super::exchange_metrics::ExchangeSnapshot> {
        self.metrics.snapshot()
    }

    /// Request/reply with one absolute deadline covering stream credit, write,
    /// response bytes and FIN. Successful transport is not domain acceptance.
    pub async fn request(
        &self,
        connection: &quinn::Connection,
        payload: &[u8],
        phase: Phase,
    ) -> Result<Vec<u8>, ExchangeError> {
        let mut observation = self.metrics.begin(ExchangeRole::Request);
        let result = self
            .request_observed(connection, payload, phase, &mut observation.transfer)
            .await;
        observation.finish(&result);
        result
    }

    async fn request_observed(
        &self,
        connection: &quinn::Connection,
        payload: &[u8],
        phase: Phase,
        meter: &mut super::exchange_metrics::TransferCount,
    ) -> Result<Vec<u8>, ExchangeError> {
        validate(payload, phase)?;
        let _permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ExchangeError::Overloaded)?;
        let deadline = Instant::now() + FRAME_DEADLINE;
        tokio::time::timeout_at(deadline, async {
            let (send, receive) = connection
                .open_bi()
                .await
                .map_err(|_| TransferError::Transport)?;
            let mut streams = Streams {
                send,
                receive,
                completed: false,
            };
            super::write_payload_metered(&mut streams.send, payload, meter).await?;
            let response =
                super::read_payload_metered(&mut streams.receive, phase.maximum(), meter).await?;
            streams.completed = true;
            Ok(response)
        })
        .await
        .map_err(|_| TransferError::Timeout)?
    }

    /// Serve an accepted bidi stream, holding a shared slot through the owner
    /// callback and reply. Timeout or cancellation resets both stream directions;
    /// a stalled callback cannot retain the permit forever. The connection loop
    /// must bound sessions and enforce the initial connection-handshake deadline.
    pub async fn serve<F, R>(
        &self,
        send: quinn::SendStream,
        receive: quinn::RecvStream,
        phase: Phase,
        reply: F,
    ) -> Result<(), ExchangeError>
    where
        F: FnOnce(Vec<u8>) -> R,
        R: Future<Output = Result<Vec<u8>, ExchangeError>>,
    {
        let mut observation = self.metrics.begin(ExchangeRole::Serve);
        let result = self
            .serve_observed(send, receive, phase, reply, &mut observation.transfer)
            .await;
        observation.finish(&result);
        result
    }

    async fn serve_observed<F, R>(
        &self,
        send: quinn::SendStream,
        receive: quinn::RecvStream,
        phase: Phase,
        reply: F,
        meter: &mut super::exchange_metrics::TransferCount,
    ) -> Result<(), ExchangeError>
    where
        F: FnOnce(Vec<u8>) -> R,
        R: Future<Output = Result<Vec<u8>, ExchangeError>>,
    {
        let mut streams = Streams {
            send,
            receive,
            completed: false,
        };
        let _permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ExchangeError::Overloaded)?;
        tokio::time::timeout(FRAME_DEADLINE, async {
            let request =
                super::read_payload_metered(&mut streams.receive, phase.maximum(), meter).await?;
            let response = reply(request).await?;
            validate(&response, phase)?;
            super::write_payload_metered(&mut streams.send, &response, meter).await?;
            streams.completed = true;
            Ok(())
        })
        .await
        .map_err(|_| TransferError::Timeout)?
    }

    /// Submit only a datagram-class encoded message. No reliable-stream fallback
    /// is permitted. Success means queued locally, never acknowledged/delivered.
    pub fn datagram(
        connection: &quinn::Connection,
        packet: wire::Encoded,
    ) -> Result<(), ExchangeError> {
        super::traffic::Traffic::default().submit(connection, packet)
    }
}

fn validate(payload: &[u8], phase: Phase) -> Result<(), ExchangeError> {
    if payload.len() > phase.maximum() {
        return Err(TransferError::Codec(codec::CodecError::TooLarge).into());
    }
    codec::validate(payload).map_err(TransferError::from)?;
    Ok(())
}

struct Streams {
    send: quinn::SendStream,
    receive: quinn::RecvStream,
    completed: bool,
}

impl Drop for Streams {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.send.reset(1_u8.into());
            let _ = self.receive.stop(1_u8.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tls::{PeerIdentity, SERVER_NAME};
    use super::*;

    #[tokio::test]
    async fn real_request_reply_is_bounded_and_cancellation_releases_capacity() {
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connect = client
            .connect_with(
                client_identity
                    .client_config(server_identity.certificate())
                    .unwrap(),
                server.local_addr().unwrap(),
                SERVER_NAME,
            )
            .unwrap();
        let (outgoing, incoming) =
            tokio::join!(connect, async { server.accept().await.unwrap().await });
        let outgoing = outgoing.unwrap();
        let incoming = incoming.unwrap();
        let client_pool = ExchangePool::new(1).unwrap();
        let server_pool = ExchangePool::new(1).unwrap();
        let packet = codec::encode(&true).unwrap();
        let (received, served) = tokio::join!(
            client_pool.request(&outgoing, &packet, Phase::Handshake),
            async {
                let (send, receive) = incoming.accept_bi().await.unwrap();
                server_pool
                    .serve(send, receive, Phase::Handshake, |bytes| async move {
                        assert!(codec::decode::<bool>(&bytes).unwrap());
                        Ok(codec::encode(&false).unwrap())
                    })
                    .await
            }
        );
        served.unwrap();
        assert!(!codec::decode::<bool>(&received.unwrap()).unwrap());

        // Hold one outgoing request at the remote application, then prove the
        // pool refuses more work instead of accumulating semaphore waiters.
        let request_pool = client_pool.clone();
        let request_connection = outgoing.clone();
        let pending = tokio::spawn(async move {
            request_pool
                .request(&request_connection, &packet, Phase::Membership)
                .await
        });
        let (send, receive) = incoming.accept_bi().await.unwrap();
        assert!(matches!(
            client_pool
                .request(&outgoing, &codec::encode(&true).unwrap(), Phase::Membership)
                .await,
            Err(ExchangeError::Overloaded)
        ));
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert_eq!(client_pool.slots.available_permits(), 1);
        drop((send, receive));
        client.close(0_u8.into(), b"done");
        server.close(0_u8.into(), b"done");
    }
}
