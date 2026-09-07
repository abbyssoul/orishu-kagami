//! Bounded bootstrap and admitted-member IO. No admission tokens enter here.
use super::{
    FRAME_DEADLINE,
    dial_metrics::{DialMetrics, Outcome, Stage},
    exchange::{ExchangePool, Phase},
    handshake,
    tls::{PeerIdentity, SERVER_NAME},
};
use orishu_membership::{CertFingerprint, FormationId, LocalIdentity, NodeId};
use std::{net::SocketAddr, time::Duration};

/// Secret-free target binding copied from operator-authenticated join material.
#[derive(Debug, Clone)]
pub struct IntroducerTarget {
    /// Required immutable target formation.
    pub formation: FormationId,
    /// Required assigned introducer identity, not merely its label.
    pub node: NodeId,
    /// Certificate pin verified during TLS before sending application bytes.
    pub fingerprint: CertFingerprint,
    endpoints: Vec<SocketAddr>,
}

impl TryFrom<&orishu::model::cluster::JoinMaterial> for IntroducerTarget {
    type Error = DialError;
    fn try_from(material: &orishu::model::cluster::JoinMaterial) -> Result<Self, Self::Error> {
        material.validate().map_err(|_| DialError::Invalid)?;
        Ok(Self {
            formation: material.formation_id.clone(),
            node: material.introducer_node_id.clone(),
            fingerprint: material.introducer_fingerprint,
            endpoints: material
                .peer_endpoints
                .iter()
                .map(|s| s.parse().map_err(|_| DialError::Invalid))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Secret-free routing snapshot of an already admitted member. This type cannot
/// be constructed from an operator join token. The owner must revalidate the
/// current membership and generation before registering its handshake result.
#[derive(Debug, Clone)]
pub struct MemberTarget(IntroducerTarget);

impl MemberTarget {
    /// Copy at most eight usable literal endpoints from the current member
    /// record. Unknown/self targets and unsupported endpoint claims fail closed.
    pub fn from_model(
        model: &orishu_membership::Membership,
        node: &NodeId,
    ) -> Result<Self, DialError> {
        let member = model.members().get(node).ok_or(DialError::Invalid)?;
        if node == model.local_id()
            || !matches!(
                member.liveness,
                orishu_membership::Liveness::Alive | orishu_membership::Liveness::Suspected
            )
            || member.endpoints.peers.is_empty()
            || member.endpoints.peers.len() > 8
        {
            return Err(DialError::Invalid);
        }
        let endpoints = member
            .endpoints
            .peers
            .iter()
            .map(|value| {
                if value.0.len() > 128 {
                    return Err(DialError::Invalid);
                }
                let address: SocketAddr = value.0.parse().map_err(|_| DialError::Invalid)?;
                if address.port() == 0
                    || address.ip().is_unspecified()
                    || address.ip().is_multicast()
                {
                    return Err(DialError::Invalid);
                }
                Ok(address)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(IntroducerTarget {
            formation: model.formation().clone(),
            node: node.clone(),
            fingerprint: member.cert_fingerprint,
            endpoints,
        }))
    }
}

/// Redacted transport outcomes; no remote diagnostics or operator secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DialError {
    /// Invalid material/local identity or TLS configuration.
    #[error("invalid peer dial configuration")]
    Invalid,
    /// No queue is created when the worker's dial budget is occupied.
    #[error("peer dial capacity exhausted")]
    Overloaded,
    /// The total attempt, including candidate fallback, exceeded its budget.
    #[error("peer dial deadline exceeded")]
    Timeout,
    /// No candidate completed pinned TLS and the initial application exchange.
    #[error("peer handshake unavailable")]
    Unavailable,
}

/// One process-wide budget for outbound handshake attempts. The owner must share
/// this instance, not construct one per request. There are no permit waiters.
pub struct Dialer(tokio::sync::Semaphore, DialMetrics);
impl Default for Dialer {
    fn default() -> Self {
        Self::with_metrics(DialMetrics::default())
    }
}

/// TLS-authenticated connection and raw ACK awaiting current-owner validation.
/// Dropping/cancelling before transfer closes the connection, including clones.
pub struct PendingHandshake {
    connection: Option<quinn::Connection>,
    reply: Vec<u8>,
}
impl Drop for PendingHandshake {
    fn drop(&mut self) {
        if let Some(connection) = &self.connection {
            connection.close(0_u32.into(), b"outbound handshake abandoned");
        }
    }
}
impl PendingHandshake {
    #[cfg(test)]
    pub(crate) fn observe_connection(&self) -> quinn::Connection {
        self.connection.as_ref().expect("owned connection").clone()
    }

    /// Transfer to the owner's guarded registration path. TLS success alone is
    /// not application binding or admission; validate ACK and lifecycle there.
    pub fn into_parts(mut self) -> (quinn::Connection, Vec<u8>) {
        (
            self.connection.take().expect("owned connection"),
            std::mem::take(&mut self.reply),
        )
    }
}

impl Dialer {
    pub(crate) fn with_metrics(metrics: DialMetrics) -> Self {
        Self(tokio::sync::Semaphore::new(4), metrics)
    }

    /// Optional aggregate readings, without enumerating targets or owner work.
    #[cfg(feature = "observability")]
    pub fn counters(&self) -> Option<super::dial_metrics::Snapshot> {
        self.1.snapshot()
    }

    /// Occupied and configured outbound-attempt slots, sampled independently.
    #[cfg(feature = "observability")]
    pub fn pressure(&self) -> (usize, usize) {
        (4 - self.0.available_permits(), 4)
    }

    #[cfg(test)]
    pub(crate) fn available_attempts(&self) -> usize {
        self.0.available_permits()
    }

    /// At most eight literal candidates, one at a time, under a 15-second total
    /// budget. Each TLS and frame phase has a five-second deadline. No DNS,
    /// redirects, JoinReq, or token transmission occurs here. The endpoint and
    /// exchange pool belong to the worker; no private per-attempt listener.
    pub async fn handshake(
        &self,
        endpoint: &quinn::Endpoint,
        identity: &PeerIdentity,
        local: &LocalIdentity,
        target: &IntroducerTarget,
        pool: &ExchangePool,
    ) -> Result<PendingHandshake, DialError> {
        let request = handshake::request(local, target.formation.clone(), true)
            .map_err(|_| DialError::Invalid)?;
        self.exchange_handshake(endpoint, identity, &request, target, pool)
            .await
    }

    /// Reconnect using admitted identity, never an applicant handshake or token.
    /// Shares the bootstrap dial budget and deadlines; successful IO still needs
    /// generation-fenced member ACK validation through the membership owner.
    pub async fn member_handshake(
        &self,
        endpoint: &quinn::Endpoint,
        identity: &PeerIdentity,
        local: &LocalIdentity,
        target: &MemberTarget,
        pool: &ExchangePool,
    ) -> Result<PendingHandshake, DialError> {
        let request = handshake::request(local, target.0.formation.clone(), false)
            .map_err(|_| DialError::Invalid)?;
        self.exchange_handshake(endpoint, identity, &request, &target.0, pool)
            .await
    }

    async fn exchange_handshake(
        &self,
        endpoint: &quinn::Endpoint,
        identity: &PeerIdentity,
        request: &[u8],
        target: &IntroducerTarget,
        pool: &ExchangePool,
    ) -> Result<PendingHandshake, DialError> {
        let _permit = self.0.try_acquire().map_err(|_| {
            self.1.capacity_refused();
            DialError::Overloaded
        })?;
        let attempt = self.1.begin(Stage::Attempt);
        let config = match identity.member_client_config(target.fingerprint) {
            Ok(config) => config,
            Err(_) => {
                attempt.finish(Outcome::Failed);
                return Err(DialError::Invalid);
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(15), async {
            for address in &target.endpoints {
                let tls = self.1.begin(Stage::Tls);
                let Ok(connecting) = endpoint.connect_with(config.clone(), *address, SERVER_NAME)
                else {
                    tls.finish(Outcome::Failed);
                    continue;
                };
                let connection = match tokio::time::timeout(FRAME_DEADLINE, connecting).await {
                    Ok(Ok(connection)) => {
                        tls.finish(Outcome::Completed);
                        connection
                    }
                    Err(_) | Ok(Err(quinn::ConnectionError::TimedOut)) => {
                        tls.finish(Outcome::TimedOut);
                        continue;
                    }
                    Ok(Err(_)) => {
                        tls.finish(Outcome::Failed);
                        continue;
                    }
                };
                let mut pending = PendingHandshake {
                    connection: Some(connection),
                    reply: Vec::new(),
                };
                if let Ok(reply) = pool
                    .request(
                        pending.connection.as_ref().unwrap(),
                        request,
                        Phase::Handshake,
                    )
                    .await
                {
                    pending.reply = reply;
                    return Ok(pending);
                }
            }
            Err(DialError::Unavailable)
        })
        .await
        .unwrap_or(Err(DialError::Timeout));
        attempt.finish(match &result {
            Ok(_) => Outcome::Completed,
            Err(DialError::Timeout) => Outcome::TimedOut,
            Err(_) => Outcome::Failed,
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        driver::{self, Generation},
        peer::{server, session::AuthenticatedSession},
    };
    use orishu_membership::{Membership, SessionId, testing};

    fn observed_dialer() -> Dialer {
        #[cfg(feature = "observability")]
        {
            Dialer::with_metrics(DialMetrics::enabled())
        }
        #[cfg(not(feature = "observability"))]
        {
            Dialer::default()
        }
    }

    #[cfg(feature = "observability")]
    fn assert_counts(dialer: &Dialer, attempts: [u64; 4], tls: [u64; 4], refusals: u64) {
        let snapshot = dialer.counters().unwrap();
        assert_eq!(snapshot.capacity_refused, refusals);
        for (stage, expected) in [(Stage::Attempt, attempts), (Stage::Tls, tls)] {
            let observed = snapshot.stage(stage);
            for (outcome, expected) in Outcome::ALL.into_iter().zip(expected) {
                assert_eq!(observed.outcome(outcome), expected, "{stage:?}/{outcome:?}");
            }
            assert_eq!(
                observed.buckets.iter().sum::<u64>(),
                expected.iter().sum::<u64>()
            );
        }
    }

    #[cfg(feature = "observability")]
    #[tokio::test]
    async fn outbound_metrics_distinguish_candidate_exhaustion_total_timeout_and_local_failure() {
        tokio::time::timeout(Duration::from_secs(25), async {
            let identity = PeerIdentity::generate().unwrap();
            let local = model(&identity, "outbound-timeouts");
            let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let mut target = IntroducerTarget {
                formation: local.formation().clone(),
                node: local.local_id().clone(),
                fingerprint: identity.fingerprint(),
                endpoints: vec![sink.local_addr().unwrap()],
            };
            let pool = ExchangePool::new(4).unwrap();
            let dialer = observed_dialer();
            assert!(matches!(
                dialer
                    .handshake(&client, &identity, local.local(), &target, &pool)
                    .await,
                Err(DialError::Unavailable)
            ));
            assert_counts(&dialer, [0, 1, 0, 0], [0, 0, 1, 0], 0);
            let mut packet = [0; 2048];
            assert!(sink.try_recv_from(&mut packet).unwrap().0 > 0);

            target.endpoints = vec![sink.local_addr().unwrap(); 4];
            assert!(matches!(
                dialer
                    .handshake(&client, &identity, local.local(), &target, &pool)
                    .await,
                Err(DialError::Timeout)
            ));
            let snapshot = dialer.counters().unwrap();
            let attempts = snapshot.stage(Stage::Attempt);
            assert_eq!(attempts.outcome(Outcome::Failed), 1);
            assert_eq!(attempts.outcome(Outcome::TimedOut), 1);
            assert_eq!(attempts.outcome(Outcome::Completed), 0);
            assert_eq!(attempts.outcome(Outcome::Cancelled), 0);
            assert_eq!(attempts.buckets.iter().sum::<u64>(), 2);
            assert!(attempts.duration_micros >= 19_000_000);
            let tls = snapshot.stage(Stage::Tls);
            assert_eq!(tls.outcome(Outcome::Completed), 0);
            assert_eq!(tls.outcome(Outcome::Failed), 0);
            // At the outer deadline the last inner timeout may already have
            // fired; otherwise disposal is cancellation, not a second timeout.
            assert!(tls.outcome(Outcome::TimedOut) >= 3);
            assert!(tls.outcome(Outcome::Cancelled) <= 1);
            assert_eq!(
                tls.buckets.iter().sum::<u64>(),
                tls.outcome(Outcome::TimedOut) + tls.outcome(Outcome::Cancelled)
            );
            assert_eq!(dialer.pressure(), (0, 4));

            client.close(0_u32.into(), b"local failure control");
            target.endpoints.truncate(1);
            assert!(matches!(
                dialer
                    .handshake(&client, &identity, local.local(), &target, &pool)
                    .await,
                Err(DialError::Unavailable)
            ));
            let snapshot = dialer.counters().unwrap();
            assert_eq!(snapshot.stage(Stage::Attempt).outcome(Outcome::Failed), 2);
            assert_eq!(snapshot.stage(Stage::Tls).outcome(Outcome::Failed), 1);
            assert_eq!(dialer.pressure(), (0, 4));
        })
        .await
        .expect("real candidate and total dial deadlines stay bounded");
    }

    #[cfg(feature = "observability")]
    #[tokio::test]
    async fn outbound_metrics_cancel_after_tls_and_reject_application_exchange() {
        tokio::time::timeout(Duration::from_secs(8), async {
            for reject in [false, true] {
                let identity = PeerIdentity::generate().unwrap();
                let server_identity = PeerIdentity::generate().unwrap();
                let local = model(&identity, "outbound-application");
                let server = quinn::Endpoint::server(
                    server_identity.server_config().unwrap(),
                    "127.0.0.1:0".parse().unwrap(),
                )
                .unwrap();
                let target = IntroducerTarget {
                    formation: local.formation().clone(),
                    node: local.local_id().clone(),
                    fingerprint: server_identity.fingerprint(),
                    endpoints: vec![server.local_addr().unwrap()],
                };
                let (read, read_complete) = tokio::sync::oneshot::channel();
                let server_task = tokio::spawn(async move {
                    let connection = server.accept().await.unwrap().await.unwrap();
                    let (mut send, mut recv) = connection.accept_bi().await.unwrap();
                    assert!(
                        !super::super::read_payload(&mut recv)
                            .await
                            .unwrap()
                            .is_empty()
                    );
                    read.send(()).unwrap();
                    if reject {
                        send.reset(0_u32.into()).unwrap();
                    }
                    connection.closed().await;
                    drop((send, recv, server));
                });
                let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
                let pool = ExchangePool::new(4).unwrap();
                let dialer = observed_dialer();
                {
                    let dialing =
                        dialer.handshake(&client, &identity, local.local(), &target, &pool);
                    tokio::pin!(dialing);
                    if reject {
                        assert!(matches!(dialing.await, Err(DialError::Unavailable)));
                    } else {
                        tokio::select! {
                            _ = &mut dialing => panic!("server withheld reply"),
                            result = read_complete => result.unwrap(),
                        }
                        assert_eq!(dialer.pressure(), (1, 4));
                        assert_counts(&dialer, [0; 4], [1, 0, 0, 0], 0);
                    }
                }
                assert_counts(
                    &dialer,
                    if reject { [0, 1, 0, 0] } else { [0, 0, 0, 1] },
                    [1, 0, 0, 0],
                    0,
                );
                assert_eq!(dialer.pressure(), (0, 4));
                server_task.await.unwrap();
                client.close(0_u32.into(), b"fixture complete");
            }
        })
        .await
        .expect("application failure and cancellation close owned connections");
    }

    fn model(identity: &PeerIdentity, name: &str) -> Membership {
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
    async fn lost_join_ack_keeps_remote_insertion_and_reports_unresolved_locally() {
        use crate::peer::{exchange::ExchangeError, wire::Transport};
        use orishu_membership::{DurationMillis, Limits, LimitsSpec};
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let model_server = model(&server_identity, "server");
        let source = model(&client_identity, "client");
        let source = Membership::standalone(
            source.formation().clone(),
            source.cluster_name().clone(),
            source.local().clone(),
            Default::default(),
            Limits::try_from(LimitsSpec {
                max_join_attempts: 1,
                join_backoff: DurationMillis(500),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        let endpoint = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let target = IntroducerTarget {
            formation: model_server.formation().clone(),
            node: model_server.local_id().clone(),
            fingerprint: server_identity.fingerprint(),
            endpoints: vec![endpoint.local_addr().unwrap()],
        };
        let token = || crate::credentials::SecretToken::parse("a".repeat(64)).unwrap();
        let (server_owner, server_task) =
            driver::spawn_standalone_with_join_token(model_server, token());
        let (joiner, joiner_task) = crate::formation_metrics::test_owner(source.clone());
        let owner = server_owner.clone();
        // Test-only fault at the real response-write boundary: owner admission
        // finishes, but no JoinAccepted bytes are delivered to the applicant.
        let fault = tokio::spawn(async move {
            let connection = endpoint.accept().await.unwrap().await.unwrap();
            let pool = ExchangePool::new(2).unwrap();
            let (send, receive) = connection.accept_bi().await.unwrap();
            let (session_send, session_receive) = tokio::sync::oneshot::channel();
            pool.serve(send, receive, Phase::Handshake, |bytes| async {
                let reply = owner
                    .accept_peer_handshake(Generation(0), connection.clone(), bytes)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                session_send.send(reply.session).unwrap();
                Ok(reply.bytes)
            })
            .await
            .unwrap();
            let session = session_receive.await.unwrap();
            let (send, receive) = connection.accept_bi().await.unwrap();
            let result = pool
                .serve(send, receive, Phase::Membership, |bytes| async {
                    let ack = owner
                        .receive_peer_packet(Generation(0), session, Transport::Stream, bytes)
                        .unwrap()
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(ack.is_some());
                    assert_eq!(owner.view().unwrap().summary.member_count, 2);
                    Err(ExchangeError::Owner)
                })
                .await;
            assert!(result.is_err());
            connection.close(0_u32.into(), b"injected ACK loss");
        });
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            let pending = Dialer::default()
                .handshake(
                    &client,
                    &client_identity,
                    source.local(),
                    &target,
                    &ExchangePool::new(2).unwrap(),
                )
                .await
                .unwrap();
            let binding = joiner
                .accept_introducer_handshake_reply(Generation(0), pending, target.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            #[cfg(feature = "observability")]
            assert_eq!(
                joiner.formation_counters().unwrap().registry_pressure(),
                crate::formation_metrics::RegistryPressure {
                    slots_in_use: 1,
                    slots_capacity: 64,
                    provisional_slots_in_use: 1,
                    provisional_slots_capacity: 16,
                },
                "outgoing introducer binding uses the shared provisional budget"
            );
            joiner
                .begin_join(
                    Generation(0),
                    source.formation().clone(),
                    target.formation.clone(),
                    binding.session,
                    token(),
                )
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            fault.await.unwrap();
            while joiner.view().unwrap().summary.participation
                == orishu::model::cluster::Participation::Joining
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let view = joiner.view().unwrap();
            assert_eq!(
                view.summary.participation,
                orishu::model::cluster::Participation::JoinUnresolved
            );
            assert_eq!(view.summary.formation_id, *source.formation());
            assert_eq!(view.summary.member_count, 1);
            assert_eq!(server_owner.view().unwrap().summary.member_count, 2);
            #[cfg(feature = "observability")]
            {
                use crate::formation_metrics::Event;
                let counters = joiner.formation_counters().unwrap();
                assert_eq!(counters.get(Event::JoinRetryDue), 1);
                assert_eq!(counters.get(Event::JoinAbandoned), 1);
                assert_eq!(counters.get(Event::StaleTimerInput), 0);
                assert_eq!(joiner.counters().admissions.rejected, 0);
            }
            assert!(
                joiner
                    .begin_join(
                        Generation(0),
                        source.formation().clone(),
                        target.formation.clone(),
                        binding.session,
                        token()
                    )
                    .unwrap()
                    .await
                    .unwrap()
                    .is_err()
            );
        })
        .await
        .unwrap();
        joiner.shutdown().await.unwrap();
        server_owner.shutdown().await.unwrap();
        assert_eq!(joiner_task.await.unwrap(), Ok(()));
        assert_eq!(server_task.await.unwrap(), Ok(()));
        client.close(0_u32.into(), b"test complete");
    }

    #[tokio::test]
    async fn real_silent_dials_bound_capacity_and_release_it_on_cancellation() {
        use std::sync::Arc;
        tokio::time::timeout(Duration::from_secs(5), async {
            let identity = Arc::new(PeerIdentity::generate().unwrap());
            let membership = model(&identity, "saturated-dialer");
            let local = membership.local().clone();
            let formation = membership.formation().clone();
            let (owner, owner_task) = driver::spawn_standalone(membership);
            let endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let dialer = Arc::new(observed_dialer());
            let pool = owner.exchange_pool();
            let mut sinks = Vec::new();
            let mut targets = Vec::new();
            let mut jobs = tokio::task::JoinSet::new();
            for index in 0..4 {
                let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
                let target = MemberTarget(IntroducerTarget {
                    formation: formation.clone(),
                    node: format!("unreachable-{index}").parse().unwrap(),
                    fingerprint: identity.fingerprint(),
                    endpoints: vec![sink.local_addr().unwrap()],
                });
                sinks.push(sink);
                targets.push(target.clone());
                let (dialer, endpoint, identity, local, pool) = (
                    dialer.clone(),
                    endpoint.clone(),
                    identity.clone(),
                    local.clone(),
                    pool.clone(),
                );
                jobs.spawn(async move {
                    dialer
                        .member_handshake(&endpoint, &identity, &local, &target, &pool)
                        .await
                });
            }
            // Four independent sockets must see QUIC Initial packets: these are
            // actual pending attempts, not permits taken directly by the test.
            let mut bytes = [0; 2048];
            for sink in &sinks {
                tokio::time::timeout(Duration::from_secs(1), sink.recv_from(&mut bytes))
                    .await
                    .expect("each dial emitted real UDP traffic")
                    .unwrap();
            }
            assert_eq!(jobs.len(), 4);
            assert_eq!(dialer.0.available_permits(), 0);
            #[cfg(feature = "observability")]
            {
                assert_eq!(dialer.pressure(), (4, 4));
                assert_counts(&dialer, [0; 4], [0; 4], 0);
            }
            assert!(matches!(
                tokio::time::timeout(
                    Duration::from_millis(100),
                    dialer.member_handshake(&endpoint, &identity, &local, &targets[0], &pool,)
                )
                .await
                .expect("fifth dial refuses without waiting"),
                Err(DialError::Overloaded)
            ));
            tokio::time::timeout(Duration::from_secs(1), async {
                owner
                    .set_membership_lock(Generation(0), formation, true)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert!(owner.view().unwrap().summary.membership_locked);
                // The adapter test owns these IO tasks; cancelling them mirrors
                // the runtime's JoinSet shutdown without inventing owner coupling.
                jobs.shutdown().await;
                assert!(jobs.is_empty());
                assert_eq!(dialer.0.available_permits(), 4);
                #[cfg(feature = "observability")]
                {
                    assert_eq!(dialer.pressure(), (0, 4));
                    assert_counts(&dialer, [0, 0, 0, 4], [0, 0, 0, 4], 1);
                }
                owner.shutdown().await.unwrap();
                assert_eq!(owner_task.await.unwrap(), Ok(()));
            })
            .await
            .expect("control, IO cancellation and owner shutdown remain bounded");
            endpoint.close(0_u32.into(), b"test complete");
        })
        .await
        .expect("saturated dial fixture stays bounded");
    }

    #[tokio::test]
    async fn late_admitted_handshake_is_fenced_after_leave_and_shutdown() {
        use crate::peer::registry::RegistryError;
        use orishu_membership::{Address, Command, Message};

        // Separate owners and actual pinned QUIC/application handshakes. The
        // already-admitted models isolate reconnect completion, not admission.
        tokio::time::timeout(Duration::from_secs(20), async {
            for case in ["current", "leave", "queued-shutdown", "closed-owner"] {
                let server_identity = PeerIdentity::generate().unwrap();
                let client_identity = PeerIdentity::generate().unwrap();
                let endpoint = quinn::Endpoint::server(
                    server_identity.server_config().unwrap(),
                    "127.0.0.1:0".parse().unwrap(),
                )
                .unwrap();
                let initial = model(&server_identity, "late-member-server");
                let mut local = initial.local().clone();
                local.endpoints.peers = vec![Address(endpoint.local_addr().unwrap().to_string())];
                let mut server_model = Membership::standalone(
                    initial.formation().clone(),
                    initial.cluster_name().clone(),
                    local,
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
                let applicant = model(&client_identity, "late-member-client");
                let mut client_model = Membership::standalone(
                    initial.formation().clone(),
                    initial.cluster_name().clone(),
                    applicant.local().clone(),
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
                testing::insert_member(
                    &mut server_model,
                    client_model.members()[client_model.local_id()].clone(),
                );
                testing::insert_member(
                    &mut client_model,
                    server_model.members()[server_model.local_id()].clone(),
                );
                let target =
                    MemberTarget::from_model(&client_model, server_model.local_id()).unwrap();
                let local = client_model.local().clone();
                let (server_owner, server_task) = driver::spawn_standalone(server_model);
                let (client_owner, client_task) = driver::spawn_standalone(client_model);
                let dispatcher = server::spawn(endpoint, server_owner.clone());
                let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
                let pending = Dialer::default()
                    .member_handshake(
                        &client,
                        &client_identity,
                        &local,
                        &target,
                        &client_owner.exchange_pool(),
                    )
                    .await
                    .unwrap();
                let old = client_owner.view().unwrap();
                let (connection, bytes) = pending.into_parts();
                assert!(connection.close_reason().is_none(), "{case}: held success");
                if case == "leave" {
                    client_owner
                        .try_submit(
                            old.generation,
                            Message::Local(Command::Leave {
                                replacement_formation: "replacement".parse().unwrap(),
                                replacement_node_id: "replacement-node".parse().unwrap(),
                                replacement_cluster_name: "replacement-label".parse().unwrap(),
                            }),
                        )
                        .unwrap();
                    while client_owner.view().unwrap().generation == old.generation {
                        tokio::task::yield_now().await;
                    }
                } else if case == "closed-owner" {
                    client_owner.shutdown().await.unwrap();
                    client_owner.closed().await;
                }
                // The unregistered outgoing connection survives replacement;
                // refusal/disposal below, not fixture cleanup, must close it.
                assert!(
                    connection.close_reason().is_none(),
                    "{case}: before delivery"
                );
                let registration = client_owner.accept_member_handshake_reply(
                    old.generation,
                    connection.clone(),
                    bytes,
                );
                if case == "queued-shutdown" {
                    // No yield between publication and reserved shutdown. Keep
                    // both the observer connection and owner handle alive.
                    client_owner.shutdown().await.unwrap();
                    client_owner.closed().await;
                    assert!(registration.unwrap().await.is_err());
                } else if case == "closed-owner" {
                    assert!(matches!(registration, Err(RegistryError::Unavailable)));
                } else {
                    let result = registration.unwrap().await.unwrap();
                    if case == "current" {
                        assert!(result.is_ok(), "current generation must still register");
                        assert!(connection.close_reason().is_none());
                        assert_eq!(client_owner.view().unwrap().summary.member_count, 2);
                    } else {
                        assert!(matches!(result, Err(RegistryError::Stale)));
                        let current = client_owner.view().unwrap();
                        assert_eq!(current.summary.formation_id.as_str(), "replacement");
                        assert_eq!(current.summary.source_node_id.as_str(), "replacement-node");
                        assert_eq!(current.summary.member_count, 1);
                        assert_eq!(current.completed_exchanges, old.completed_exchanges);
                        assert_eq!(current.admissions, old.admissions);
                    }
                }
                if case != "current" {
                    assert!(matches!(
                        tokio::time::timeout(Duration::from_secs(1), connection.closed())
                            .await
                            .expect("owner must dispose late admitted handshake"),
                        quinn::ConnectionError::LocallyClosed
                    ));
                    tokio::time::timeout(Duration::from_secs(3), client.wait_idle())
                        .await
                        .expect("refused connection drains without fixture closure");
                    assert_eq!(client.open_connections(), 0);
                }
                if matches!(case, "current" | "leave") {
                    client_owner.shutdown().await.unwrap();
                }
                assert_eq!(client_task.await.unwrap(), Ok(()));
                server_owner.shutdown().await.unwrap();
                assert_eq!(server_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
            }
        })
        .await
        .expect("late admitted handshake fixture deadline");
    }

    #[tokio::test]
    async fn admitted_dial_skips_silent_candidate_and_restores_policy_exchange() {
        use orishu_membership::{Address, AdmissionPolicy, Limits};
        tokio::time::timeout(Duration::from_secs(30), async {
            let server_identity = PeerIdentity::generate().unwrap();
            let client_identity = PeerIdentity::generate().unwrap();
            // A bound, silent UDP socket prevents connection-refused shortcuts.
            let silent = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let endpoint = quinn::Endpoint::server(
                server_identity.server_config().unwrap(), "127.0.0.1:0".parse().unwrap(),
            ).unwrap();
            let initial = model(&server_identity, "fallback-server");
            let mut local = initial.local().clone();
            local.endpoints.peers = vec![
                Address(silent.local_addr().unwrap().to_string()),
                Address(endpoint.local_addr().unwrap().to_string()),
            ];
            let mut server_model = Membership::standalone(
                initial.formation().clone(), initial.cluster_name().clone(), local,
                AdmissionPolicy::default(), Limits::default(),
            ).unwrap();
            let applicant = model(&client_identity, "fallback-client");
            let mut client_model = Membership::standalone(
                initial.formation().clone(), initial.cluster_name().clone(), applicant.local().clone(),
                AdmissionPolicy::default(), Limits::default(),
            ).unwrap();
            // Already-admitted fixture: the test exercises reconnection, not
            // bootstrap. Both owners independently bind the same IDs and pins.
            testing::insert_member(&mut server_model, client_model.members()[client_model.local_id()].clone());
            testing::insert_member(&mut client_model, server_model.members()[server_model.local_id()].clone());
            let target = MemberTarget::from_model(&client_model, server_model.local_id()).unwrap();
            let local = client_model.local().clone();
            let formation = server_model.formation().clone();
            let (server_owner, server_task) = driver::spawn_standalone(server_model);
            let (client_owner, client_task) = driver::spawn_standalone(client_model);
            let dispatcher = server::spawn(endpoint, server_owner.clone());
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let dialer = observed_dialer();
            let pool = client_owner.exchange_pool();
            let dialing = dialer.member_handshake(&client, &client_identity, &local, &target, &pool);
            tokio::pin!(dialing);
            let mut bytes = [0; 2048];
            tokio::select! {
                _ = &mut dialing => panic!("dial finished before silent candidate received traffic"),
                packet = silent.recv_from(&mut bytes) => { assert!(packet.unwrap().0 > 0); }
            }
            assert_eq!(dialer.0.available_permits(), 3);
            tokio::time::timeout(Duration::from_secs(1), async {
                server_owner.set_membership_lock(Generation(0), formation.clone(), true)
                    .unwrap().await.unwrap().unwrap();
                assert!(server_owner.view().unwrap().summary.membership_locked);
                assert!(!client_owner.view().unwrap().summary.membership_locked);
            }).await.expect("unreachable dial does not block owner control");
            let pending = tokio::time::timeout(Duration::from_secs(7), &mut dialing)
                .await.expect("five-second candidate deadline reaches healthy fallback").unwrap();
            assert_eq!(dialer.0.available_permits(), 4);
            #[cfg(feature = "observability")]
            {
                assert_counts(&dialer, [1, 0, 0, 0], [1, 0, 1, 0], 0);
                let snapshot = dialer.counters().unwrap();
                assert!(snapshot.stage(Stage::Attempt).duration_micros >= 4_500_000);
                assert!(snapshot.stage(Stage::Tls).duration_micros >= 4_500_000);
            }
            let (connection, reply) = pending.into_parts();
            let accepted = client_owner.accept_member_handshake_reply(Generation(0), connection.clone(), reply)
                .unwrap().await.unwrap().unwrap();
            let pump_owner = client_owner.clone();
            let pump_connection = connection.clone();
            let pump_pool = pool.clone();
            let pump = tokio::spawn(async move {
                server::serve_registered(&pump_connection, &pump_owner, Generation(0), accepted.session, &pump_pool).await;
            });
            // An initial round can have started before a route existed. Its
            // configured timeout must expire before the next five-second tick.
            let recovery_budget = Duration::from_millis(Limits::default().anti_entropy_timeout().0)
                + Duration::from_secs(7);
            let reconciled = tokio::time::timeout(recovery_budget, async {
                loop {
                    let view = client_owner.view().unwrap();
                    assert_eq!(view.summary.formation_id, formation);
                    assert_eq!(view.summary.member_count, 2);
                    if view.summary.membership_locked && view.completed_exchanges > 0 { break; }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await;
            assert!(reconciled.is_ok(), "fallback reconciliation: server {:?}, client {:?}, connection {:?}", server_owner.view(), client_owner.view(), connection.close_reason());
            tokio::time::timeout(Duration::from_secs(2), async {
                client_owner.shutdown().await.unwrap();
                server_owner.shutdown().await.unwrap();
                assert_eq!(client_task.await.unwrap(), Ok(()));
                assert_eq!(server_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                pump.await.unwrap();
                connection.closed().await;
            }).await.expect("fallback owners and connection tasks shut down");
        }).await.expect("member fallback fixture stays bounded");
    }

    #[tokio::test]
    async fn real_dispatcher_dial_pins_tls_and_owner_ack_identity() {
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let model_server = model(&server_identity, "server");
        let model_client = model(&client_identity, "client");
        let endpoint = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let target = IntroducerTarget {
            formation: model_server.formation().clone(),
            node: model_server.local_id().clone(),
            fingerprint: server_identity.fingerprint(),
            endpoints: vec![endpoint.local_addr().unwrap()],
        };
        let (owner, task) = driver::spawn_standalone_with_join_token(
            model_server,
            crate::credentials::SecretToken::parse("a".repeat(64)).unwrap(),
        );
        let server = server::spawn(endpoint, owner.clone());
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let dialer = observed_dialer();
        let pool = ExchangePool::new(4).unwrap();
        let exercise = async {
            let pending = dialer
                .handshake(
                    &client,
                    &client_identity,
                    model_client.local(),
                    &target,
                    &pool,
                )
                .await
                .unwrap();
            let (connection, reply) = pending.into_parts();
            let mut session = AuthenticatedSession::from_connection(
                &connection,
                SessionId(1),
                Generation(0),
                model_client.formation().clone(),
            )
            .unwrap();
            assert!(
                handshake::accept_pinned_introducer_reply(
                    &reply,
                    &mut session,
                    &model_client,
                    Generation(0),
                    target.formation.clone(),
                    &"wrong-introducer".parse().unwrap(),
                    target.fingerprint
                )
                .is_err()
            );
            handshake::accept_pinned_introducer_reply(
                &reply,
                &mut session,
                &model_client,
                Generation(0),
                target.formation.clone(),
                &target.node,
                target.fingerprint,
            )
            .unwrap();
            assert_eq!(owner.view().unwrap().summary.member_count, 1);
            #[cfg(feature = "observability")]
            assert_counts(&dialer, [1, 0, 0, 0], [1, 0, 0, 0], 0);
            connection.close(0_u32.into(), b"test complete");
            let mut wrong = target.clone();
            wrong.fingerprint = client_identity.fingerprint();
            assert!(matches!(
                dialer
                    .handshake(
                        &client,
                        &client_identity,
                        model_client.local(),
                        &wrong,
                        &pool
                    )
                    .await,
                Err(DialError::Unavailable)
            ));
            #[cfg(feature = "observability")]
            assert_counts(&dialer, [1, 1, 0, 0], [1, 1, 0, 0], 0);
            let permits = dialer.0.try_acquire_many(4).unwrap();
            assert!(matches!(
                dialer
                    .handshake(
                        &client,
                        &client_identity,
                        model_client.local(),
                        &target,
                        &pool
                    )
                    .await,
                Err(DialError::Overloaded)
            ));
            drop(permits);
            let pending = dialer
                .handshake(
                    &client,
                    &client_identity,
                    model_client.local(),
                    &target,
                    &pool,
                )
                .await
                .unwrap();
            let retained = pending.connection.as_ref().unwrap().clone();
            drop(pending);
            assert!(retained.close_reason().is_some());
            let (joiner, joiner_task) = driver::spawn_standalone(model(&client_identity, "client"));
            for scenario in 0..3 {
                let pending = dialer
                    .handshake(
                        &client,
                        &client_identity,
                        model_client.local(),
                        &target,
                        &pool,
                    )
                    .await
                    .unwrap();
                let retained = pending.connection.as_ref().unwrap().clone();
                let mut expected = target.clone();
                if scenario == 1 {
                    expected.node = "wrong-introducer".parse().unwrap();
                }
                let generation = if scenario == 2 {
                    Generation(99)
                } else {
                    Generation(0)
                };
                let result = joiner
                    .accept_introducer_handshake_reply(generation, pending, expected)
                    .unwrap()
                    .await
                    .unwrap();
                match scenario {
                    0 => {
                        assert_eq!(result.unwrap().session, SessionId(1));
                        assert_eq!(joiner.view().unwrap().summary.member_count, 1);
                        assert_eq!(
                            joiner.view().unwrap().summary.formation_id,
                            *model_client.formation()
                        );
                        retained.close(0_u32.into(), b"test complete");
                    }
                    1 => assert!(matches!(
                        result,
                        Err(crate::peer::registry::RegistryError::Handshake)
                    )),
                    _ => assert!(matches!(
                        result,
                        Err(crate::peer::registry::RegistryError::Stale)
                    )),
                }
                assert!(retained.close_reason().is_some());
            }
            let request = orishu::model::cluster::JoinRequest {
                schema_version: 1,
                operation_id: "real-join".parse().unwrap(),
                formation_id: model_client.formation().clone(),
                material: orishu::model::cluster::JoinMaterial {
                    schema_version: 1,
                    formation_id: target.formation.clone(),
                    introducer_node_id: target.node.clone(),
                    introducer_fingerprint: target.fingerprint,
                    peer_endpoints: target.endpoints.iter().map(ToString::to_string).collect(),
                    introducer_ready: true,
                    token: "a".repeat(64).try_into().unwrap(),
                },
            };
            let driver::PreparedJoin::Start(job) = joiner
                .prepare_join(request.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap()
            else {
                panic!("new operation must start");
            };
            let pending = dialer
                .handshake(
                    &client,
                    &client_identity,
                    model_client.local(),
                    &target,
                    &pool,
                )
                .await
                .unwrap();
            job.finish(pending);
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let view = joiner.view().unwrap();
                    if view.summary.formation_id == target.formation {
                        assert_eq!(view.summary.member_count, 2);
                        assert_eq!(
                            view.summary.participation,
                            orishu::model::cluster::Participation::CatchingUp
                        );
                        assert!(!view.summary.introducer_ready);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("owner-issued JoinReq and validated owner adoption");
            let operation = joiner
                .join_status(request.operation_id.clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(
                operation.state,
                orishu::model::cluster::JoinOperationState::CatchingUp { .. }
            ));
            assert!(matches!(
                joiner
                    .prepare_join(request)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap(),
                driver::PreparedJoin::Replay(_)
            ));
            assert_eq!(owner.view().unwrap().summary.member_count, 2);
            assert!(joiner.join_material().unwrap().await.unwrap().is_none());
            joiner.shutdown().await.unwrap();
            assert_eq!(joiner_task.await.unwrap(), Ok(()));
        };
        tokio::time::timeout(Duration::from_secs(10), exercise)
            .await
            .unwrap();
        owner.shutdown().await.unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        server.await.unwrap();
        client.close(0_u32.into(), b"test complete");
    }
}
