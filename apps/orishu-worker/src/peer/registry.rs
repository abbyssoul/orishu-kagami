//! Owner-side connection registry. TLS facts and application binding are checked
//! against the live membership model, never an IO task's stale copy of it.

use super::{admission, handshake, session::AuthenticatedSession};
use crate::driver::Generation;
use orishu_membership::{
    Membership, SenderIdentity, SessionId,
    model::{BlocklistAction, BlocklistKey},
};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::Instant;

const MAX_SESSIONS: usize = 64;
const MAX_APPLICANTS: usize = 16;
const APPLICANT_LIFETIME: Duration = Duration::from_secs(10);

/// Bounded refusal without peer-controlled diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("peer connection capacity exhausted")]
    Overloaded,
    #[error("peer handshake or TLS binding rejected")]
    Handshake,
    #[error("peer handshake belongs to a stale owner generation")]
    Stale,
    #[error("a live connection already owns this certificate binding")]
    Duplicate,
    #[error("peer session identity space exhausted")]
    Exhausted,
    #[error("membership owner is unavailable")]
    Unavailable,
    #[error("peer source is blocked or its network policy is invalid")]
    SourceBlocked,
    #[error("peer session is absent, expired or no longer authorized")]
    UnknownSession,
    #[error("peer packet does not match the bounded session protocol")]
    InvalidPacket,
}

/// Successful application binding, not membership admission. Send the bytes on
/// the original handshake stream; subsequent IO must retain this session ID.
pub struct HandshakeReply {
    /// Process-unique session correlation, never reused after formation changes.
    pub session: SessionId,
    /// Encoded HandshakeAck for the same bidirectional stream.
    pub bytes: Vec<u8>,
}

struct Entry {
    connection: quinn::Connection,
    session: AuthenticatedSession,
    provisional_until: Option<Instant>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        // Connection clones in IO tasks must not prolong an invalid binding.
        self.connection.close(0_u32.into(), b"session retired");
    }
}

/// Fixed-capacity registry kept beside the serialized membership model.
#[derive(Default)]
pub(crate) struct Registry {
    next: u64,
    sessions: BTreeMap<SessionId, Entry>,
    #[cfg(feature = "observability")]
    metrics: crate::formation_metrics::FormationMetrics,
    // Diagnostic bookkeeping only: the existing admission gate still reads
    // actual entries. Compiled-out builds retain no additional count.
    #[cfg(feature = "observability")]
    provisional: usize,
}

impl Registry {
    pub(crate) fn observed(metrics: crate::formation_metrics::FormationMetrics) -> Self {
        let registry = Self {
            next: 0,
            sessions: BTreeMap::new(),
            #[cfg(feature = "observability")]
            metrics,
            #[cfg(feature = "observability")]
            provisional: 0,
        };
        #[cfg(not(feature = "observability"))]
        let _ = metrics;
        registry.publish_pressure();
        registry
    }

    fn publish_pressure(&self) {
        #[cfg(feature = "observability")]
        self.metrics.registry(
            self.sessions.len(),
            MAX_SESSIONS,
            self.provisional,
            MAX_APPLICANTS,
        );
    }
    /// Retire all connections without reusing process-scoped session IDs.
    pub(crate) fn close_all(&self) {
        for entry in self.sessions.values() {
            entry
                .connection
                .close(0_u32.into(), b"membership participation stopped");
        }
    }
    #[cfg(test)]
    pub(crate) fn disconnect_all(&self) {
        for entry in self.sessions.values() {
            entry
                .connection
                .close(0_u32.into(), b"test connection loss");
        }
    }
    /// Upgrade only after the core inserted this certificate under `node`.
    /// A successful upgrade retires the provisional handshake lifetime.
    pub fn promote(
        &mut self,
        model: &Membership,
        generation: Generation,
        session: SessionId,
        node: orishu_membership::NodeId,
    ) -> Result<(), RegistryError> {
        self.synchronize(model, generation);
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or(RegistryError::UnknownSession)?;
        entry
            .session
            .bind_member(model, generation, node)
            .map_err(|_| RegistryError::UnknownSession)?;
        #[cfg(feature = "observability")]
        if entry.provisional_until.is_some() {
            self.provisional -= 1;
        }
        entry.provisional_until = None;
        self.publish_pressure();
        Ok(())
    }

    /// Resolve only a currently admitted, source-authorized session. Missing
    /// connections are transport failures, never permission to use another peer.
    pub fn member_connection(
        &mut self,
        model: &Membership,
        generation: Generation,
        node: &orishu_membership::NodeId,
    ) -> Option<(SessionId, quinn::Connection)> {
        self.synchronize(model, generation);
        self.sessions.iter().find_map(|(id, entry)| {
            let context = entry.session.context(model, generation).ok()?;
            (context.sender == SenderIdentity::Admitted(node.clone()))
                .then(|| (*id, entry.connection.clone()))
        })
    }

    /// Resolve only a provisional introducer in another formation.
    pub fn introducer_connection(
        &mut self,
        model: &Membership,
        generation: Generation,
        session: SessionId,
        target: &orishu_membership::FormationId,
    ) -> Option<quinn::Connection> {
        self.synchronize(model, generation);
        let entry = self.sessions.get(&session)?;
        let context = entry.session.context(model, generation).ok()?;
        (&context.formation == target && target != model.formation())
            .then(|| entry.connection.clone())
    }

    /// Current authenticated facts for a correlated credential check. Recheck
    /// source policy and lifetime immediately before producing shell evidence.
    pub fn credential_context(
        &mut self,
        model: &Membership,
        generation: Generation,
        session: SessionId,
    ) -> Result<(orishu_membership::PeerContext, std::net::IpAddr), RegistryError> {
        self.synchronize(model, generation);
        let entry = self
            .sessions
            .get(&session)
            .ok_or(RegistryError::UnknownSession)?;
        let context = entry
            .session
            .context(model, generation)
            .map_err(|_| RegistryError::UnknownSession)?;
        Ok((context, entry.connection.remote_address().ip()))
    }

    /// Revalidate current transport source, identity and lifetime immediately
    /// before decoding a packet. The caller must apply its core input in this
    /// same owner turn; the decoded token remains shell-only evidence.
    pub fn decode_packet(
        &mut self,
        model: &Membership,
        generation: Generation,
        session: SessionId,
        bytes: &[u8],
        transport: super::wire::Transport,
    ) -> Result<super::wire::Decoded, RegistryError> {
        self.synchronize(model, generation);
        let entry = self
            .sessions
            .get(&session)
            .ok_or(RegistryError::UnknownSession)?;
        entry
            .session
            .decode(bytes, model, generation, transport)
            .map_err(|_| RegistryError::InvalidPacket)
    }

    /// Prune closed, expired, removed and old-generation sessions. The owner
    /// calls this after transitions and periodic wakeups. At most 64 binding
    /// checks consult the model's bounded membership and exclusion views.
    pub fn synchronize(&mut self, model: &Membership, generation: Generation) {
        self.synchronize_at(model, generation, Instant::now());
    }

    fn synchronize_at(&mut self, model: &Membership, generation: Generation, now: Instant) {
        let before = self.sessions.len();
        #[cfg(feature = "observability")]
        let mut retired_provisional = 0;
        self.sessions.retain(|_, entry| {
            let keep = entry.connection.close_reason().is_none()
                && entry
                    .provisional_until
                    .is_none_or(|deadline| now < deadline)
                && entry.session.context(model, generation).is_ok()
                && !source_blocked(model, &entry.connection);
            #[cfg(feature = "observability")]
            if !keep && entry.provisional_until.is_some() {
                retired_provisional += 1;
            }
            keep
        });
        #[cfg(feature = "observability")]
        {
            self.provisional -= retired_provisional;
        }
        if self.sessions.len() != before {
            self.publish_pressure();
        }
    }

    pub fn accept(
        &mut self,
        model: &Membership,
        generation: Generation,
        connection: quinn::Connection,
        bytes: &[u8],
    ) -> Result<HandshakeReply, RegistryError> {
        self.register(
            model,
            generation,
            connection,
            bytes,
            HandshakeMode::Incoming,
        )
    }

    /// Bind a completed outbound member handshake only against a member already
    /// held in this formation. This is not the unknown-introducer join role.
    pub fn accept_member_reply(
        &mut self,
        model: &Membership,
        generation: Generation,
        connection: quinn::Connection,
        bytes: &[u8],
    ) -> Result<HandshakeReply, RegistryError> {
        self.register(
            model,
            generation,
            connection,
            bytes,
            HandshakeMode::MemberReply,
        )
    }

    /// Register a pinned introducer as provisional, never as an admitted member.
    pub fn accept_introducer_reply(
        &mut self,
        model: &Membership,
        generation: Generation,
        connection: quinn::Connection,
        bytes: &[u8],
        target: super::dial::IntroducerTarget,
    ) -> Result<HandshakeReply, RegistryError> {
        self.register(
            model,
            generation,
            connection,
            bytes,
            HandshakeMode::IntroducerReply(target),
        )
    }

    fn register(
        &mut self,
        model: &Membership,
        generation: Generation,
        connection: quinn::Connection,
        bytes: &[u8],
        mode: HandshakeMode,
    ) -> Result<HandshakeReply, RegistryError> {
        self.synchronize(model, generation);
        let result = self.bind(model, generation, &connection, bytes, mode);
        if result.is_err() {
            connection.close(0_u32.into(), b"handshake rejected");
        }
        result
    }

    fn bind(
        &mut self,
        model: &Membership,
        generation: Generation,
        connection: &quinn::Connection,
        bytes: &[u8],
        mode: HandshakeMode,
    ) -> Result<HandshakeReply, RegistryError> {
        if source_blocked(model, connection) {
            return Err(RegistryError::SourceBlocked);
        }
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(RegistryError::Overloaded);
        }
        let next = self.next.checked_add(1).ok_or(RegistryError::Exhausted)?;
        let id = SessionId(next);
        let mut session = AuthenticatedSession::from_connection(
            connection,
            id,
            generation,
            model.formation().clone(),
        )
        .map_err(|_| RegistryError::Handshake)?;
        let introducer = matches!(&mode, HandshakeMode::IntroducerReply(_));
        if let HandshakeMode::IntroducerReply(target) = &mode {
            if &target.formation == model.formation() {
                return Err(RegistryError::Handshake);
            }
            handshake::accept_pinned_introducer_reply(
                bytes,
                &mut session,
                model,
                generation,
                target.formation.clone(),
                &target.node,
                target.fingerprint,
            )
            .map_err(|_| RegistryError::Handshake)?;
        } else if matches!(mode, HandshakeMode::MemberReply) {
            let fingerprint = session.fingerprint();
            handshake::accept_reply(
                bytes,
                &mut session,
                model,
                generation,
                model.formation().clone(),
                fingerprint,
            )
            .map_err(|_| RegistryError::Handshake)?;
        } else {
            handshake::accept_request(bytes, &mut session, model, generation)
                .map_err(|_| RegistryError::Handshake)?;
        }
        if self
            .sessions
            .values()
            .any(|entry| entry.session.fingerprint() == session.fingerprint())
        {
            // First live binding wins. Do not disconnect an established peer in
            // response to a duplicate connection. Retry after it has closed.
            return Err(RegistryError::Duplicate);
        }
        let applicant = introducer
            || matches!(
                session
                    .context(model, generation)
                    .map_err(|_| RegistryError::Handshake)?
                    .sender,
                SenderIdentity::Applicant(_)
            );
        if applicant
            && self
                .sessions
                .values()
                .filter(|entry| entry.provisional_until.is_some())
                .count()
                >= MAX_APPLICANTS
        {
            return Err(RegistryError::Overloaded);
        }
        let bytes = handshake::reply(model.local(), model.formation().clone(), None)
            .map_err(|_| RegistryError::Handshake)?;
        self.next = next;
        self.sessions.insert(
            id,
            Entry {
                connection: connection.clone(),
                session,
                provisional_until: applicant.then(|| Instant::now() + APPLICANT_LIFETIME),
            },
        );
        #[cfg(feature = "observability")]
        if applicant {
            self.provisional += 1;
        }
        self.publish_pressure();
        Ok(HandshakeReply { session: id, bytes })
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        #[cfg(feature = "observability")]
        self.metrics.registry(0, 0, 0, 0);
    }
}

pub(crate) enum HandshakeMode {
    Incoming,
    MemberReply,
    IntroducerReply(super::dial::IntroducerTarget),
}

fn source_blocked(model: &Membership, connection: &quinn::Connection) -> bool {
    let patterns = model.blocklist().values().filter_map(|entry| {
        if entry.action == BlocklistAction::Block
            && let BlocklistKey::Network(pattern) = &entry.key
        {
            Some(pattern)
        } else {
            None
        }
    });
    admission::check_source(connection.remote_address().ip(), patterns).blocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{
        exchange::{ExchangePool, Phase},
        tls::{PeerIdentity, SERVER_NAME},
    };
    use orishu_membership::{AdmissionPolicy, Command, Limits, Message, testing};

    fn test_metrics() -> crate::formation_metrics::FormationMetrics {
        #[cfg(feature = "observability")]
        {
            crate::formation_metrics::FormationMetrics::enabled()
        }
        #[cfg(not(feature = "observability"))]
        {
            crate::formation_metrics::FormationMetrics::default()
        }
    }

    fn observed_owner(
        model: Membership,
        token: Option<crate::credentials::SecretToken>,
    ) -> (
        crate::driver::Handle,
        tokio::task::JoinHandle<Result<(), crate::driver::DriverError>>,
    ) {
        crate::driver::spawn_standalone_observed(
            model,
            token,
            Default::default(),
            Default::default(),
            test_metrics(),
        )
    }

    fn registry_pressure(
        metrics: &crate::formation_metrics::FormationMetrics,
        used: u64,
        provisional: u64,
        running: bool,
    ) {
        #[cfg(feature = "observability")]
        assert_eq!(
            metrics.snapshot().unwrap().registry_pressure(),
            crate::formation_metrics::RegistryPressure {
                slots_in_use: used,
                slots_capacity: if running { 64 } else { 0 },
                provisional_slots_in_use: provisional,
                provisional_slots_capacity: if running { 16 } else { 0 },
            }
        );
        let _ = (metrics, used, provisional, running);
    }

    fn owner_registry_pressure(
        handle: &crate::driver::Handle,
        used: u64,
        provisional: u64,
        running: bool,
    ) {
        #[cfg(feature = "observability")]
        assert_eq!(
            handle.formation_counters().unwrap().registry_pressure(),
            crate::formation_metrics::RegistryPressure {
                slots_in_use: used,
                slots_capacity: if running { 64 } else { 0 },
                provisional_slots_in_use: provisional,
                provisional_slots_capacity: if running { 16 } else { 0 },
            }
        );
        let _ = (handle, used, provisional, running);
    }

    fn model(identity: &PeerIdentity, name: &str) -> Membership {
        let fixture = testing::standalone(name);
        let mut local = fixture.local().clone();
        local.cert_fingerprint = identity.fingerprint();
        Membership::standalone(
            format!("formation-{name}").parse().unwrap(),
            fixture.cluster_name().clone(),
            local,
            AdmissionPolicy::default(),
            Limits::default(),
        )
        .unwrap()
    }

    async fn connect(
        server: &quinn::Endpoint,
        client: &quinn::Endpoint,
        server_identity: &PeerIdentity,
        client_identity: &PeerIdentity,
    ) -> (quinn::Connection, quinn::Connection) {
        tokio::time::timeout(Duration::from_secs(3), async {
            let connecting = client
                .connect_with(
                    client_identity
                        .client_config(server_identity.certificate())
                        .unwrap(),
                    server.local_addr().unwrap(),
                    SERVER_NAME,
                )
                .unwrap();
            let (outgoing, incoming) =
                tokio::join!(connecting, async { server.accept().await.unwrap().await });
            (outgoing.unwrap(), incoming.unwrap())
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn real_handshake_is_decided_by_owner_and_closed_when_formation_changes() {
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let server_model = model(&server_identity, "server");
        let client_model = model(&client_identity, "client");
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let (handle, owner) = observed_owner(server_model, None);
        let view = handle.view().unwrap();
        let request = handshake::request(
            client_model.local(),
            view.summary.formation_id.clone(),
            true,
        )
        .unwrap();
        let (outgoing, incoming) =
            connect(&server, &client, &server_identity, &client_identity).await;
        let pool = ExchangePool::new(4).unwrap();
        let exercise = async {
            let send = pool.request(&outgoing, &request, Phase::Handshake);
            let receive = async {
                let (send, receive) = incoming.accept_bi().await.unwrap();
                pool.serve(send, receive, Phase::Handshake, |bytes| async {
                    let accepted = handle
                        .accept_peer_handshake(view.generation, incoming.clone(), bytes)
                        .unwrap()
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(accepted.session, SessionId(1));
                    Ok(accepted.bytes)
                })
                .await
                .unwrap();
            };
            let (reply, ()) = tokio::join!(send, receive);
            let reply = reply.unwrap();
            let mut binding = AuthenticatedSession::from_connection(
                &outgoing,
                SessionId(1),
                Generation(0),
                client_model.formation().clone(),
            )
            .unwrap();
            handshake::accept_reply(
                &reply,
                &mut binding,
                &client_model,
                Generation(0),
                view.summary.formation_id.clone(),
                server_identity.fingerprint(),
            )
            .unwrap();
        };
        // This applicant is not admitted by application handshake alone.
        let result = tokio::time::timeout(Duration::from_secs(5), exercise).await;
        result.unwrap();
        owner_registry_pressure(&handle, 1, 1, true);

        assert_eq!(handle.counters().peer_decode_rejections, 0);
        // Both requests cross authenticated QUIC and actual stream framing.
        // Valid CBOR of the wrong shape and a handshake DTO are not membership
        // input. Invalid CBOR itself is rejected earlier by stream framing.
        for (index, payload) in [vec![0xf5], request.clone()].into_iter().enumerate() {
            tokio::time::timeout(Duration::from_secs(3), async {
                let sending = pool.request(&outgoing, &payload, Phase::Membership);
                let receiving = async {
                    let (send, receive) = incoming.accept_bi().await.unwrap();
                    pool.serve(send, receive, Phase::Membership, |bytes| async {
                        let result = handle
                            .receive_peer_packet(
                                view.generation,
                                SessionId(1),
                                super::super::wire::Transport::Stream,
                                bytes,
                            )
                            .unwrap()
                            .await
                            .unwrap();
                        assert!(result.is_err());
                        Err(crate::peer::exchange::ExchangeError::Owner)
                    })
                    .await
                };
                let (sent, served) = tokio::join!(sending, receiving);
                assert!(sent.is_err() && served.is_err());
            })
            .await
            .expect("rejected packet exchange deadline");
            assert_eq!(handle.counters().peer_decode_rejections, index as u64 + 1);
            assert_eq!(handle.counters().admissions, Default::default());
            assert_eq!(handle.view().unwrap().summary.member_count, 1);
        }

        let (duplicate_out, duplicate_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        let result = handle
            .accept_peer_handshake(view.generation, duplicate_in, request.clone())
            .unwrap()
            .await
            .unwrap();
        assert!(matches!(result, Err(RegistryError::Duplicate)));
        owner_registry_pressure(&handle, 1, 1, true);
        tokio::time::timeout(Duration::from_secs(2), duplicate_out.closed())
            .await
            .unwrap();
        assert!(
            outgoing.close_reason().is_none(),
            "duplicate displaced the original binding"
        );
        handle
            .try_submit(
                view.generation,
                Message::Local(Command::Leave {
                    replacement_formation: "replacement".parse().unwrap(),
                    replacement_node_id: "replacement-node".parse().unwrap(),
                    replacement_cluster_name: "replacement-label".parse().unwrap(),
                }),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), outgoing.closed())
            .await
            .unwrap();
        assert_eq!(handle.view().unwrap().generation, Generation(1));
        owner_registry_pressure(&handle, 0, 0, true);
        assert_eq!(handle.counters().peer_decode_rejections, 2);
        let (stale_out, stale_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        assert!(matches!(
            handle
                .accept_peer_handshake(view.generation, stale_in, request)
                .unwrap()
                .await
                .unwrap(),
            Err(RegistryError::Stale)
        ));
        tokio::time::timeout(Duration::from_secs(2), stale_out.closed())
            .await
            .unwrap();
        let current = handle.view().unwrap();
        let request =
            handshake::request(client_model.local(), current.summary.formation_id, true).unwrap();
        let (active_out, active_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        let accepted = handle
            .accept_peer_handshake(current.generation, active_in, request.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            accepted.session,
            SessionId(2),
            "session ID reused across formation change"
        );
        let (queued_out, queued_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        let queued_clone = queued_in.clone();
        let pending = handle
            .accept_peer_handshake(current.generation, queued_in, request)
            .unwrap();
        // No yield between enqueueing the handshake and reserved shutdown: the
        // owner must close both registered and discarded mailbox connections.
        handle.shutdown().await.unwrap();
        assert_eq!(owner.await.unwrap(), Ok(()));
        assert_eq!(handle.counters().peer_decode_rejections, 2);
        owner_registry_pressure(&handle, 0, 0, false);
        assert!(pending.await.is_err());
        tokio::time::timeout(Duration::from_secs(2), active_out.closed())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), queued_out.closed())
            .await
            .unwrap();
        assert!(queued_clone.close_reason().is_some());
        client.close(0_u32.into(), b"done");
        server.close(0_u32.into(), b"done");
    }

    #[tokio::test]
    async fn applicant_capacity_expiry_and_drop_are_bounded_with_real_connections() {
        let server_identity = PeerIdentity::generate().unwrap();
        let server_model = model(&server_identity, "server");
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let metrics = test_metrics();
        let mut registry = Registry::observed(metrics.clone());
        registry_pressure(&metrics, 0, 0, true);
        let mut outgoing = Vec::new();
        for index in 0..=MAX_APPLICANTS {
            let identity = PeerIdentity::generate().unwrap();
            let applicant = model(&identity, &format!("applicant-{index}"));
            let request =
                handshake::request(applicant.local(), server_model.formation().clone(), true)
                    .unwrap();
            let (sender, receiver) = connect(&server, &client, &server_identity, &identity).await;
            let result = registry.accept(&server_model, Generation(0), receiver, &request);
            if index < MAX_APPLICANTS {
                assert!(result.is_ok());
                outgoing.push(sender);
            } else {
                assert!(matches!(result, Err(RegistryError::Overloaded)));
                tokio::time::timeout(Duration::from_secs(2), sender.closed())
                    .await
                    .unwrap();
            }
        }
        assert_eq!(registry.sessions.len(), MAX_APPLICANTS);
        registry_pressure(&metrics, 16, 16, true);
        registry.synchronize_at(
            &server_model,
            Generation(0),
            Instant::now() + APPLICANT_LIFETIME,
        );
        assert!(registry.sessions.is_empty());
        registry_pressure(&metrics, 0, 0, true);
        assert_eq!(
            registry.next, MAX_APPLICANTS as u64,
            "session IDs must not reset with expiry"
        );
        for connection in outgoing {
            tokio::time::timeout(Duration::from_secs(2), connection.closed())
                .await
                .unwrap();
        }
        drop(registry);
        registry_pressure(&metrics, 0, 0, false);
        client.close(0_u32.into(), b"done");
        server.close(0_u32.into(), b"done");
    }

    #[tokio::test]
    async fn registered_member_capacity_refusal_and_reuse_are_distinct_from_provisional() {
        tokio::time::timeout(Duration::from_secs(15), async {
            let server_identity = PeerIdentity::generate().unwrap();
            let mut server_model = model(&server_identity, "server");
            let server = quinn::Endpoint::server(
                server_identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let metrics = test_metrics();
            let mut registry = Registry::observed(metrics.clone());
            let mut held = Vec::new();
            for index in 0..=MAX_SESSIONS {
                let identity = PeerIdentity::generate().unwrap();
                let member = model(&identity, &format!("member-{index}"));
                testing::insert_member(
                    &mut server_model,
                    member.members()[member.local_id()].clone(),
                );
                let request =
                    handshake::request(member.local(), server_model.formation().clone(), false)
                        .unwrap();
                let (outgoing, incoming) =
                    connect(&server, &client, &server_identity, &identity).await;
                let result = registry.accept(&server_model, Generation(0), incoming, &request);
                if index < MAX_SESSIONS {
                    assert!(
                        result.is_ok(),
                        "registered member {index}: {:?}",
                        result.err()
                    );
                    held.push(outgoing);
                    registry_pressure(&metrics, index as u64 + 1, 0, true);
                } else {
                    assert!(matches!(result, Err(RegistryError::Overloaded)));
                    registry_pressure(&metrics, 64, 0, true);
                    outgoing.closed().await;
                    // Locally close an actual registered connection, then let
                    // the same production synchronization path reclaim it.
                    registry
                        .sessions
                        .values()
                        .next()
                        .unwrap()
                        .connection
                        .close(0_u32.into(), b"release registered slot");
                    registry.synchronize(&server_model, Generation(0));
                    registry_pressure(&metrics, 63, 0, true);
                    let (outgoing, incoming) =
                        connect(&server, &client, &server_identity, &identity).await;
                    assert!(
                        registry
                            .accept(&server_model, Generation(0), incoming, &request)
                            .is_ok()
                    );
                    held.push(outgoing);
                    registry_pressure(&metrics, 64, 0, true);
                }
            }
            // Closing transport clones does not erase retained registry slots.
            registry.close_all();
            registry_pressure(&metrics, 64, 0, true);
            registry.synchronize(&server_model, Generation(0));
            registry_pressure(&metrics, 0, 0, true);
            drop(registry);
            registry_pressure(&metrics, 0, 0, false);
            for connection in held {
                connection.closed().await;
            }
            client.close(0_u32.into(), b"done");
            server.close(0_u32.into(), b"done");
        })
        .await
        .expect("bounded full-registry exercise");
    }

    #[tokio::test]
    async fn owner_abort_with_retained_session_withdraws_registry_pressure() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server_identity = PeerIdentity::generate().unwrap();
            let client_identity = PeerIdentity::generate().unwrap();
            let server_model = model(&server_identity, "server");
            let client_model = model(&client_identity, "client");
            let request =
                handshake::request(client_model.local(), server_model.formation().clone(), true)
                    .unwrap();
            let server = quinn::Endpoint::server(
                server_identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let (handle, owner) = observed_owner(server_model, None);
            let (outgoing, incoming) =
                connect(&server, &client, &server_identity, &client_identity).await;
            handle
                .accept_peer_handshake(Generation(0), incoming, request)
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            owner_registry_pressure(&handle, 1, 1, true);
            owner.abort();
            assert!(owner.await.unwrap_err().is_cancelled());
            owner_registry_pressure(&handle, 0, 0, false);
            outgoing.closed().await;
            client.close(0_u32.into(), b"done");
            server.close(0_u32.into(), b"done");
        })
        .await
        .expect("owner abort withdraws registry without waiting for transport cleanup");
    }

    #[tokio::test]
    async fn authenticated_reordered_datagrams_preserve_probe_ids_under_replay() {
        exercise_datagram_delivery(0).await;
    }

    #[tokio::test]
    async fn authenticated_acks_complete_only_their_outstanding_probe() {
        exercise_datagram_delivery(1).await;
    }

    #[tokio::test]
    async fn multiple_peers_exhaust_global_exchanges_and_recover_after_disconnect() {
        use crate::peer::server;
        tokio::time::timeout(Duration::from_secs(5), async {
            let identity = PeerIdentity::generate().unwrap();
            let initial = model(&identity, "global-pressure");
            let formation = initial.formation().clone();
            let (owner, owner_task) = crate::driver::spawn_standalone(initial);
            let endpoint = quinn::Endpoint::server(
                identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let address = endpoint.local_addr().unwrap();
            let dispatcher = server::spawn(endpoint, owner.clone());
            let mut clients = Vec::new();
            let mut connections = Vec::new();
            // Authenticate all provisional sessions before filling the shared
            // pool; each has distinct TLS identity and remains below 16 streams.
            for index in 0..5 {
                let peer = PeerIdentity::generate().unwrap();
                let applicant = model(&peer, &format!("pressure-{index}"));
                let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
                let connection = client
                    .connect_with(
                        peer.client_config(identity.certificate()).unwrap(),
                        address,
                        SERVER_NAME,
                    )
                    .unwrap()
                    .await
                    .unwrap();
                let reply = ExchangePool::new(1)
                    .unwrap()
                    .request(
                        &connection,
                        &handshake::request(applicant.local(), formation.clone(), true).unwrap(),
                        Phase::Handshake,
                    )
                    .await
                    .unwrap();
                let mut binding = AuthenticatedSession::from_connection(
                    &connection,
                    SessionId(1),
                    Generation(0),
                    applicant.formation().clone(),
                )
                .unwrap();
                handshake::accept_reply(
                    &reply,
                    &mut binding,
                    &applicant,
                    Generation(0),
                    formation.clone(),
                    identity.fingerprint(),
                )
                .unwrap();
                clients.push(client);
                connections.push(connection);
            }
            let pool = owner.exchange_pool();
            let mut held = Vec::new();
            for (index, connection) in connections.iter().enumerate() {
                for _ in 0..if index == 4 { 12 } else { 13 } {
                    let (mut send, receive) = connection.open_bi().await.unwrap();
                    send.write_all(&4096_u32.to_be_bytes()).await.unwrap();
                    send.write_all(&[0xa0]).await.unwrap();
                    held.push((send, receive));
                }
            }
            assert_eq!(held.len(), 64);
            tokio::time::timeout(Duration::from_secs(1), async {
                while pool.available_exchanges() != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("all 64 global exchange slots are held by real incomplete streams");
            let (mut excess_send, mut excess_receive) = connections[4].open_bi().await.unwrap();
            excess_send
                .write_all(&4096_u32.to_be_bytes())
                .await
                .unwrap();
            let mut byte = [0];
            let refused =
                tokio::time::timeout(Duration::from_millis(500), excess_receive.read(&mut byte))
                    .await
                    .expect("over-capacity stream is reset without a frame-timeout wait");
            assert!(matches!(refused, Err(quinn::ReadError::Reset(_))));
            assert_eq!(pool.available_exchanges(), 0);
            tokio::time::timeout(Duration::from_secs(1), async {
                owner
                    .set_membership_lock(Generation(0), formation.clone(), true)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert!(owner.view().unwrap().summary.membership_locked);
            })
            .await
            .expect("control remains available at global peer saturation");
            connections[0].close(0_u32.into(), b"release one pressure peer");
            tokio::time::timeout(Duration::from_secs(1), async {
                while pool.available_exchanges() != 13 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("disconnect releases only that peer's thirteen slots");
            // A new incomplete frame on a surviving session must acquire a
            // recovered slot, not inherit a permanently exhausted semaphore.
            let (mut recovered_send, recovered_receive) = connections[4].open_bi().await.unwrap();
            recovered_send
                .write_all(&4096_u32.to_be_bytes())
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                while pool.available_exchanges() != 12 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("surviving connection can use recovered capacity");
            held.push((recovered_send, recovered_receive));
            tokio::time::timeout(Duration::from_secs(2), async {
                owner.shutdown().await.unwrap();
                assert_eq!(owner_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                for connection in connections {
                    connection.closed().await;
                }
                assert_eq!(pool.available_exchanges(), 64);
            })
            .await
            .expect("shutdown releases all global stream capacity");
            drop(held);
            drop(clients);
        })
        .await
        .expect("global exchange-pressure fixture stays bounded");
    }

    #[tokio::test]
    async fn incomplete_streams_and_datagram_pressure_preserve_control_and_shutdown() {
        exercise_datagram_delivery(2).await;
    }

    async fn exercise_datagram_delivery(case: u8) {
        use crate::peer::{server, wire};
        use orishu_membership::{Incarnation, OutboundBody, OutboundMessage, PeerBody, ProbeId};
        tokio::time::timeout(Duration::from_secs(5), async {
            let identity = PeerIdentity::generate().unwrap();
            let peer_identity = PeerIdentity::generate().unwrap();
            let mut receiver = model(&identity, "datagram-receiver");
            let initial = model(&peer_identity, "datagram-sender");
            let mut sender = Membership::standalone(
                receiver.formation().clone(),
                receiver.cluster_name().clone(),
                initial.local().clone(),
                AdmissionPolicy::default(),
                Limits::default(),
            )
            .unwrap();
            testing::insert_member(&mut receiver, sender.members()[sender.local_id()].clone());
            testing::insert_member(&mut sender, receiver.members()[receiver.local_id()].clone());
            let receiver_id = receiver.local_id().clone();
            let (owner, owner_task) = crate::formation_metrics::test_owner(receiver);
            let endpoint = quinn::Endpoint::server(
                identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let address = endpoint.local_addr().unwrap();
            let dispatcher = server::spawn(endpoint, owner.clone());
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let connection = client
                .connect_with(
                    peer_identity.client_config(identity.certificate()).unwrap(),
                    address,
                    SERVER_NAME,
                )
                .unwrap()
                .await
                .unwrap();
            let reply = ExchangePool::new(1)
                .unwrap()
                .request(
                    &connection,
                    &handshake::request(sender.local(), sender.formation().clone(), false).unwrap(),
                    Phase::Handshake,
                )
                .await
                .unwrap();
            let mut binding = AuthenticatedSession::from_connection(
                &connection,
                SessionId(1),
                Generation(0),
                sender.formation().clone(),
            )
            .unwrap();
            handshake::accept_reply(
                &reply,
                &mut binding,
                &sender,
                Generation(0),
                sender.formation().clone(),
                identity.fingerprint(),
            )
            .unwrap();
            if case == 2 {
                exercise_peer_pressure(&owner, &connection, &sender).await;
                assert_eq!(owner_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                return;
            }
            if case == 1 {
                exercise_outstanding_acks(&owner, &connection, &binding, &sender).await;
                owner.shutdown().await.unwrap();
                assert_eq!(owner_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                connection.closed().await;
                return;
            }
            let send_ping = |seq, probe| {
                let packet = wire::encode(
                    OutboundMessage {
                        seq,
                        gossip: vec![],
                        body: OutboundBody::Ping {
                            probe: ProbeId(probe),
                            incarnation: Incarnation(0),
                        },
                    },
                    sender.formation().clone(),
                    SenderIdentity::Admitted(sender.local_id().clone()),
                    None,
                )
                .unwrap();
                assert_eq!(packet.transport, wire::Transport::Datagram);
                connection.send_datagram(packet.bytes.into()).unwrap();
            };
            // Deliberately deliver a lower, previously unseen sequence after a
            // higher one. Each real reply must echo its own probe, not arrival order.
            for (seq, expected) in [(100, 700), (99, 701), (100, 700), (1, 703)] {
                send_ping(seq, expected);
                tokio::time::timeout(Duration::from_secs(1), async {
                    loop {
                        let packet = connection.read_datagram().await.unwrap();
                        let decoded = binding
                            .decode(&packet, &sender, Generation(0), wire::Transport::Datagram)
                            .unwrap();
                        if let PeerBody::Ack { probe, .. } = decoded.input.body {
                            assert_eq!(probe, ProbeId(expected));
                            assert_eq!(
                                decoded.input.context.sender,
                                SenderIdentity::Admitted(receiver_id.clone())
                            );
                            break;
                        }
                    }
                })
                .await
                .expect("reordered unseen datagram receives its correlated ACK");
            }
            // Sequence replay is permitted for datagrams, but an unsolicited
            // ACK cannot adopt a higher incarnation without probe correlation.
            let before = owner.model_snapshot().await;
            let unsolicited = wire::encode(
                OutboundMessage {
                    seq: 100,
                    gossip: vec![],
                    body: OutboundBody::Ack {
                        probe: ProbeId(999),
                        incarnation: Incarnation(999),
                    },
                },
                sender.formation().clone(),
                SenderIdentity::Admitted(sender.local_id().clone()),
                None,
            )
            .unwrap();
            connection.send_datagram(unsolicited.bytes.into()).unwrap();
            send_ping(101, 702);
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    let packet = connection.read_datagram().await.unwrap();
                    let decoded = binding
                        .decode(&packet, &sender, Generation(0), wire::Transport::Datagram)
                        .unwrap();
                    if let PeerBody::Ack { probe, .. } = decoded.input.body {
                        assert_eq!(probe, ProbeId(702), "only the fresh Ping gets a reply");
                        break;
                    }
                }
            })
            .await
            .expect("fresh probe progresses after replay");
            let quiet = tokio::time::timeout(Duration::from_millis(150), async {
                loop {
                    let packet = connection.read_datagram().await.unwrap();
                    let decoded = binding
                        .decode(&packet, &sender, Generation(0), wire::Transport::Datagram)
                        .unwrap();
                    if let PeerBody::Ack { probe, .. } = decoded.input.body {
                        panic!("unexpected duplicate ACK {probe:?}");
                    }
                }
            })
            .await;
            assert!(quiet.is_err());
            let view = owner.view().unwrap();
            assert_eq!(view.summary.member_count, 2);
            assert_eq!(view.summary.alive_count, 2);
            assert_eq!(
                owner.counters().traffic,
                crate::driver::TrafficCounters {
                    swim: 6,
                    anti_entropy: 0,
                    gossip_items: 0,
                },
                "repeated and unsolicited packets count as traffic, not successful probes"
            );
            let after = owner.model_snapshot().await;
            assert_eq!(
                after.members()[sender.local_id()].incarnation,
                before.members()[sender.local_id()].incarnation
            );
            owner.shutdown().await.unwrap();
            assert_eq!(owner_task.await.unwrap(), Ok(()));
            dispatcher.await.unwrap();
            connection.closed().await;
        })
        .await
        .expect("datagram replay fixture stays bounded");
    }

    async fn exercise_peer_pressure(
        owner: &crate::driver::Handle,
        connection: &quinn::Connection,
        sender: &Membership,
    ) {
        use crate::peer::wire;
        use orishu_membership::{Incarnation, OutboundBody, OutboundMessage, ProbeId};
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };
        let pool = owner.exchange_pool();
        let mut incomplete = Vec::new();
        // Fill 16-stream transport credit, including unreclaimed handshake
        // credit. Missing bodies and FIN keep every data stream incomplete.
        for index in 0..16 {
            let opened =
                tokio::time::timeout(Duration::from_millis(100), connection.open_bi()).await;
            let Ok(opened) = opened else {
                assert_eq!(index, 15, "credit exhausted before expected bound");
                break;
            };
            let (mut send, receive) = opened.unwrap();
            send.write_all(&4096_u32.to_be_bytes()).await.unwrap();
            send.write_all(&[0xa0]).await.unwrap();
            incomplete.push((send, receive));
        }
        let remaining = 64 - incomplete.len();
        tokio::time::timeout(Duration::from_secs(1), async {
            while pool.available_exchanges() > remaining {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("incomplete frames hold worker exchange permits");
        assert!(
            tokio::time::timeout(Duration::from_millis(100), connection.open_bi())
                .await
                .is_err(),
            "another stream cannot bypass transport credit"
        );
        let baseline = owner.view().unwrap().transitions;
        let sent = Arc::new(AtomicU64::new(0));
        let count = sent.clone();
        let flood_connection = connection.clone();
        let formation = sender.formation().clone();
        let node = sender.local_id().clone();
        let flood = tokio::spawn(async move {
            let work = async {
                for seq in 1000..5096 {
                    let packet = wire::encode(
                        OutboundMessage {
                            seq,
                            gossip: vec![],
                            body: OutboundBody::Ping {
                                probe: ProbeId(seq),
                                incarnation: Incarnation(0),
                            },
                        },
                        formation.clone(),
                        SenderIdentity::Admitted(node.clone()),
                        None,
                    )
                    .unwrap();
                    if flood_connection.send_datagram(packet.bytes.into()).is_err() {
                        return;
                    }
                    count.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            };
            tokio::select! {
                _ = flood_connection.closed() => {},
                _ = work => panic!("pressure exhausted before shutdown"),
            }
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while sent.load(Ordering::Relaxed) < 32
                || owner.view().unwrap().transitions < baseline + 16
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual peer datagrams reach owner while streams stay incomplete");
        assert!(pool.available_exchanges() <= remaining);
        let before_control = sent.load(Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(1), async {
            for locked in [true, false] {
                owner
                    .set_membership_lock(Generation(0), sender.formation().clone(), locked)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(owner.view().unwrap().summary.membership_locked, locked);
            }
            while sent.load(Ordering::Relaxed) == before_control {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("operator control progresses during combined peer pressure");
        assert!(pool.available_exchanges() <= remaining);
        tokio::time::timeout(Duration::from_secs(2), async {
            owner.shutdown().await.unwrap();
            connection.closed().await;
            flood.await.unwrap();
            while pool.available_exchanges() != 64 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown closes pressure connection and releases every exchange");
        drop(incomplete);
    }

    async fn exercise_outstanding_acks(
        owner: &crate::driver::Handle,
        connection: &quinn::Connection,
        binding: &AuthenticatedSession,
        sender: &Membership,
    ) {
        use crate::peer::wire;
        use orishu_membership::{Incarnation, OutboundBody, OutboundMessage, PeerBody, ProbeId};
        let send_ack = |seq, probe, incarnation| {
            let packet = wire::encode(
                OutboundMessage {
                    seq,
                    gossip: vec![],
                    body: OutboundBody::Ack {
                        probe,
                        incarnation: Incarnation(incarnation),
                    },
                },
                sender.formation().clone(),
                SenderIdentity::Admitted(sender.local_id().clone()),
                None,
            )
            .unwrap();
            connection.send_datagram(packet.bytes.into()).unwrap();
        };
        let mut previous = None;
        for round in 1..=2 {
            let probe = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let packet = connection.read_datagram().await.unwrap();
                    let decoded = binding
                        .decode(&packet, sender, Generation(0), wire::Transport::Datagram)
                        .unwrap();
                    if let PeerBody::Ping { probe, .. } = decoded.input.body {
                        break probe;
                    }
                }
            })
            .await
            .expect("real owner emits a scheduled probe");
            let before = owner.model_snapshot().await;
            assert!(before.probes().contains_key(&probe));
            #[cfg(feature = "observability")]
            let deadlines_before = owner.formation_counters().unwrap();
            let incarnation = before.members()[sender.local_id()].incarnation;
            let diagnostics = owner.view().unwrap().diagnostics;
            // First an unknown ID; next round a replay of the completed ID.
            // Neither may consume the real outstanding probe or adopt 999.
            send_ack(100, previous.unwrap_or(ProbeId(999_999)), 999);
            tokio::time::timeout(Duration::from_millis(200), async {
                while owner.view().unwrap().diagnostics == diagnostics {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("owner processes the uncorrelated ACK");
            let rejected = owner.model_snapshot().await;
            assert!(rejected.probes().contains_key(&probe));
            assert_eq!(
                rejected.members()[sender.local_id()].incarnation,
                incarnation
            );
            // Lower envelope sequences remain valid for datagrams. The probe ID,
            // not the last sequence seen, identifies the acknowledgement.
            send_ack(10 - round, probe, round);
            tokio::time::timeout(Duration::from_millis(200), async {
                loop {
                    let current = owner.model_snapshot().await;
                    if !current.probes().contains_key(&probe) {
                        assert_eq!(
                            current.members()[sender.local_id()].incarnation,
                            Incarnation(round)
                        );
                        assert_eq!(current.alive_count(), 2);
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("matching ACK completes probe and records its incarnation");
            #[cfg(feature = "observability")]
            {
                use crate::formation_metrics::Event;
                let counters = owner.formation_counters().unwrap();
                for event in [
                    Event::DirectProbeDeadline,
                    Event::IndirectProbeDeadline,
                    Event::SuspicionDeadline,
                ] {
                    assert_eq!(
                        counters.get(event),
                        deadlines_before.get(event),
                        "answered probe must not expire; earlier pre-session rounds remain counted"
                    );
                }
            }
            previous = Some(probe);
        }
    }

    #[tokio::test]
    async fn concurrent_authenticated_joins_share_one_local_capacity_slot() {
        use crate::peer::{server, wire};
        use orishu_membership::{Effect, JoinReply, OutboundBody, PeerBody, RejectReason};
        use std::sync::Arc;

        tokio::time::timeout(Duration::from_secs(15), async {
            let identity = PeerIdentity::generate().unwrap();
            let initial = model(&identity, "capacity-server");
            let initial = Membership::standalone(
                initial.formation().clone(),
                initial.cluster_name().clone(),
                initial.local().clone(),
                AdmissionPolicy {
                    capacity: 2,
                    ..Default::default()
                },
                Limits::default(),
            )
            .unwrap();
            let target = initial.formation().clone();
            let local_id = initial.local_id().clone();
            let token = crate::credentials::SecretToken::generate().unwrap();
            let expected =
                crate::credentials::SecretToken::parse(token.expose().to_owned()).unwrap();
            let (handle, owner) =
                crate::driver::spawn_standalone_with_join_token(initial, expected);
            let endpoint = quinn::Endpoint::server(
                identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let address = endpoint.local_addr().unwrap();
            let dispatcher = server::spawn(endpoint, handle.clone());
            let barrier = Arc::new(tokio::sync::Barrier::new(3));
            let mut requests = tokio::task::JoinSet::new();
            let mut clients = Vec::new();
            let mut connections = Vec::new();
            for name in ["capacity-a", "capacity-b"] {
                let applicant = PeerIdentity::generate().unwrap();
                let local = model(&applicant, name);
                // These clients have no peer listener. Do not inherit the
                // generic fixture's introducer role and synthetic redirect IP.
                let mut applicant_local = local.local().clone();
                applicant_local.accepts.peers = false;
                applicant_local.endpoints.peers.clear();
                let local = Membership::standalone(
                    local.formation().clone(),
                    local.cluster_name().clone(),
                    applicant_local,
                    AdmissionPolicy::default(),
                    Limits::default(),
                )
                .unwrap();
                let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
                let connection = client
                    .connect_with(
                        applicant.client_config(identity.certificate()).unwrap(),
                        address,
                        SERVER_NAME,
                    )
                    .unwrap()
                    .await
                    .unwrap();
                let pool = ExchangePool::new(1).unwrap();
                let reply = pool
                    .request(
                        &connection,
                        &handshake::request(local.local(), target.clone(), true).unwrap(),
                        Phase::Handshake,
                    )
                    .await
                    .unwrap();
                let mut binding = AuthenticatedSession::from_connection(
                    &connection,
                    SessionId(1),
                    Generation(0),
                    local.formation().clone(),
                )
                .unwrap();
                handshake::accept_reply(
                    &reply,
                    &mut binding,
                    &local,
                    Generation(0),
                    target.clone(),
                    identity.fingerprint(),
                )
                .unwrap();
                let attempt = orishu_membership::update(
                    local.clone(),
                    Message::Local(Command::BeginJoin {
                        session: SessionId(1),
                        target_formation: target.clone(),
                    }),
                );
                let message = attempt
                    .effects
                    .into_iter()
                    .find_map(|effect| match effect {
                        Effect::Send { message, .. }
                            if matches!(message.body, OutboundBody::JoinRequest { .. }) =>
                        {
                            Some(message)
                        }
                        _ => None,
                    })
                    .unwrap();
                let packet = wire::encode_join(
                    message,
                    target.clone(),
                    SenderIdentity::Applicant(local.local_name().clone()),
                    &token,
                    local.local_id(),
                )
                .unwrap();
                clients.push(client);
                connections.push(connection.clone());
                let release = barrier.clone();
                requests.spawn(async move {
                    // Neither applicant emits JoinReq until both authenticated
                    // sessions exist. The production dispatcher decides ordering.
                    release.wait().await;
                    let reply = pool
                        .request(&connection, &packet.bytes, Phase::Membership)
                        .await
                        .unwrap();
                    let decoded = binding
                        .decode(&reply, &local, Generation(0), wire::Transport::Stream)
                        .unwrap();
                    match decoded.input.body.clone() {
                        PeerBody::JoinReply(JoinReply::Accepted { .. }) => {
                            let adopted = orishu_membership::update(
                                attempt.model,
                                Message::Peer(decoded.input),
                            )
                            .model;
                            assert_eq!(adopted.members().len(), 2);
                            Some((adopted.local_id().clone(), applicant.fingerprint()))
                        }
                        PeerBody::JoinReply(JoinReply::Rejected {
                            reason: RejectReason::CapacityExhausted,
                        }) => None,
                        other => panic!("unexpected admission outcome: {other:?}"),
                    }
                });
            }
            assert_eq!(handle.view().unwrap().summary.member_count, 1);
            barrier.wait().await;
            let mut accepted = Vec::new();
            while let Some(request) = requests.join_next().await {
                if let Some(binding) = request.unwrap() {
                    accepted.push(binding);
                }
            }
            assert_eq!(accepted.len(), 1, "one acceptance and one capacity refusal");
            let current = handle.model_snapshot().await;
            assert_eq!(current.formation(), &target);
            assert_eq!(current.members().len(), 2);
            assert!(current.members().contains_key(&local_id));
            let (assigned, fingerprint) = &accepted[0];
            assert_ne!(assigned, &local_id);
            assert_eq!(&current.members()[assigned].cert_fingerprint, fingerprint);
            assert!(current.tombstones().is_empty());
            tokio::time::timeout(Duration::from_secs(2), async {
                handle
                    .set_membership_lock(Generation(0), target, true)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                assert!(handle.view().unwrap().summary.membership_locked);
                handle.shutdown().await.unwrap();
                assert_eq!(owner.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                for connection in connections {
                    connection.closed().await;
                }
            })
            .await
            .expect("control and shutdown remain responsive after concurrent admission");
            drop(clients);
        })
        .await
        .expect("concurrent admission fixture stays bounded");
    }

    #[tokio::test]
    async fn real_join_packet_drives_owner_lock_token_and_admission_outcomes() {
        exercise_join_outcomes(0..9).await;
    }

    #[tokio::test]
    async fn queued_policy_changes_order_admission_after_authentication() {
        tokio::time::timeout(Duration::from_secs(10), exercise_join_outcomes(9..12))
            .await
            .expect("queued policy/admission cases and owner shutdown stay bounded");
    }

    async fn exercise_join_outcomes(cases: std::ops::Range<usize>) {
        use crate::peer::wire::{self, Transport};
        use orishu_membership::{Effect, JoinReply, OutboundBody, PeerBody, RejectReason};
        for case in cases {
            let server_identity = PeerIdentity::generate().unwrap();
            let client_identity = PeerIdentity::generate().unwrap();
            let server_model = orishu_membership::update(
                model(&server_identity, "server"),
                Message::Local(Command::SetMembershipLock(matches!(case, 0 | 10))),
            )
            .model;
            let client_model = model(&client_identity, "client");
            let server = quinn::Endpoint::server(
                server_identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let token = crate::credentials::SecretToken::generate().unwrap();
            let mut local = server_model.local().clone();
            local.endpoints.peers = vec![orishu_membership::Address(
                server.local_addr().unwrap().to_string(),
            )];
            let server_model = orishu_membership::update(
                Membership::standalone(
                    server_model.formation().clone(),
                    server_model.cluster_name().clone(),
                    local,
                    server_model.policy().clone(),
                    server_model.limits().clone(),
                )
                .unwrap(),
                Message::Local(Command::SetMembershipLock(matches!(case, 0 | 10))),
            )
            .model;
            let mut server_model = server_model;
            if case == 5 {
                let mut existing = testing::member("existing-assignment", 1);
                existing.cert_fingerprint = client_identity.fingerprint();
                testing::insert_member(&mut server_model, existing);
            }
            if case == 2 {
                for index in 0..40 {
                    server_model = orishu_membership::update(
                        server_model,
                        Message::Local(Command::UpdateBlocklist(testing::blocklist_entry(
                            &format!("baseline-block-{index:02}"),
                        ))),
                    )
                    .model;
                }
            }
            let expected = if case == 1 {
                crate::credentials::SecretToken::generate().unwrap()
            } else {
                crate::credentials::SecretToken::parse(token.expose().to_owned()).unwrap()
            };
            // Missing formation credentials must refuse admission, not kill
            // the owner (including while target credential catch-up is pending).
            let (handle, owner) = if case == 4 {
                observed_owner(server_model, None)
            } else {
                observed_owner(server_model, Some(expected))
            };
            if case == 3 {
                handle
                    .try_submit(
                        Generation(0),
                        Message::Local(Command::Leave {
                            replacement_formation: "replacement".parse().unwrap(),
                            replacement_node_id: "replacement-node".parse().unwrap(),
                            replacement_cluster_name: "replacement-label".parse().unwrap(),
                        }),
                    )
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(2), async {
                    while handle.view().unwrap().generation == Generation(0) {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
            }
            let current = handle.view().unwrap();
            let generation = current.generation;
            let target = current.summary.formation_id;
            let (outgoing, incoming) =
                connect(&server, &client, &server_identity, &client_identity).await;
            let accepted = handle
                .accept_peer_handshake(
                    generation,
                    incoming.clone(),
                    handshake::request(client_model.local(), target.clone(), true).unwrap(),
                )
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            let mut binding = AuthenticatedSession::from_connection(
                &outgoing,
                SessionId(1),
                Generation(0),
                client_model.formation().clone(),
            )
            .unwrap();
            owner_registry_pressure(&handle, 1, 1, true);
            handshake::accept_reply(
                &accepted.bytes,
                &mut binding,
                &client_model,
                Generation(0),
                target.clone(),
                server_identity.fingerprint(),
            )
            .unwrap();
            let attempt = orishu_membership::update(
                client_model.clone(),
                Message::Local(Command::BeginJoin {
                    session: SessionId(1),
                    target_formation: target.clone(),
                }),
            );
            let message = attempt
                .effects
                .into_iter()
                .find_map(|effect| match effect {
                    Effect::Send { message, .. }
                        if matches!(message.body, OutboundBody::JoinRequest { .. }) =>
                    {
                        Some(message)
                    }
                    _ => None,
                })
                .unwrap();
            let encoded = wire::encode_join(
                message,
                target.clone(),
                SenderIdentity::Applicant(client_model.local_name().clone()),
                &token,
                client_model.local_id(),
            )
            .unwrap();
            let pool = ExchangePool::new(4).unwrap();
            let mut adopted_model = None;
            let exchange = async {
                let sending = pool.request(&outgoing, &encoded.bytes, Phase::Membership);
                let receiving = async {
                    let (send, receive) = incoming.accept_bi().await.unwrap();
                    pool.serve(send, receive, Phase::Membership, |bytes| async {
                        // Both inputs are queued without yielding. The real
                        // owner chooses its control lane before this peer input;
                        // authentication happened before the policy change.
                        let policy = matches!(case, 9 | 10).then(|| {
                            handle
                                .set_membership_lock(generation, target.clone(), case == 9)
                                .unwrap()
                        });
                        let response = handle
                            .receive_peer_packet(
                                generation,
                                accepted.session,
                                Transport::Stream,
                                bytes,
                            )
                            .unwrap();
                        if let Some(policy) = policy {
                            policy.await.unwrap().unwrap();
                        }
                        let response = response
                            .await
                            .unwrap()
                            .unwrap()
                            .expect("core emitted admission outcome");
                        if case == 11 {
                            // A later lock cannot retroactively undo admission.
                            handle
                                .set_membership_lock(generation, target.clone(), true)
                                .unwrap()
                                .await
                                .unwrap()
                                .unwrap();
                        }
                        if case >= 9 {
                            let view = handle.view().unwrap();
                            assert_eq!(view.generation, generation);
                            assert_eq!(view.summary.membership_locked, case != 10);
                        }
                        assert_eq!(response.transport, Transport::Stream);
                        Ok(response.bytes)
                    })
                    .await
                    .unwrap();
                };
                let (response, ()) = tokio::join!(sending, receiving);
                let decoded = binding
                    .decode(
                        &response.unwrap(),
                        &client_model,
                        Generation(0),
                        Transport::Stream,
                    )
                    .unwrap();
                match case {
                    0 | 9 => assert!(matches!(
                        decoded.input.body,
                        PeerBody::JoinReply(JoinReply::Rejected {
                            reason: RejectReason::MembershipLocked
                        })
                    )),
                    5 => assert!(matches!(
                        decoded.input.body,
                        PeerBody::JoinReply(JoinReply::Rejected {
                            reason: RejectReason::AlreadyAdmitted
                        })
                    )),
                    1 | 3 | 4 => assert!(matches!(
                        decoded.input.body,
                        PeerBody::JoinReply(JoinReply::Rejected {
                            reason: RejectReason::InvalidToken
                        })
                    )),
                    _ => {
                        assert!(matches!(
                            &decoded.input.body,
                            PeerBody::JoinReply(JoinReply::Accepted { .. })
                        ));
                        let adopted =
                            orishu_membership::update(attempt.model, Message::Peer(decoded.input))
                                .model;
                        assert_eq!(adopted.formation(), &target);
                        assert_eq!(adopted.members().len(), 2);
                        assert_eq!(
                            adopted.local().cert_fingerprint,
                            client_identity.fingerprint()
                        );
                        adopted_model = Some(adopted);
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(5), exchange)
                .await
                .unwrap();
            let mut member_count = if matches!(case, 2 | 5..=8 | 10 | 11) {
                2
            } else {
                1
            };
            assert_eq!(handle.view().unwrap().summary.member_count, member_count);
            owner_registry_pressure(
                &handle,
                1,
                if adopted_model.is_some() { 0 } else { 1 },
                true,
            );
            let admitted = matches!(case, 2 | 6..=8 | 10 | 11);
            assert_eq!(
                handle.counters().admissions,
                crate::driver::AdmissionCounters {
                    accepted: u64::from(admitted),
                    rejected: u64::from(!admitted),
                    assignment_replays: 0,
                },
                "issuer counts actual insertion/refusal, not seeded membership: case {case}"
            );
            if case >= 9 {
                let current = handle.model_snapshot().await;
                assert_eq!(current.formation(), &target);
                assert!(current.tombstones().is_empty());
                if let Some(adopted) = &adopted_model {
                    assert_eq!(
                        current.members()[adopted.local_id()].cert_fingerprint,
                        client_identity.fingerprint()
                    );
                } else {
                    assert!(current.members().values().all(|member| {
                        member.cert_fingerprint != client_identity.fingerprint()
                    }));
                }
            }
            if matches!(case, 6..=8) {
                member_count = exercise_retired_replay(
                    &handle,
                    (&server, &client),
                    (&server_identity, &client_identity),
                    adopted_model.take().unwrap(),
                    (&encoded.bytes, accepted.session, &incoming),
                    &token,
                    case,
                )
                .await;
                assert_eq!(handle.counters().admissions.assignment_replays, 0);
            }
            if case == 2 {
                // Same live connection is already promoted on the introducer,
                // but a sender that lost its ACK still uses applicant identity.
                handle
                    .set_membership_lock(generation, target.clone(), true)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                let before = handle.view().unwrap().transitions;
                let retried = async {
                    let receiving = async {
                        let (send, receive) = incoming.accept_bi().await.unwrap();
                        pool.serve(send, receive, Phase::Membership, |bytes| async {
                            Ok(handle
                                .receive_peer_packet(
                                    generation,
                                    accepted.session,
                                    Transport::Stream,
                                    bytes,
                                )
                                .unwrap()
                                .await
                                .unwrap()
                                .unwrap()
                                .unwrap()
                                .bytes)
                        })
                        .await
                        .unwrap();
                    };
                    let (reply, ()) = tokio::join!(
                        pool.request(&outgoing, &encoded.bytes, Phase::Membership),
                        receiving
                    );
                    let replayed = binding
                        .decode(
                            &reply.unwrap(),
                            &client_model,
                            Generation(0),
                            Transport::Stream,
                        )
                        .unwrap();
                    let PeerBody::JoinReply(JoinReply::Accepted { assigned, .. }) =
                        replayed.input.body
                    else {
                        panic!("exact retry must recover acceptance");
                    };
                    assert_eq!(&assigned, adopted_model.as_ref().unwrap().local_id());
                };
                tokio::time::timeout(Duration::from_secs(2), retried)
                    .await
                    .unwrap();
                let wrong_token = crate::credentials::SecretToken::generate().unwrap();
                for variant in 0..3 {
                    let local = client_model.local();
                    let mut capacity = local.capacity;
                    if variant == 1 {
                        capacity.clients += 1;
                    }
                    let different_attempt: orishu_membership::NodeId =
                        "another-attempt".parse().unwrap();
                    let invalid = wire::encode_join(
                        orishu_membership::OutboundMessage {
                            seq: 999,
                            gossip: vec![],
                            body: OutboundBody::JoinRequest {
                                name: local.worker_name.clone(),
                                cert_fingerprint: local.cert_fingerprint,
                                protocol: local.protocol,
                                endpoints: local.endpoints.clone(),
                                accepts: local.accepts,
                                capacity,
                                capabilities: local.capabilities.clone(),
                            },
                        },
                        target.clone(),
                        SenderIdentity::Applicant(local.worker_name.clone()),
                        if variant == 0 { &wrong_token } else { &token },
                        if variant == 2 {
                            &different_attempt
                        } else {
                            client_model.local_id()
                        },
                    )
                    .unwrap();
                    let refused = async {
                        let serving = async {
                            let (send, receive) = incoming.accept_bi().await.unwrap();
                            assert!(
                                pool.serve(send, receive, Phase::Membership, |bytes| async {
                                    assert!(
                                        handle
                                            .receive_peer_packet(
                                                generation,
                                                accepted.session,
                                                Transport::Stream,
                                                bytes
                                            )
                                            .unwrap()
                                            .await
                                            .unwrap()
                                            .is_err()
                                    );
                                    Err(crate::peer::exchange::ExchangeError::Owner)
                                })
                                .await
                                .is_err()
                            );
                        };
                        let (reply, ()) = tokio::join!(
                            pool.request(&outgoing, &invalid.bytes, Phase::Membership),
                            serving
                        );
                        assert!(
                            reply.is_err(),
                            "wrong token, changed request or fresh attempt on an admitted session must refuse"
                        );
                    };
                    tokio::time::timeout(Duration::from_secs(2), refused)
                        .await
                        .unwrap();
                }
                assert_eq!(
                    handle.view().unwrap().transitions,
                    before,
                    "replay does not re-enter the core"
                );
                assert_eq!(handle.view().unwrap().summary.member_count, 2);
                assert_eq!(
                    handle.counters().admissions,
                    crate::driver::AdmissionCounters {
                        accepted: 1,
                        rejected: 0,
                        assignment_replays: 1,
                    },
                    "valid replay publishes once; invalid replays never enter the core"
                );
                assert!(handle.view().unwrap().summary.membership_locked);
                handle
                    .set_membership_lock(generation, target.clone(), false)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
            }
            for (session, bytes) in [
                (SessionId(999), encoded.bytes),
                (accepted.session, vec![0xa0]),
            ] {
                let before = handle.counters().admissions;
                assert!(
                    handle
                        .receive_peer_packet(Generation(0), session, Transport::Stream, bytes)
                        .unwrap()
                        .await
                        .unwrap()
                        .is_err()
                );
                assert_eq!(handle.counters().admissions, before);
            }
            assert_eq!(handle.view().unwrap().summary.member_count, member_count);
            if let Some(adopted) = adopted_model.filter(|_| case < 9) {
                outgoing.close(0_u32.into(), b"reconnect after adoption");
                tokio::time::timeout(Duration::from_secs(2), incoming.closed())
                    .await
                    .unwrap();
                exercise_member_traffic(
                    &handle,
                    (&server, &client),
                    (&server_identity, &client_identity),
                    adopted,
                    &token,
                )
                .await;
            }
            let retained = handle.counters().admissions;
            let previous = handle.view().unwrap();
            handle
                .try_submit(
                    previous.generation,
                    Message::Local(Command::Leave {
                        replacement_formation: "metrics-replacement".parse().unwrap(),
                        replacement_node_id: "metrics-local".parse().unwrap(),
                        replacement_cluster_name: "metrics-label".parse().unwrap(),
                    }),
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while handle.view().unwrap().generation == previous.generation {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(handle.counters().admissions, retained);
            handle.shutdown().await.unwrap();
            assert_eq!(owner.await.unwrap(), Ok(()));
            owner_registry_pressure(&handle, 0, 0, false);
            assert_eq!(handle.counters().admissions, retained);
            client.close(0_u32.into(), b"done");
            server.close(0_u32.into(), b"done");
        }
    }

    async fn exercise_retired_replay(
        owner: &crate::driver::Handle,
        endpoints: (&quinn::Endpoint, &quinn::Endpoint),
        identities: (&PeerIdentity, &PeerIdentity),
        adopted: Membership,
        original: (&[u8], SessionId, &quinn::Connection),
        token: &crate::credentials::SecretToken,
        case: usize,
    ) -> usize {
        use crate::peer::wire::{self, Transport};
        use orishu_membership::{OutboundBody, OutboundMessage};
        let assigned = adopted.local_id().clone();
        let generation = owner.view().unwrap().generation;
        match case {
            6 => owner
                .try_submit(
                    generation,
                    Message::Local(Command::RemoveMember {
                        node: assigned.clone(),
                        mode: orishu_membership::RemovalMode::Force,
                        reason: None,
                    }),
                )
                .unwrap(),
            7 => {
                let mut block = testing::blocklist_entry("blocked-applicant");
                block.key = BlocklistKey::Fingerprint(identities.1.fingerprint());
                owner
                    .try_submit(generation, Message::Local(Command::UpdateBlocklist(block)))
                    .unwrap();
            }
            8 => {
                // Serialized self-departure enters the ordinary owner codec;
                // only the following reconnect/replay uses new QUIC streams.
                let leave = wire::encode(
                    OutboundMessage {
                        seq: 1_000_000,
                        gossip: vec![],
                        body: OutboundBody::Announce {
                            announcement: orishu_membership::Announcement::Leave,
                            target: assigned.clone(),
                            incarnation: adopted.incarnation(),
                        },
                    },
                    adopted.formation().clone(),
                    SenderIdentity::Admitted(assigned.clone()),
                    None,
                )
                .unwrap();
                owner
                    .receive_peer_packet(generation, original.1, Transport::Datagram, leave.bytes)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
            }
            _ => unreachable!(),
        }
        tokio::time::timeout(Duration::from_secs(2), original.2.closed())
            .await
            .expect("restriction retires the original admitted session");
        // Control requests can overtake the core-input lane, so poll the public
        // projection until the selected retirement/restriction is applied.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let item = owner.inspect(assigned.clone()).unwrap().await.unwrap();
                let ready = match case {
                    6 => item.is_none(),
                    // Session retirement above is the block's observable fence;
                    // blocking does not erase the member's inspection record.
                    7 => item.is_some(),
                    8 => item.is_some_and(|item| {
                        item.liveness == orishu::model::node::MemberState::Dead
                    }),
                    _ => unreachable!(),
                };
                if ready {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("retirement is visible before replay");
        let count = owner.view().unwrap().summary.member_count;
        let (outgoing, incoming) =
            connect(endpoints.0, endpoints.1, identities.0, identities.1).await;
        let pool = ExchangePool::new(4).unwrap();
        let handshake =
            handshake::request(adopted.local(), adopted.formation().clone(), true).unwrap();
        let registered = async {
            let (send, receive) = incoming.accept_bi().await.unwrap();
            let mut session = None;
            pool.serve(send, receive, Phase::Handshake, |bytes| async {
                let reply = owner
                    .accept_peer_handshake(generation, incoming.clone(), bytes)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap();
                session = Some(reply.session);
                Ok(reply.bytes)
            })
            .await
            .unwrap();
            session.unwrap()
        };
        let (reply, session) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                pool.request(&outgoing, &handshake, Phase::Handshake),
                registered
            )
        })
        .await
        .unwrap();
        reply.unwrap();
        // A fresh applicant handshake succeeds, but its old accepted assignment
        // is now barred. The ledger must refuse without re-entering admission.
        let before = owner.view().unwrap().transitions;
        let refused = async {
            let (send, receive) = incoming.accept_bi().await.unwrap();
            assert!(
                pool.serve(send, receive, Phase::Membership, |bytes| async {
                    assert!(
                        owner
                            .receive_peer_packet(generation, session, Transport::Stream, bytes)
                            .unwrap()
                            .await
                            .unwrap()
                            .is_err()
                    );
                    Err(crate::peer::exchange::ExchangeError::Owner)
                })
                .await
                .is_err()
            );
        };
        let (reply, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                pool.request(&outgoing, original.0, Phase::Membership),
                refused
            )
        })
        .await
        .unwrap();
        assert!(reply.is_err());
        assert_eq!(owner.view().unwrap().transitions, before);
        assert_eq!(owner.view().unwrap().summary.member_count, count);
        {
            let local = adopted.local();
            let fresh = wire::encode_join(
                OutboundMessage {
                    seq: 2_000_000,
                    gossip: vec![],
                    body: OutboundBody::JoinRequest {
                        name: local.worker_name.clone(),
                        cert_fingerprint: local.cert_fingerprint,
                        protocol: local.protocol,
                        endpoints: local.endpoints.clone(),
                        accepts: local.accepts,
                        capacity: local.capacity,
                        capabilities: local.capabilities.clone(),
                    },
                },
                adopted.formation().clone(),
                SenderIdentity::Applicant(local.worker_name.clone()),
                token,
                &"fresh-lifecycle-attempt".parse().unwrap(),
            )
            .unwrap();
            let serving = async {
                let (send, receive) = incoming.accept_bi().await.unwrap();
                pool.serve(send, receive, Phase::Membership, |bytes| async {
                    Ok(owner
                        .receive_peer_packet(generation, session, Transport::Stream, bytes)
                        .unwrap()
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap()
                        .bytes)
                })
                .await
                .unwrap();
            };
            let (reply, ()) = tokio::time::timeout(Duration::from_secs(2), async {
                tokio::join!(
                    pool.request(&outgoing, &fresh.bytes, Phase::Membership),
                    serving
                )
            })
            .await
            .unwrap();
            let source = owner.view().unwrap().summary.source_node_id;
            let context = testing::peer_context(&adopted, &source, 1);
            let reply = wire::decode(&reply.unwrap(), &context, Transport::Stream).unwrap();
            use orishu_membership::{JoinReply, PeerBody, RejectReason};
            match (case, reply.input.body) {
                (
                    6,
                    PeerBody::JoinReply(JoinReply::Rejected {
                        reason: RejectReason::Tombstoned,
                    }),
                ) => {}
                (
                    7,
                    PeerBody::JoinReply(JoinReply::Rejected {
                        reason: RejectReason::Blocklisted { key },
                    }),
                ) => {
                    assert_eq!(key, BlocklistKey::Fingerprint(identities.1.fingerprint()));
                }
                (
                    8,
                    PeerBody::JoinReply(JoinReply::Accepted {
                        assigned: replacement,
                        ..
                    }),
                ) => {
                    assert_ne!(
                        replacement, assigned,
                        "fresh admission must not resurrect the retired identity"
                    );
                    assert_eq!(
                        owner
                            .inspect(assigned)
                            .unwrap()
                            .await
                            .unwrap()
                            .unwrap()
                            .liveness,
                        orishu::model::node::MemberState::Dead
                    );
                }
                _ => panic!("fresh attempt must obey current exclusion/dead-history gates"),
            }
            assert_eq!(
                owner.view().unwrap().summary.member_count,
                count + usize::from(case == 8)
            );
        }
        outgoing.close(0_u32.into(), b"retired replay test done");
        owner.view().unwrap().summary.member_count
    }

    async fn exercise_member_traffic(
        a: &crate::driver::Handle,
        endpoints: (&quinn::Endpoint, &quinn::Endpoint),
        identities: (&PeerIdentity, &PeerIdentity),
        adopted: Membership,
        admission_token: &crate::credentials::SecretToken,
    ) {
        // Initialize the second owner from the independently validated adoption
        // above. This tests sustained transport, not a production join command.
        let target = adopted.formation().clone();
        let target_node = a.view().unwrap().summary.source_node_id;
        let route = crate::peer::dial::MemberTarget::from_model(&adopted, &target_node).unwrap();
        assert!(crate::peer::dial::MemberTarget::from_model(&adopted, adopted.local_id()).is_err());
        let pool = ExchangePool::new(4).unwrap();
        let dialer = crate::peer::dial::Dialer::default();
        let (session_send, session_receive) = tokio::sync::oneshot::channel();
        let handshake_exchange = async {
            let send =
                dialer.member_handshake(endpoints.1, identities.1, adopted.local(), &route, &pool);
            let receive = async {
                let incoming = endpoints.0.accept().await.unwrap().await.unwrap();
                let (send, receive) = incoming.accept_bi().await.unwrap();
                pool.serve(send, receive, Phase::Handshake, |bytes| async {
                    let accepted = a
                        .accept_peer_handshake(Generation(0), incoming.clone(), bytes)
                        .unwrap()
                        .await
                        .unwrap()
                        .unwrap();
                    session_send.send(accepted.session).unwrap();
                    Ok(accepted.bytes)
                })
                .await
                .unwrap();
                incoming
            };
            let (pending, incoming) = tokio::join!(send, receive);
            (pending.unwrap(), incoming)
        };
        let (pending, incoming) = tokio::time::timeout(Duration::from_secs(5), handshake_exchange)
            .await
            .unwrap();
        let (outgoing, reply) = pending.into_parts();
        let session_a = session_receive.await.unwrap();
        let (b, owner_b) = crate::driver::spawn_standalone(adopted);
        let session_b = b
            .accept_member_handshake_reply(Generation(0), outgoing.clone(), reply)
            .unwrap()
            .await
            .unwrap()
            .unwrap()
            .session;
        let mut pumps = tokio::task::JoinSet::new();
        let catchup_connection = outgoing.clone();
        let removal_connection = incoming.clone();
        for (handle, session, connection) in [
            (a.clone(), session_a, incoming),
            (b.clone(), session_b, outgoing),
        ] {
            let pool = handle.exchange_pool();
            pumps.spawn(async move {
                crate::peer::server::serve_registered(
                    &connection,
                    &handle,
                    Generation(0),
                    session,
                    &pool,
                )
                .await;
            });
        }
        {
            use crate::peer::catchup::{
                self,
                wire::{Action, Outcome, Rejection, Reply, Request},
            };
            let requester = b.view().unwrap().summary.source_node_id;
            let operation: orishu::model::cluster::OperationId = "wire-baseline".parse().unwrap();
            let exchange = |action| {
                let request =
                    Request::new(target.clone(), requester.clone(), operation.clone(), action);
                let pool = &pool;
                let connection = &catchup_connection;
                let target = &target;
                let target_node = &target_node;
                let operation = &operation;
                async move {
                    let bytes = pool
                        .request(connection, &request.encode().unwrap(), Phase::Membership)
                        .await
                        .unwrap();
                    Reply::decode(&bytes, target, target_node, operation)
                        .unwrap()
                        .outcome
                }
            };
            let Outcome::Baseline { descriptor } = exchange(Action::Begin).await else {
                panic!("baseline response");
            };
            assert!(
                descriptor.pages > 1,
                "real transfer must exercise pagination"
            );
            assert!(matches!(
                exchange(Action::Confirm {
                    snapshot: descriptor.snapshot,
                    root: descriptor.root
                })
                .await,
                Outcome::Rejected {
                    reason: Rejection::Invalid
                }
            ));
            let mut receiver = catchup::Receiver::new(descriptor.clone()).unwrap();
            for index in 0..descriptor.pages {
                let Outcome::Page { page } = exchange(Action::Page {
                    snapshot: descriptor.snapshot,
                    index,
                })
                .await
                else {
                    panic!("page response");
                };
                receiver = receiver
                    .push(&crate::peer::codec::encode(&page).unwrap())
                    .unwrap();
            }
            let baseline = receiver.finish_baseline().unwrap();
            assert_eq!(baseline.formation_id, target);
            let Outcome::Credential {
                snapshot,
                root,
                token,
            } = exchange(Action::Confirm {
                snapshot: descriptor.snapshot,
                root: descriptor.root,
            })
            .await
            else {
                panic!("credential response");
            };
            assert_eq!(snapshot, descriptor.snapshot);
            assert_eq!(root, descriptor.root);
            assert!(admission_token.matches(token.expose()));
            let binding = catchup::client::Binding {
                formation: target.clone(),
                source: target_node.clone(),
                fingerprint: identities.0.fingerprint(),
                requester: requester.clone(),
                request: "receiver-baseline".parse().unwrap(),
            };
            let completed = catchup::client::fetch(&catchup_connection, &binding, &pool)
                .await
                .unwrap();
            let (received, token) = completed.into_parts();
            assert_eq!(received.formation_id, target);
            assert_eq!(received.blocklist.len(), 40);
            assert!(admission_token.matches(token.expose()));
            let wrong_pin = catchup::client::Binding {
                fingerprint: identities.1.fingerprint(),
                ..binding
            };
            assert!(matches!(
                catchup::client::fetch(&catchup_connection, &wrong_pin, &pool).await,
                Err(catchup::client::Error::Binding)
            ));
        }
        a.set_membership_lock(Generation(0), target.clone(), true)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let left = a.view().unwrap();
                let right = b.view().unwrap();
                if right.summary.membership_locked
                    && left.summary.alive_count == 2
                    && right.summary.alive_count == 2
                    && left.completed_exchanges > 0
                    && right.completed_exchanges > 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("real SWIM, reliable exchange and policy convergence");
        for handle in [a, &b] {
            assert!(handle.counters().traffic.swim > 0);
            assert!(handle.counters().traffic.anti_entropy > 0);
        }
        b.set_membership_lock(Generation(0), target.clone(), false)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(4), async {
            while a.view().unwrap().summary.membership_locked {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("unlock converges back to first owner");
        // A valid tombstone from an authenticated member must eject the local
        // worker through ordinary gossip, not only the catch-up resource path.
        let removed = b.view().unwrap().summary.source_node_id;
        let removal = crate::peer::wire::encode(
            orishu_membership::OutboundMessage {
                seq: 1_000_000,
                body: orishu_membership::OutboundBody::Ping {
                    probe: orishu_membership::ProbeId(1_000_000),
                    incarnation: orishu_membership::Incarnation(0),
                },
                gossip: vec![orishu_membership::GossipDelta {
                    hops: 0,
                    body: orishu_membership::DeltaBody::TombstoneUpdate(
                        orishu_membership::MembershipTombstone {
                            node_id: removed.clone(),
                            name: "client".parse().unwrap(),
                            cert_fingerprint: identities.1.fingerprint(),
                            removal_mode: orishu_membership::RemovalMode::Force,
                            version: orishu_membership::testing::version(1_000_000),
                            cleared: false,
                            reason: None,
                        },
                    ),
                }],
            },
            target.clone(),
            orishu_membership::SenderIdentity::Admitted(target_node),
            None,
        )
        .unwrap();
        assert_eq!(removal.deferred_gossip, 0);
        let before_removal = b.counters().traffic.gossip_items;
        removal_connection
            .send_datagram(removal.bytes.into())
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while b.view().unwrap().summary.participation
                != orishu::model::cluster::Participation::Ejected
            {
                tokio::task::yield_now().await;
            }
            catchup_connection.closed().await;
        })
        .await
        .expect("wire self-removal ejects and closes the peer connection");
        let ejected = b.view().unwrap();
        assert!(b.counters().traffic.gossip_items > before_removal);
        assert_eq!(ejected.generation, Generation(1));
        assert_eq!(ejected.summary.formation_id, target);
        assert_eq!(ejected.summary.source_node_id, removed);
        assert!(!ejected.summary.introducer_ready);
        assert!(b.join_material().unwrap().await.unwrap().is_none());
        assert!(b.prepare_catchup().unwrap().await.unwrap().is_none());
        assert!(b.member_dial(None).unwrap().await.unwrap().plan.is_none());
        // Even a completion claiming the new generation cannot restart core
        // activity in an ejected shell. Old-generation work is fenced too.
        for generation in [Generation(0), ejected.generation] {
            b.try_submit(generation, Message::Local(Command::StartProbeRound))
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while b.view().unwrap().stale_inputs < ejected.stale_inputs + 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(b.view().unwrap().transitions, ejected.transitions);
        b.shutdown().await.unwrap();
        assert_eq!(owner_b.await.unwrap(), Ok(()));
        pumps.abort_all();
        while pumps.join_next().await.is_some() {}
    }

    #[tokio::test]
    async fn owner_rechecks_actual_source_after_quic_migration_and_policy_changes() {
        use orishu_membership::model::{BlocklistEntry, NetworkPattern};
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let server_model = model(&server_identity, "server");
        let client_model = model(&client_identity, "client");
        let network = BlocklistKey::Network(NetworkPattern("127.0.0.2/32".into()));
        let server_model = orishu_membership::update(
            server_model,
            Message::Local(Command::UpdateBlocklist(BlocklistEntry {
                key: network.clone(),
                action: BlocklistAction::Block,
                version: testing::version(1),
                added_by: "operator".into(),
            })),
        )
        .model;
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let (handle, owner) = crate::driver::spawn_standalone(server_model);
        let view = handle.view().unwrap();
        let request =
            handshake::request(client_model.local(), view.summary.formation_id, true).unwrap();
        let (outgoing, incoming) =
            connect(&server, &client, &server_identity, &client_identity).await;
        let observed = incoming.clone();
        handle
            .accept_peer_handshake(view.generation, incoming, request.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(observed.remote_address().ip().to_string(), "127.0.0.1");

        // Rebind the real client endpoint to a different loopback IP. QUIC path
        // validation updates the server-side address; no advertised field or
        // synthetic source parameter is used to exercise the registry check.
        client
            .rebind(std::net::UdpSocket::bind("127.0.0.2:0").unwrap())
            .unwrap();
        outgoing.send_datagram(vec![0_u8].into()).unwrap();
        let reason = tokio::time::timeout(Duration::from_secs(4), outgoing.closed())
            .await
            .unwrap();
        assert!(matches!(
            reason,
            quinn::ConnectionError::ApplicationClosed(_)
        ));
        assert_eq!(observed.remote_address().ip().to_string(), "127.0.0.2");

        let (blocked_out, blocked_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        assert!(matches!(
            handle
                .accept_peer_handshake(view.generation, blocked_in, request.clone())
                .unwrap()
                .await
                .unwrap(),
            Err(RegistryError::SourceBlocked)
        ));
        tokio::time::timeout(Duration::from_secs(2), blocked_out.closed())
            .await
            .unwrap();
        handle
            .try_submit(
                view.generation,
                Message::Local(Command::UpdateBlocklist(BlocklistEntry {
                    key: network,
                    action: BlocklistAction::Allow,
                    version: testing::version(2),
                    added_by: "operator".into(),
                })),
            )
            .unwrap();
        let (allowed_out, allowed_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        handle
            .accept_peer_handshake(view.generation, allowed_in, request.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(allowed_out.close_reason().is_none());

        // Opaque policy text can pass core shape validation but must fail closed
        // in the shell. Its error must retire an existing session too.
        handle
            .try_submit(
                view.generation,
                Message::Local(Command::UpdateBlocklist(BlocklistEntry {
                    key: BlocklistKey::Network(NetworkPattern("invalid-network".into())),
                    action: BlocklistAction::Block,
                    version: testing::version(3),
                    added_by: "operator".into(),
                })),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), allowed_out.closed())
            .await
            .unwrap();
        let (invalid_out, invalid_in) =
            connect(&server, &client, &server_identity, &client_identity).await;
        assert!(matches!(
            handle
                .accept_peer_handshake(view.generation, invalid_in, request)
                .unwrap()
                .await
                .unwrap(),
            Err(RegistryError::SourceBlocked)
        ));
        tokio::time::timeout(Duration::from_secs(2), invalid_out.closed())
            .await
            .unwrap();
        handle.shutdown().await.unwrap();
        assert_eq!(owner.await.unwrap(), Ok(()));
        client.close(0_u32.into(), b"done");
        server.close(0_u32.into(), b"done");
    }
}
