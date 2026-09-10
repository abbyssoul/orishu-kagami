//! Authenticated session binding. A TLS certificate proves key possession, not
//! membership: the owner must bind the assigned identity against its current model.

use crate::driver::Generation;
use orishu_membership::{
    CertFingerprint, FormationId, Liveness, Membership, NodeId, PeerContext, ProtocolVersion,
    SenderIdentity, SessionId, WorkerName,
};
use rustls::pki_types::CertificateDer;
use sha2::{Digest, Sha256};

/// Secret-free reasons a connection cannot carry membership input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// TLS identity/profile was not established by the expected transport.
    #[error("peer transport identity is unavailable or incompatible")]
    Unauthenticated,
    /// The owner abandoned this connection's formation/generation.
    #[error("peer session belongs to a stale formation generation")]
    Stale,
    /// No matching live member/certificate binding exists.
    #[error("peer session does not match an admitted identity")]
    Identity,
    /// Handshake binding has not completed.
    #[error("peer session identity has not been bound")]
    Unbound,
}

/// A connection's authenticated facts, never reconstructed from envelope claims.
/// The final peer connection adapter owns its lifetime and closes rejected sessions.
pub struct AuthenticatedSession {
    id: SessionId,
    generation: Generation,
    formation: FormationId,
    fingerprint: CertFingerprint,
    sender: Option<SenderIdentity>,
    introducer_target: Option<FormationId>,
}

impl AuthenticatedSession {
    /// Extract facts only after the QUIC handshake completed. Application claims
    /// cannot provide the certificate or negotiate the profile on its behalf.
    pub fn from_connection(
        connection: &quinn::Connection,
        id: SessionId,
        generation: Generation,
        formation: FormationId,
    ) -> Result<Self, SessionError> {
        let handshake = connection
            .handshake_data()
            .ok_or(SessionError::Unauthenticated)?
            .downcast::<quinn::crypto::rustls::HandshakeData>()
            .map_err(|_| SessionError::Unauthenticated)?;
        if handshake.protocol.as_deref() != Some(super::tls::ALPN) {
            return Err(SessionError::Unauthenticated);
        }
        let certificates = connection
            .peer_identity()
            .ok_or(SessionError::Unauthenticated)?
            .downcast::<Vec<CertificateDer<'static>>>()
            .map_err(|_| SessionError::Unauthenticated)?;
        if certificates.len() != 1 || certificates[0].len() > 16_384 {
            return Err(SessionError::Unauthenticated);
        }
        Ok(Self {
            id,
            generation,
            formation,
            fingerprint: CertFingerprint::from_bytes(
                Sha256::digest(certificates[0].as_ref()).into(),
            ),
            sender: None,
            introducer_target: None,
        })
    }

    /// Bind an outbound join's introducer using operator-authenticated trust
    /// material, before the joiner has any members in the target formation.
    /// This role permits only a JoinReply; it is not an admitted-member binding.
    pub fn bind_introducer(
        &mut self,
        model: &Membership,
        generation: Generation,
        target: FormationId,
        node: NodeId,
        trusted_fingerprint: CertFingerprint,
    ) -> Result<(), SessionError> {
        self.check_generation(model, generation)?;
        if self.sender.is_some()
            || self.fingerprint != trusted_fingerprint
            || &target == model.formation()
        {
            return Err(SessionError::Identity);
        }
        self.sender = Some(SenderIdentity::Admitted(node));
        self.introducer_target = Some(target);
        Ok(())
    }

    /// Bind a provisional applicant label. This grants only the wire adapter's
    /// JoinReq path; it does not grant post-admission traffic or a node identity.
    pub fn bind_applicant(&mut self, name: WorkerName) -> Result<(), SessionError> {
        let sender = SenderIdentity::Applicant(name);
        if self
            .sender
            .as_ref()
            .is_some_and(|current| current != &sender)
        {
            return Err(SessionError::Identity);
        }
        self.sender = Some(sender);
        Ok(())
    }

    /// Bind or upgrade only to an identity already held by membership. A label
    /// that happens to equal a node ID is never evidence of this binding.
    pub fn bind_member(
        &mut self,
        model: &Membership,
        generation: Generation,
        node: NodeId,
    ) -> Result<(), SessionError> {
        self.check_generation(model, generation)?;
        if self.introducer_target.is_some() {
            return Err(SessionError::Identity);
        }
        self.check_member(model, &node)?;
        if let Some(sender) = &self.sender {
            match sender {
                SenderIdentity::Admitted(current) if current != &node => {
                    return Err(SessionError::Identity);
                }
                SenderIdentity::Applicant(name)
                    if name != &model.member(&node).ok_or(SessionError::Identity)?.name =>
                {
                    return Err(SessionError::Identity);
                }
                _ => {}
            }
        }
        self.sender = Some(SenderIdentity::Admitted(node));
        Ok(())
    }

    /// Revalidate against the current owner before each message, so a cached
    /// binding cannot keep using an identity after removal, death or formation exit.
    /// The wire decoder supplies seq/gossip only after checking the envelope.
    pub(crate) fn context(
        &self,
        model: &Membership,
        generation: Generation,
    ) -> Result<PeerContext, SessionError> {
        self.check_generation(model, generation)?;
        let sender = self.sender.clone().ok_or(SessionError::Unbound)?;
        if let SenderIdentity::Admitted(node) = &sender
            && self.introducer_target.is_none()
        {
            self.check_member(model, node)?;
        }
        Ok(PeerContext {
            session: self.id,
            authenticated: true,
            cert_fingerprint: self.fingerprint,
            sender,
            formation: self
                .introducer_target
                .as_ref()
                .unwrap_or(&self.formation)
                .clone(),
            protocol: ProtocolVersion::CURRENT,
            seq: 0,
            gossip: vec![],
        })
    }

    /// Decode through both the current membership binding and role gate. In
    /// particular, a pinned introducer cannot use its pre-admission connection
    /// for arbitrary admitted traffic while the joiner still owns another formation.
    pub fn decode(
        &self,
        bytes: &[u8],
        model: &Membership,
        generation: Generation,
        transport: super::wire::Transport,
    ) -> Result<super::wire::Decoded, super::wire::WireError> {
        let mut context = self
            .context(model, generation)
            .map_err(|_| super::wire::WireError::Session)?;
        let mut admitted_retry = None;
        if self.introducer_target.is_none()
            && super::wire::is_join_request(bytes)
            && let SenderIdentity::Admitted(node) = &context.sender
        {
            let member = model.member(node).ok_or(super::wire::WireError::Session)?;
            admitted_retry = Some(node.clone());
            context.sender = SenderIdentity::Applicant(member.name.clone());
        }
        let mut decoded = super::wire::decode(bytes, &context, transport)?;
        decoded.admitted_retry = admitted_retry;
        if self.introducer_target.is_some()
            && !matches!(
                decoded.input.body,
                orishu_membership::PeerBody::JoinReply(_)
            )
        {
            return Err(super::wire::WireError::Session);
        }
        Ok(decoded)
    }

    pub(crate) fn check_generation(
        &self,
        model: &Membership,
        generation: Generation,
    ) -> Result<(), SessionError> {
        if self.generation != generation || &self.formation != model.formation() {
            return Err(SessionError::Stale);
        }
        Ok(())
    }

    pub(crate) fn fingerprint(&self) -> CertFingerprint {
        self.fingerprint
    }
    pub(crate) fn is_unbound(&self) -> bool {
        self.sender.is_none()
    }

    fn check_member(&self, model: &Membership, node: &NodeId) -> Result<(), SessionError> {
        validate_member(model, node, self.fingerprint)
    }
}

/// Shared current-identity check for registered sessions and retained transfers.
/// Transport authentication, source-network policy and generation remain caller
/// obligations; a member record alone is not an authenticated session.
pub(crate) fn validate_member(
    model: &Membership,
    node: &NodeId,
    fingerprint: CertFingerprint,
) -> Result<(), SessionError> {
    let member = model.member(node).ok_or(SessionError::Identity)?;
    use orishu_membership::model::BlocklistKey;
    if node == model.local_id()
        || member.cert_fingerprint != fingerprint
        || [
            BlocklistKey::Node(node.clone()),
            BlocklistKey::Name(member.name.clone()),
            BlocklistKey::Fingerprint(fingerprint),
        ]
        .iter()
        .any(|key| model.is_blocked(key))
        || !matches!(member.liveness, Liveness::Alive | Liveness::Suspected)
        || model
            .tombstones()
            .values()
            .any(|tombstone| !tombstone.cleared && tombstone.cert_fingerprint == fingerprint)
    {
        return Err(SessionError::Identity);
    }
    Ok(())
}

#[cfg(test)]
#[path = "formation_replay.rs"]
mod formation_replay;

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::{Command, Message, RemovalMode, testing};

    #[test]
    fn new_member_handshake_waits_for_trusted_serialized_membership_discovery() {
        use super::super::{handshake, wire};
        use orishu_membership::{DeltaBody, GossipDelta, OutboundBody, OutboundMessage, ProbeId};

        // A knows B; B has admitted C. TLS facts are fixture inputs here:
        // this reproduces the application-binding dependency, not TLS or
        // thirty-worker wall-clock convergence.
        let mut a = testing::standalone("node-A");
        let mut b = testing::member("node-B", 1);
        b.cert_fingerprint = testing::fingerprint(2);
        let mut c = testing::member("node-C", 1);
        c.cert_fingerprint = testing::fingerprint(3);
        testing::insert_member(&mut a, b.clone());
        let mut local_c = testing::local_identity(c.id.as_str());
        local_c.worker_name = c.name.clone();
        local_c.cert_fingerprint = c.cert_fingerprint;
        let hello = handshake::request(&local_c, a.formation().clone(), false).unwrap();
        let mut from_c = AuthenticatedSession {
            id: SessionId(3),
            generation: Generation(0),
            formation: a.formation().clone(),
            fingerprint: c.cert_fingerprint,
            sender: None,
            introducer_target: None,
        };
        assert!(handshake::accept_request(&hello, &mut from_c, &a, Generation(0)).is_err());
        assert!(from_c.is_unbound());
        assert!(a.member(&c.id).is_none(), "a handshake cannot admit C");

        let mut from_b = session(&a, &b.id);
        from_b.bind_member(&a, Generation(0), b.id.clone()).unwrap();
        let packet = wire::encode(
            OutboundMessage {
                seq: 1,
                body: OutboundBody::Ping {
                    probe: ProbeId(1),
                    incarnation: b.incarnation,
                },
                gossip: vec![GossipDelta {
                    hops: 1,
                    body: DeltaBody::MembershipUpdate(c.clone()),
                }],
            },
            a.formation().clone(),
            SenderIdentity::Admitted(b.id),
            None,
        )
        .unwrap();
        assert_eq!(packet.deferred_gossip, 0);
        // Decode through the actual bound-session/wire path, then merge in
        // the same core used by the owner; no direct insertion of C into A.
        let decoded = from_b
            .decode(&packet.bytes, &a, Generation(0), packet.transport)
            .unwrap();
        let mut driver = testing::Driver::new(a);
        driver.apply(Message::Peer(decoded.input));
        assert_eq!(driver.model().member(&c.id), Some(&c));
        handshake::accept_request(&hello, &mut from_c, driver.model(), Generation(0)).unwrap();
        assert_eq!(
            from_c
                .context(driver.model(), Generation(0))
                .unwrap()
                .sender,
            SenderIdentity::Admitted(c.id.clone())
        );

        driver.apply(Message::Local(Command::RemoveMember {
            node: c.id,
            mode: RemovalMode::Force,
            reason: None,
        }));
        assert!(from_c.context(driver.model(), Generation(0)).is_err());
        assert!(
            handshake::accept_request(&hello, &mut from_c, driver.model(), Generation(0)).is_err()
        );
    }

    fn session(model: &Membership, node: &NodeId) -> AuthenticatedSession {
        AuthenticatedSession {
            id: SessionId(1),
            generation: Generation(0),
            formation: model.formation().clone(),
            fingerprint: model.member(node).unwrap().cert_fingerprint,
            sender: None,
            introducer_target: None,
        }
    }

    #[test]
    fn label_never_substitutes_for_assigned_identity_or_certificate() {
        let model = testing::model_with_members(1);
        let node = NodeId::new("node-0000").unwrap();
        let mut peer = session(&model, &node);
        assert_eq!(
            peer.context(&model, Generation(0)).unwrap_err(),
            SessionError::Unbound
        );
        peer.bind_applicant(WorkerName::new(node.as_str()).unwrap())
            .unwrap();
        assert!(matches!(
            peer.context(&model, Generation(0)).unwrap().sender,
            SenderIdentity::Applicant(_)
        ));
        assert_eq!(
            peer.bind_member(&model, Generation(0), node.clone()),
            Err(SessionError::Identity)
        );
        let mut peer = session(&model, &node);
        peer.fingerprint = testing::fingerprint(99);
        assert_eq!(
            peer.bind_member(&model, Generation(0), node),
            Err(SessionError::Identity)
        );
    }

    #[test]
    fn accepted_binding_is_rechecked_after_removal_and_generation_changes() {
        let model = testing::model_with_members(1);
        let node = NodeId::new("node-0000").unwrap();
        let mut peer = session(&model, &node);
        peer.bind_applicant(model.member(&node).unwrap().name.clone())
            .unwrap();
        peer.bind_member(&model, Generation(0), node.clone())
            .unwrap();
        assert_eq!(
            peer.context(&model, Generation(0)).unwrap().sender,
            SenderIdentity::Admitted(node.clone())
        );
        assert_eq!(
            peer.context(&model, Generation(1)).unwrap_err(),
            SessionError::Stale
        );
        let model = orishu_membership::update(
            model,
            Message::Local(Command::RemoveMember {
                node,
                mode: RemovalMode::Force,
                reason: None,
            }),
        )
        .model;
        assert_eq!(
            peer.context(&model, Generation(0)).unwrap_err(),
            SessionError::Identity
        );
    }

    #[test]
    fn existing_binding_does_not_bypass_a_new_identity_block() {
        let model = testing::model_with_members(1);
        let node = NodeId::new("node-0000").unwrap();
        let mut peer = session(&model, &node);
        peer.bind_member(&model, Generation(0), node.clone())
            .unwrap();
        let block = testing::blocklist_entry(model.member(&node).unwrap().name.as_str());
        let model =
            orishu_membership::update(model, Message::Local(Command::UpdateBlocklist(block))).model;
        assert_eq!(
            peer.context(&model, Generation(0)).unwrap_err(),
            SessionError::Identity
        );
        let mut self_connection = session(&model, model.local_id());
        assert_eq!(
            self_connection.bind_member(&model, Generation(0), model.local_id().clone()),
            Err(SessionError::Identity)
        );
    }

    #[test]
    fn outbound_introducer_is_pinned_but_not_an_admitted_peer() {
        let model = testing::model_with_members(0);
        let mut peer = session(&model, model.local_id());
        let target = FormationId::new("other-formation").unwrap();
        let node = NodeId::new("introducer").unwrap();
        let pin = peer.fingerprint;
        assert_eq!(
            peer.bind_introducer(
                &model,
                Generation(0),
                target.clone(),
                node.clone(),
                testing::fingerprint(99)
            ),
            Err(SessionError::Identity)
        );
        peer.bind_introducer(&model, Generation(0), target.clone(), node.clone(), pin)
            .unwrap();
        use orishu_membership::{OutboundBody, OutboundMessage, RejectReason};
        let encode = |body| {
            super::super::wire::encode(
                OutboundMessage {
                    body,
                    seq: 1,
                    gossip: vec![],
                },
                target.clone(),
                SenderIdentity::Admitted(node.clone()),
                None,
            )
            .unwrap()
        };
        let reply = encode(OutboundBody::JoinRejected {
            reason: RejectReason::MembershipLocked,
        });
        assert!(
            peer.decode(&reply.bytes, &model, Generation(0), reply.transport)
                .is_ok()
        );
        let ping = encode(OutboundBody::Ping {
            probe: orishu_membership::ProbeId(1),
            incarnation: orishu_membership::Incarnation::INITIAL,
        });
        assert!(
            peer.decode(&ping.bytes, &model, Generation(0), ping.transport)
                .is_err()
        );
        assert!(
            peer.decode(&reply.bytes, &model, Generation(1), reply.transport)
                .is_err()
        );
        assert_eq!(
            peer.bind_member(&model, Generation(0), model.local_id().clone()),
            Err(SessionError::Identity)
        );
    }
}
