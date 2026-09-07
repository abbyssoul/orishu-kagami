//! Bounded application handshake, separate from admission and its secret token.

use super::{
    codec,
    session::{AuthenticatedSession, SessionError},
};
use crate::driver::Generation;
use orishu_membership::{
    CertFingerprint, FormationId, GossipDelta, LocalIdentity, Membership, NodeId, ProtocolVersion,
    WorkerName,
};
use serde::{Deserialize, Serialize};

/// Maximum handshake payload, checked at stream framing before allocation.
pub const MAX_HANDSHAKE_BYTES: usize = 4096;

/// Bounded handshake refusal vocabulary, independent of admission gate decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Refusal {
    IncompatibleProtocol,
    FormationMismatch,
    IdentityMismatch,
    Overloaded,
}

/// Errors never contain a claimed name, certificate or peer payload.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    /// CBOR or size validation failed.
    #[error(transparent)]
    Codec(#[from] codec::CodecError),
    /// Session no longer belongs to this generation or identity.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Envelope, role or phase fields are inconsistent.
    #[error("invalid peer handshake")]
    Invalid,
    /// The target explicitly refused the handshake; no binding was granted.
    #[error("peer handshake refused: {0:?}")]
    Refused(Refusal),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope {
    proto: ProtocolVersion,
    #[serde(rename = "type")]
    kind: Kind,
    sender_id: String,
    formation_id: FormationId,
    seq: u64,
    payload: Payload,
    gossip: Vec<GossipDelta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
enum Kind {
    Handshake,
    HandshakeAck,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Payload {
    #[serde(deserialize_with = "required_node_id")]
    node_id: Option<NodeId>,
    node_name: WorkerName,
    cert_fingerprint: CertFingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<Refusal>,
}

fn required_node_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NodeId>, D::Error> {
    Option::<NodeId>::deserialize(deserializer)
}

/// Encode the initial request. A join applicant explicitly omits its old node
/// identity; a reconnecting member uses its current assigned ID.
pub fn request(
    local: &LocalIdentity,
    target: FormationId,
    joining: bool,
) -> Result<Vec<u8>, HandshakeError> {
    encode(local, target, joining, None)
}

/// Encode an ACK/NACK on the request's bidirectional stream. A successful
/// handshake is transport binding, never admission or catch-up completion.
pub fn reply(
    local: &LocalIdentity,
    formation: FormationId,
    refusal: Option<Refusal>,
) -> Result<Vec<u8>, HandshakeError> {
    encode(local, formation, false, Some(refusal))
}

fn encode(
    local: &LocalIdentity,
    formation: FormationId,
    applicant: bool,
    reply: Option<Option<Refusal>>,
) -> Result<Vec<u8>, HandshakeError> {
    let node_id = (!applicant).then(|| local.node_id.clone());
    let sender_id = node_id
        .as_ref()
        .map_or(local.worker_name.as_str(), NodeId::as_str)
        .to_owned();
    let envelope = Envelope {
        proto: ProtocolVersion::CURRENT,
        kind: if reply.is_some() {
            Kind::HandshakeAck
        } else {
            Kind::Handshake
        },
        sender_id,
        formation_id: formation,
        seq: 0,
        gossip: vec![],
        payload: Payload {
            node_id,
            node_name: local.worker_name.clone(),
            cert_fingerprint: local.cert_fingerprint,
            accepted: reply.map(|refusal| refusal.is_none()),
            reason: reply.flatten(),
        },
    };
    let bytes = codec::encode(&envelope)?;
    if bytes.len() > MAX_HANDSHAKE_BYTES {
        return Err(codec::CodecError::TooLarge.into());
    }
    Ok(bytes)
}

fn decode(
    bytes: &[u8],
    session: &AuthenticatedSession,
    kind: Kind,
    formation: &FormationId,
) -> Result<Envelope, HandshakeError> {
    if bytes.len() > MAX_HANDSHAKE_BYTES {
        return Err(codec::CodecError::TooLarge.into());
    }
    if !session.is_unbound() {
        return Err(HandshakeError::Invalid);
    }
    let envelope: Envelope = codec::decode(bytes)?;
    let payload = &envelope.payload;
    if envelope.proto != ProtocolVersion::CURRENT
        || envelope.kind != kind
        || &envelope.formation_id != formation
        || envelope.seq != 0
        || !envelope.gossip.is_empty()
        || payload.cert_fingerprint != session.fingerprint()
        || envelope.sender_id
            != payload
                .node_id
                .as_ref()
                .map_or(payload.node_name.as_str(), NodeId::as_str)
    {
        return Err(HandshakeError::Invalid);
    }
    Ok(envelope)
}

/// Bind an inbound request using current owner state and TLS facts. No token is
/// consumed here and no member is inserted. Call once on the first bidi stream.
pub fn accept_request(
    bytes: &[u8],
    session: &mut AuthenticatedSession,
    model: &Membership,
    generation: Generation,
) -> Result<(), HandshakeError> {
    session.check_generation(model, generation)?;
    let envelope = decode(bytes, session, Kind::Handshake, model.formation())?;
    let payload = envelope.payload;
    if payload.accepted.is_some() || payload.reason.is_some() {
        return Err(HandshakeError::Invalid);
    }
    match payload.node_id {
        Some(node) => {
            if model
                .member(&node)
                .is_none_or(|member| member.name != payload.node_name)
            {
                return Err(HandshakeError::Invalid);
            }
            session.bind_member(model, generation, node)?;
        }
        None => session.bind_applicant(payload.node_name)?,
    }
    Ok(())
}

/// Bind a reply to the pinned target. Before joining, the introducer receives a
/// JoinReply-only role; a reconnect uses the current admitted membership instead.
pub fn accept_reply(
    bytes: &[u8],
    session: &mut AuthenticatedSession,
    model: &Membership,
    generation: Generation,
    target: FormationId,
    trusted_fingerprint: CertFingerprint,
) -> Result<(), HandshakeError> {
    session.check_generation(model, generation)?;
    let envelope = decode(bytes, session, Kind::HandshakeAck, &target)?;
    let payload = envelope.payload;
    if trusted_fingerprint != session.fingerprint() {
        return Err(HandshakeError::Invalid);
    }
    match (payload.accepted, payload.reason) {
        (Some(false), Some(reason)) => return Err(HandshakeError::Refused(reason)),
        (Some(true), None) => {}
        _ => return Err(HandshakeError::Invalid),
    }
    let node = payload.node_id.ok_or(HandshakeError::Invalid)?;
    if &target == model.formation() {
        if model
            .member(&node)
            .is_none_or(|member| member.name != payload.node_name)
        {
            return Err(HandshakeError::Invalid);
        }
        session.bind_member(model, generation, node)?;
    } else {
        session.bind_introducer(model, generation, target, node, trusted_fingerprint)?;
    }
    Ok(())
}

/// Validate operator-bound introducer identity before establishing its limited
/// JoinReply role. A matching certificate alone cannot substitute a different
/// formation-assigned identity from stale or mismatched bootstrap material.
pub fn accept_pinned_introducer_reply(
    bytes: &[u8],
    session: &mut AuthenticatedSession,
    model: &Membership,
    generation: Generation,
    target: FormationId,
    node: &orishu_membership::NodeId,
    fingerprint: CertFingerprint,
) -> Result<(), HandshakeError> {
    session.check_generation(model, generation)?;
    let envelope = decode(bytes, session, Kind::HandshakeAck, &target)?;
    if envelope.payload.node_id.as_ref() != Some(node) {
        return Err(HandshakeError::Invalid);
    }
    accept_reply(bytes, session, model, generation, target, fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_applicant_handshake_has_no_old_node_identity_or_credentials() {
        let mut local = orishu_membership::testing::local_identity("old-node");
        local.worker_name = WorkerName::new("w").unwrap();
        local.cert_fingerprint = CertFingerprint::from_bytes([0; 32]);
        let bytes = request(&local, FormationId::new("f").unwrap(), true).unwrap();
        let mut golden = b"\xa7\x65proto\x01\x64type\x69Handshake\x68senderId\x61w\x6bformationId\x61f\x63seq\x00\x67payload\xa3\x66nodeId\xf6\x68nodeName\x61w\x6fcertFingerprint\x58\x20".to_vec();
        golden.extend_from_slice(&[0; 32]);
        golden.extend_from_slice(b"\x66gossip\x80");
        assert_eq!(bytes, golden);
        let envelope: Envelope = codec::decode(&bytes).unwrap();
        assert!(envelope.payload.node_id.is_none());
        assert!(!bytes.windows(8).any(|window| window == b"old-node"));
        for end in 0..bytes.len() {
            assert!(codec::decode::<Envelope>(&bytes[..end]).is_err());
        }
        let mut value = serde_json::to_value(&envelope).unwrap();
        value["payload"].as_object_mut().unwrap().remove("nodeId");
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }
}
