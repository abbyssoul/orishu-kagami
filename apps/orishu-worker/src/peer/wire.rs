//! Membership profile 5: explicit transport DTOs and secret-free core inputs.

use orishu_membership::{
    Accepts, Announcement, Capabilities, CertFingerprint, ClusterName, FormationId, GossipDelta,
    Incarnation, IndirectResult, JoinReply, Member, NodeCapacity, NodeId, OutboundBody,
    OutboundMessage, PeerBody, PeerContext, PeerInput, ProbeId, ProtocolVersion, RejectReason,
    SenderIdentity, WorkerName,
    antientropy::MerkleDigest,
    model::{Address, AntiEntropyCursor, Endpoints},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::codec::{self, CodecError};
use crate::credentials::SecretToken;

/// Optional-context encoding for the single negotiated profile 5.
pub mod profile5;

/// Datagram payload ceiling; framing is absent and SWIM never falls back to streams.
pub const MAX_DATAGRAM_BYTES: usize = 1200;

/// Secret-free failure categories suitable for bounded diagnostics.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    /// Invalid or oversized CBOR.
    #[error(transparent)]
    Codec(#[from] CodecError),
    /// Session facts do not match the envelope.
    #[error("peer envelope does not match authenticated session")]
    Session,
    /// Protocol version is not supported by this profile.
    #[error("unsupported membership protocol version")]
    Version,
    /// Wrong channel for this message's delivery semantics.
    #[error("membership message used the wrong transport")]
    Transport,
    /// Admission credential is missing or malformed.
    #[error("invalid join credential")]
    Credential,
}

/// The delivery semantics required by a membership message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Reliable, bounded, length-prefixed frame on a bidirectional stream.
    Stream,
    /// Loss-tolerant SWIM datagram with no stream fallback.
    Datagram,
}

// Intentionally no Debug: this DTO may own a raw token on an encrypted wire path.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    proto: ProtocolVersion,
    sender_id: String,
    formation_id: FormationId,
    seq: u64,
    #[serde(flatten)]
    body: Body,
    gossip: Vec<GossipDelta>,
    // Borrowed and parsed separately after session/domain validation. Ignoring
    // here allows non-text optional context without retaining its contents.
    #[serde(default, skip_serializing, rename = "traceParent")]
    _trace_parent: serde::de::IgnoredAny,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", deny_unknown_fields)]
enum Body {
    JoinReq(JoinRequest),
    JoinReply(Decision),
    Ping(Probe),
    Ack(Probe),
    PingReq(IndirectProbe),
    PingReply(IndirectReply),
    Announce(Notice),
    PullReq(PullRequest),
    PullReply(PullResponse),
}

impl Body {
    fn transport(&self) -> Transport {
        match self {
            Self::JoinReq(_) | Self::JoinReply(_) | Self::PullReq(_) | Self::PullReply(_) => {
                Transport::Stream
            }
            _ => Transport::Datagram,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JoinRequest {
    attempt_id: NodeId,
    join_token: String,
    node_name: WorkerName,
    cert_fingerprint: CertFingerprint,
    endpoints: Endpoints,
    accepts: Accepts,
    capacity: NodeCapacity,
    capabilities: Capabilities,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", deny_unknown_fields)]
enum Decision {
    #[serde(rename = "ACK", rename_all = "camelCase")]
    Accepted {
        formation_id: FormationId,
        cluster_name: ClusterName,
        assigned_node_id: NodeId,
        membership: Vec<Member>,
    },
    #[serde(rename = "NACK")]
    Rejected { reason: RejectReason },
    #[serde(rename = "Redirect", rename_all = "camelCase")]
    Redirect { redirect_to: Vec<Address> },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    #[serde(rename = "probeId")]
    probe: ProbeId,
    incarnation: Incarnation,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndirectProbe {
    #[serde(rename = "probeId")]
    probe: ProbeId,
    #[serde(rename = "targetId")]
    target: NodeId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndirectReply {
    #[serde(rename = "probeId")]
    probe: ProbeId,
    #[serde(rename = "targetId")]
    target: NodeId,
    result: IndirectResult,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Notice {
    announcement: Announcement,
    #[serde(rename = "targetId")]
    target: NodeId,
    incarnation: Incarnation,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PullRequest {
    round: u64,
    digest: MerkleDigest,
    buckets: Vec<u16>,
    cursor: Option<AntiEntropyCursor>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PullResponse {
    round: u64,
    digest: MerkleDigest,
    deltas: Vec<GossipDelta>,
    complete: bool,
    cursor: Option<AntiEntropyCursor>,
}

/// Decoded input and a separately owned credential. Only `input` enters the core.
/// No Debug implementation: even accidental structured logging must not expose tokens.
pub struct Decoded {
    /// Replayable semantic message with authenticated transport facts.
    pub input: PeerInput,
    /// Hold only in the bounded admission session until verification completes.
    pub join_token: Option<SecretToken>,
    /// Shell-only identity and canonical public-request digest for admission replay.
    pub(crate) join_attempt: Option<JoinAttempt>,
    /// An admitted session may only replay its original assignment, never admit
    /// again. The owner must check its ledger before applying any core input.
    pub(crate) admitted_retry: Option<NodeId>,
    /// Optional diagnostic ancestry from a validated profile-5 envelope. Never
    /// feed this into the membership core or adopt it before credential checks.
    pub trace_parent: Option<crate::trace_context::TraceParent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JoinAttempt {
    pub id: NodeId,
    pub digest: [u8; 32],
}

pub(crate) fn is_join_request(bytes: &[u8]) -> bool {
    codec::record_fields(bytes).is_ok_and(|fields| {
        fields.into_iter().any(|(name, value)| {
            name == "type" && codec::decode::<String>(value).is_ok_and(|kind| kind == "JoinReq")
        })
    })
}

/// Decode an unframed CBOR payload after TLS. `session` must come from the
/// shell's authenticated connection binding, not claimed envelope fields.
/// Its sequence/gossip fields are replaced by the validated wire values.
pub fn decode(
    bytes: &[u8],
    session: &PeerContext,
    transport: Transport,
) -> Result<Decoded, WireError> {
    decode_inner(bytes, session, transport, true)
}

fn decode_inner(
    bytes: &[u8],
    session: &PeerContext,
    transport: Transport,
    trace_extension: bool,
) -> Result<Decoded, WireError> {
    if !session.authenticated {
        return Err(WireError::Session);
    }
    if transport == Transport::Datagram && bytes.len() > MAX_DATAGRAM_BYTES {
        return Err(CodecError::TooLarge.into());
    }
    let fields = codec::record_fields(bytes)?;
    const NAMES: [&str; 7] = [
        "proto",
        "senderId",
        "formationId",
        "seq",
        "type",
        "payload",
        "gossip",
    ];
    let has_trace = trace_extension && fields.iter().any(|(name, _)| *name == "traceParent");
    if fields.len() != NAMES.len() + usize::from(has_trace)
        || fields
            .iter()
            .any(|(name, _)| !NAMES.contains(name) && !(has_trace && *name == "traceParent"))
    {
        return Err(CodecError::Schema.into());
    }
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
            .ok_or(CodecError::Schema)
    };
    let protocol: ProtocolVersion = codec::decode(field("proto")?)?;
    if protocol != ProtocolVersion::CURRENT || session.protocol != protocol {
        return Err(WireError::Version);
    }
    let formation: FormationId = codec::decode(field("formationId")?)?;
    let sender: String = codec::decode(field("senderId")?)?;
    let expected = match &session.sender {
        SenderIdentity::Admitted(id) => id.as_str(),
        SenderIdentity::Applicant(name) => name.as_str(),
    };
    if formation != session.formation || sender != expected {
        return Err(WireError::Session);
    }
    // Envelope facts have passed before any typed payload or membership allocation.
    let envelope: Envelope = codec::decode(bytes)?;
    if envelope.body.transport() != transport {
        return Err(WireError::Transport);
    }
    if matches!(session.sender, SenderIdentity::Applicant(_))
        && !matches!(envelope.body, Body::JoinReq(_))
    {
        return Err(WireError::Session);
    }
    let mut join_token = None;
    let mut join_attempt = None;
    let body = match envelope.body {
        Body::JoinReq(request) => {
            if session.sender != SenderIdentity::Applicant(request.node_name.clone())
                || session.cert_fingerprint != request.cert_fingerprint
            {
                return Err(WireError::Session);
            }
            let public = codec::encode(&(
                protocol,
                &formation,
                &request.attempt_id,
                &request.node_name,
                request.cert_fingerprint,
                &request.endpoints,
                request.accepts,
                request.capacity,
                &request.capabilities,
            ))?;
            let mut hash = Sha256::new();
            hash.update(b"orishu/admission-attempt/1\0");
            hash.update(public);
            join_attempt = Some(JoinAttempt {
                id: request.attempt_id,
                digest: hash.finalize().into(),
            });
            join_token =
                Some(SecretToken::parse(request.join_token).map_err(|_| WireError::Credential)?);
            PeerBody::JoinRequest {
                endpoints: request.endpoints,
                accepts: request.accepts,
                capacity: request.capacity,
                capabilities: request.capabilities,
            }
        }
        Body::JoinReply(decision) => PeerBody::JoinReply(match decision {
            Decision::Accepted {
                formation_id,
                cluster_name,
                assigned_node_id,
                membership,
            } => {
                let payload = codec::record_fields(field("payload")?)?;
                let snapshot_bytes = payload
                    .iter()
                    .find(|(key, _)| *key == "membership")
                    .ok_or(CodecError::Schema)?
                    .1
                    .len();
                JoinReply::Accepted {
                    formation: formation_id,
                    cluster_name,
                    assigned: assigned_node_id,
                    snapshot: membership,
                    snapshot_bytes,
                }
            }
            Decision::Rejected { reason } => JoinReply::Rejected { reason },
            Decision::Redirect { redirect_to } => JoinReply::Redirect {
                candidates: redirect_to,
            },
        }),
        Body::Ping(Probe { probe, incarnation }) => PeerBody::Ping { probe, incarnation },
        Body::Ack(Probe { probe, incarnation }) => PeerBody::Ack { probe, incarnation },
        Body::PingReq(IndirectProbe { probe, target }) => PeerBody::PingReq { probe, target },
        Body::PingReply(IndirectReply {
            probe,
            target,
            result,
        }) => PeerBody::PingReply {
            probe,
            target,
            result,
        },
        Body::Announce(Notice {
            announcement,
            target,
            incarnation,
        }) => PeerBody::Announce {
            announcement,
            target,
            incarnation,
        },
        Body::PullReq(PullRequest {
            round,
            digest,
            buckets,
            cursor,
        }) => PeerBody::PullRequest {
            round,
            digest,
            buckets,
            cursor,
        },
        Body::PullReply(PullResponse {
            round,
            digest,
            deltas,
            complete,
            cursor,
        }) => PeerBody::PullReply {
            round,
            digest,
            deltas,
            complete,
            cursor,
        },
    };
    let mut context = session.clone();
    context.seq = envelope.seq;
    context.gossip = envelope.gossip;
    let trace_parent = if has_trace {
        codec::bounded_text(
            field("traceParent")?,
            crate::trace_context::MAX_TRACE_PARENT_BYTES,
        )
        .and_then(|text| crate::trace_context::TraceParent::parse(text.as_bytes()))
    } else {
        None
    };
    Ok(Decoded {
        input: PeerInput { context, body },
        join_token,
        join_attempt,
        admitted_retry: None,
        trace_parent,
    })
}

/// Encoded unframed message, with explicit delivery and deferred gossip count.
pub struct Encoded {
    /// CBOR payload. Secret-bearing join frames must never be logged.
    pub bytes: Vec<u8>,
    /// Required transport; callers must not promote oversized datagrams.
    pub transport: Transport,
    /// Supplied deltas omitted to fit this datagram; reconciliation must retain them.
    pub deferred_gossip: usize,
    /// Locally omitted values, consumed by owner feedback before network IO.
    pub(crate) deferred: Vec<GossipDelta>,
}

impl Encoded {
    /// Append optional IO-owned ancestry only when it fits the selected packet.
    /// Local sampling/credential policy belongs to the caller, not this codec.
    #[cfg(feature = "otlp-tracing")]
    pub(crate) fn with_parent(self, parent: Option<crate::trace_context::TraceParent>) -> Self {
        profile5::attach(self, parent)
    }
}

/// Map a core effect to its wire DTO, attaching a join token only on `JoinReq`.
/// Datagram gossip is truncated to fit; an oversized base is an explicit error.
pub fn encode(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
    token: Option<&SecretToken>,
) -> Result<Encoded, WireError> {
    encode_inner(message, formation, sender, token, None)
}

/// Encode a member effect and identify gossip the owner must retain locally.
/// This is sender bookkeeping, never peer receipt or authority to merge data.
pub(crate) fn encode_member(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
) -> Result<(Encoded, Vec<GossipDelta>), WireError> {
    let mut encoded = encode(message, formation, sender, None)?;
    let deferred = std::mem::take(&mut encoded.deferred);
    Ok((encoded, deferred))
}

/// Encode an applicant request with the same fresh shell identity on every retry.
pub(crate) fn encode_join(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
    token: &SecretToken,
    attempt: &NodeId,
) -> Result<Encoded, WireError> {
    if !matches!(message.body, OutboundBody::JoinRequest { .. }) {
        return Err(WireError::Session);
    }
    encode_inner(
        message,
        formation,
        sender,
        Some(token),
        Some(attempt.clone()),
    )
}

fn encode_inner(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
    token: Option<&SecretToken>,
    attempt: Option<NodeId>,
) -> Result<Encoded, WireError> {
    let sender_id = match &sender {
        SenderIdentity::Admitted(id) => id.as_str(),
        SenderIdentity::Applicant(name) => name.as_str(),
    }
    .to_owned();
    let body = match message.body {
        OutboundBody::JoinRequest {
            name,
            cert_fingerprint,
            protocol,
            endpoints,
            accepts,
            capacity,
            capabilities,
        } => {
            if protocol != ProtocolVersion::CURRENT {
                return Err(WireError::Version);
            }
            if sender != SenderIdentity::Applicant(name.clone()) {
                return Err(WireError::Session);
            }
            Body::JoinReq(JoinRequest {
                attempt_id: attempt.ok_or(WireError::Session)?,
                node_name: name,
                cert_fingerprint,
                endpoints,
                accepts,
                capacity,
                capabilities,
                join_token: token.ok_or(WireError::Credential)?.expose().to_owned(),
            })
        }
        OutboundBody::JoinAccepted {
            formation,
            cluster_name,
            assigned,
            snapshot,
        } => Body::JoinReply(Decision::Accepted {
            formation_id: formation,
            cluster_name,
            assigned_node_id: assigned,
            membership: snapshot,
        }),
        OutboundBody::JoinRejected { reason } => Body::JoinReply(Decision::Rejected { reason }),
        OutboundBody::JoinRedirect { candidates } => Body::JoinReply(Decision::Redirect {
            redirect_to: candidates,
        }),
        OutboundBody::Ping { probe, incarnation } => Body::Ping(Probe { probe, incarnation }),
        OutboundBody::Ack { probe, incarnation } => Body::Ack(Probe { probe, incarnation }),
        OutboundBody::PingReq { probe, target } => Body::PingReq(IndirectProbe { probe, target }),
        OutboundBody::PingReply {
            probe,
            target,
            result,
        } => Body::PingReply(IndirectReply {
            probe,
            target,
            result,
        }),
        OutboundBody::Announce {
            announcement,
            target,
            incarnation,
        } => Body::Announce(Notice {
            announcement,
            target,
            incarnation,
        }),
        OutboundBody::PullRequest {
            round,
            digest,
            buckets,
            cursor,
        } => Body::PullReq(PullRequest {
            round,
            digest,
            buckets,
            cursor,
        }),
        OutboundBody::PullReply {
            round,
            digest,
            deltas,
            complete,
            cursor,
        } => Body::PullReply(PullResponse {
            round,
            digest,
            deltas,
            complete,
            cursor,
        }),
    };
    if matches!(sender, SenderIdentity::Applicant(_)) && !matches!(body, Body::JoinReq(_)) {
        return Err(WireError::Session);
    }
    let transport = body.transport();
    let mut envelope = Envelope {
        proto: ProtocolVersion::CURRENT,
        sender_id,
        formation_id: formation,
        seq: message.seq,
        body,
        gossip: message.gossip,
        _trace_parent: serde::de::IgnoredAny,
    };
    // Validate the complete supplied message first, including gossip that may
    // be omitted. Trimming must never hide an invalid or over-limit record.
    let mut bytes = codec::encode(&envelope)?;
    let mut deferred = Vec::new();
    let mut deferred_gossip = 0;
    if transport == Transport::Datagram && bytes.len() > MAX_DATAGRAM_BYTES {
        let mut retained_bytes = bytes.len();
        let mut omitted = Vec::new();
        while retained_bytes > MAX_DATAGRAM_BYTES {
            let delta = envelope.gossip.pop().ok_or(CodecError::TooLarge)?;
            let length = codec::encode(&delta)?.len();
            // Profile validation caps gossip at ten records. Its definite CBOR
            // array header is therefore one byte at every retained length,
            // including zero: removing a record subtracts exactly its encoding.
            retained_bytes = retained_bytes
                .checked_sub(length)
                .ok_or(CodecError::Encoding)?;
            omitted.push((delta, length));
        }
        deferred_gossip = omitted.len();
        bytes = codec::encode(&envelope)?;
        debug_assert_eq!(bytes.len(), retained_bytes);
        let included = std::mem::take(&mut envelope.gossip);
        let base_len = codec::encode(&envelope)?.len();
        envelope.gossip = included;
        // Preserve reverse removal order and only restore records that could
        // fit alone. Larger records remain available through anti-entropy.
        deferred = omitted
            .into_iter()
            .filter(|(_, length)| base_len + length <= MAX_DATAGRAM_BYTES)
            .map(|(delta, _)| delta)
            .collect();
    }
    Ok(Encoded {
        bytes,
        transport,
        deferred_gossip,
        deferred,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::{SessionId, testing};

    #[test]
    fn malformed_text_digest_is_rejected_through_authenticated_wire_decode() {
        let ctx = context();
        let digest =
            orishu_membership::antientropy::MembershipTree::build(&testing::model_with_members(1))
                .digest();
        for kind in ["PullReq", "PullReply"] {
            let payload = if kind == "PullReq" {
                serde_json::json!({"round": 1, "digest": digest, "buckets": [], "cursor": null})
            } else {
                serde_json::json!({"round": 1, "digest": digest, "deltas": [], "complete": true, "cursor": null})
            };
            let packet = serde_json::json!({
                "proto": 1, "senderId": "n", "formationId": "f", "seq": 7,
                "type": kind, "gossip": [], "payload": payload
            });
            // Begin with an accepted envelope so unrelated schema errors cannot
            // mask a failure to reject the mutated root or bucket hash.
            assert!(decode(&codec::encode(&packet).unwrap(), &ctx, Transport::Stream).is_ok());
            for field in ["/payload/digest/root", "/payload/digest/buckets/0"] {
                for character in ['é', '€', '💥', '+'] {
                    for offset in 0..=64 - character.len_utf8() {
                        let hash = format!(
                            "{}{character}{}",
                            "0".repeat(offset),
                            "0".repeat(64 - offset - character.len_utf8())
                        );
                        let mut malformed = packet.clone();
                        *malformed.pointer_mut(field).unwrap() = hash.into();
                        let bytes = codec::encode(&malformed).unwrap();
                        assert!(
                            matches!(
                                decode(&bytes, &ctx, Transport::Stream),
                                Err(WireError::Codec(CodecError::Schema))
                            ),
                            "{kind} {field}: {character:?} at byte {offset}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn already_admitted_rejection_has_stable_cbor_and_round_trips() {
        let reason = orishu_membership::RejectReason::AlreadyAdmitted;
        assert_eq!(codec::encode(&reason).unwrap(), b"\x6falreadyAdmitted");
        let ctx = context();
        let packet = encode(
            OutboundMessage {
                body: OutboundBody::JoinRejected {
                    reason: reason.clone(),
                },
                seq: 1,
                gossip: vec![],
            },
            ctx.formation.clone(),
            ctx.sender.clone(),
            None,
        )
        .unwrap();
        assert_eq!(packet.transport, Transport::Stream);
        assert_eq!(
            decode(&packet.bytes, &ctx, Transport::Stream)
                .unwrap()
                .input
                .body,
            PeerBody::JoinReply(JoinReply::Rejected { reason })
        );
    }

    fn context() -> PeerContext {
        PeerContext {
            session: SessionId(1),
            authenticated: true,
            cert_fingerprint: testing::fingerprint(1),
            sender: SenderIdentity::Admitted(NodeId::new("n").unwrap()),
            formation: FormationId::new("f").unwrap(),
            protocol: ProtocolVersion::CURRENT,
            seq: 0,
            gossip: vec![],
        }
    }

    fn send(body: OutboundBody, ctx: &PeerContext) -> Encoded {
        encode(
            OutboundMessage {
                body,
                seq: 7,
                gossip: vec![],
            },
            ctx.formation.clone(),
            ctx.sender.clone(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn golden_ping_and_session_guards() {
        let ctx = context();
        let packet = send(
            OutboundBody::Ping {
                probe: ProbeId(3),
                incarnation: Incarnation::INITIAL,
            },
            &ctx,
        );
        assert_eq!(packet.bytes, b"\xbf\x65proto\x01\x68senderId\x61n\x6bformationId\x61f\x63seq\x07\x64type\x64Ping\x67payload\xa2\x67probeId\x03\x6bincarnation\x00\x66gossip\x80\xff");
        let decoded = decode(&packet.bytes, &ctx, Transport::Datagram).unwrap();
        assert_eq!(
            decoded.input.body,
            PeerBody::Ping {
                probe: ProbeId(3),
                incarnation: Incarnation::INITIAL
            }
        );
        assert_eq!(decoded.input.context.seq, 7);
        assert!(decoded.join_token.is_none());
        assert!(matches!(
            decode(&packet.bytes, &ctx, Transport::Stream),
            Err(WireError::Transport)
        ));
        let mut wrong = ctx.clone();
        wrong.formation = FormationId::new("wrong").unwrap();
        assert!(matches!(
            decode(&packet.bytes, &wrong, Transport::Datagram),
            Err(WireError::Session)
        ));
        wrong = ctx.clone();
        wrong.sender = SenderIdentity::Applicant(WorkerName::new("n").unwrap());
        assert!(matches!(
            decode(&packet.bytes, &wrong, Transport::Datagram),
            Err(WireError::Session)
        ));
        for end in 0..packet.bytes.len() {
            assert!(decode(&packet.bytes[..end], &ctx, Transport::Datagram).is_err());
        }
    }

    #[test]
    fn join_credential_is_separated_from_replayable_input() {
        let mut ctx = context();
        let name = WorkerName::new("worker").unwrap();
        ctx.sender = SenderIdentity::Applicant(name.clone());
        let token = SecretToken::generate().unwrap();
        let body = OutboundBody::JoinRequest {
            name,
            cert_fingerprint: ctx.cert_fingerprint,
            protocol: ctx.protocol,
            endpoints: Endpoints::default(),
            accepts: Accepts::default(),
            capacity: NodeCapacity::default(),
            capabilities: testing::capabilities(),
        };
        let packet = encode_join(
            OutboundMessage {
                body,
                seq: 2,
                gossip: vec![],
            },
            ctx.formation.clone(),
            ctx.sender.clone(),
            &token,
            &"test-attempt".parse().unwrap(),
        )
        .unwrap();
        let decoded = decode(&packet.bytes, &ctx, Transport::Stream).unwrap();
        let original = decoded.join_attempt.clone().unwrap();
        assert_eq!(codec::encode(&original.id).unwrap(), b"\x6ctest-attempt");
        let mut retry: Envelope = codec::decode(&packet.bytes).unwrap();
        retry.seq += 1;
        let Body::JoinReq(request) = &mut retry.body else {
            unreachable!()
        };
        request.join_token = SecretToken::generate().unwrap().expose().to_owned();
        assert_eq!(
            decode(&codec::encode(&retry).unwrap(), &ctx, Transport::Stream)
                .unwrap()
                .join_attempt,
            Some(original.clone()),
            "sequence and credentials are not request identity"
        );
        let Body::JoinReq(request) = &mut retry.body else {
            unreachable!()
        };
        request.capacity.clients += 1;
        assert_ne!(
            decode(&codec::encode(&retry).unwrap(), &ctx, Transport::Stream)
                .unwrap()
                .join_attempt,
            Some(original)
        );
        assert!(decoded.join_token.unwrap().matches(token.expose()));
        assert!(!format!("{:?}", decoded.input).contains(token.expose()));
        ctx.cert_fingerprint = testing::fingerprint(2);
        assert!(matches!(
            decode(&packet.bytes, &ctx, Transport::Stream),
            Err(WireError::Session)
        ));
    }

    #[test]
    fn accepted_snapshot_reports_original_encoded_length() {
        let ctx = context();
        let member = testing::member("member", 0);
        let packet = send(
            OutboundBody::JoinAccepted {
                formation: ctx.formation.clone(),
                cluster_name: ClusterName::new("cluster").unwrap(),
                assigned: member.id.clone(),
                snapshot: vec![member],
            },
            &ctx,
        );
        let fields = codec::record_fields(&packet.bytes).unwrap();
        let payload = fields.iter().find(|(key, _)| *key == "payload").unwrap().1;
        let membership = codec::record_fields(payload)
            .unwrap()
            .into_iter()
            .find(|(key, _)| *key == "membership")
            .unwrap()
            .1;
        // Replace canonical array(1) with a valid non-minimal length encoding.
        let start = membership.as_ptr() as usize - packet.bytes.as_ptr() as usize;
        let mut changed = packet.bytes.clone();
        changed.splice(start..start + 1, [0x98, 0x01]);
        let decoded = decode(&changed, &ctx, Transport::Stream).unwrap();
        let PeerBody::JoinReply(JoinReply::Accepted { snapshot_bytes, .. }) = decoded.input.body
        else {
            panic!("wrong reply");
        };
        assert_eq!(snapshot_bytes, membership.len() + 1);
    }

    #[test]
    fn every_non_join_body_maps_to_the_semantic_core() {
        let ctx = context();
        let target = NodeId::new("target").unwrap();
        let digest =
            orishu_membership::antientropy::MembershipTree::build(&testing::model_with_members(1))
                .digest();
        let bodies = vec![
            OutboundBody::Ack {
                probe: ProbeId(4),
                incarnation: Incarnation::INITIAL,
            },
            OutboundBody::PingReq {
                probe: ProbeId(4),
                target: target.clone(),
            },
            OutboundBody::PingReply {
                probe: ProbeId(4),
                target: target.clone(),
                result: IndirectResult::Ack(Incarnation::INITIAL),
            },
            OutboundBody::Announce {
                announcement: Announcement::Leave,
                target,
                incarnation: Incarnation::INITIAL,
            },
            OutboundBody::JoinRejected {
                reason: RejectReason::MembershipLocked,
            },
            OutboundBody::JoinRedirect {
                candidates: vec![Address("127.0.0.1:1234".into())],
            },
            OutboundBody::PullRequest {
                round: 2,
                digest: digest.clone(),
                buckets: vec![],
                cursor: None,
            },
            OutboundBody::PullReply {
                round: 2,
                digest,
                deltas: vec![],
                complete: true,
                cursor: None,
            },
        ];
        for body in bodies {
            let packet = send(body, &ctx);
            let input = decode(&packet.bytes, &ctx, packet.transport).unwrap();
            assert_eq!(input.input.context.seq, 7);
        }
    }

    #[test]
    fn locally_trimmed_gossip_is_not_retired_before_any_wire_submission() {
        use orishu_membership::{Command, Effect, EffectOutcome, Message, PeerBody, PeerInput};
        use std::collections::BTreeSet;
        for oversized in [false, true] {
            let mut driver = testing::Driver::new(testing::model_with_members(2));
            let peer = NodeId::new("node-0001").unwrap();
            let mut incoming = testing::peer_context(driver.model(), &peer, 1);
            let expected: BTreeSet<_> = (2..if oversized { 11 } else { 12 })
                .map(|index| NodeId::new(format!("node-{index:04}")).unwrap())
                .collect();
            incoming.gossip = expected
                .iter()
                .map(|node| GossipDelta {
                    hops: 0,
                    body: orishu_membership::DeltaBody::MembershipUpdate(testing::member(
                        node.as_str(),
                        1,
                    )),
                })
                .collect();
            if oversized {
                let mut large = testing::member("large", 99);
                large.capabilities.accelerators = vec!["x".repeat(200); 8];
                incoming.gossip.insert(
                    0,
                    GossipDelta {
                        hops: 0,
                        body: orishu_membership::DeltaBody::MembershipUpdate(large),
                    },
                );
            }
            // Unknown ACK keeps the probe path quiet while exercising real merge.
            driver.apply(Message::Peer(PeerInput {
                context: incoming,
                body: PeerBody::Ack {
                    probe: ProbeId(999),
                    incarnation: Incarnation::INITIAL,
                },
            }));
            let mut seen = BTreeSet::new();
            for _ in 0..128 {
                driver.apply(Message::Local(Command::StartProbeRound));
                driver.supply_peers(&[peer.as_str()]);
                let effects = std::mem::take(&mut driver.effects);
                for effect in effects {
                    if let Effect::Send { message, .. } = effect {
                        let (encoded, deferred) = encode_member(
                            message,
                            driver.model().formation().clone(),
                            SenderIdentity::Admitted(driver.model().local_id().clone()),
                        )
                        .unwrap();
                        assert_eq!(encoded.transport, Transport::Datagram);
                        assert!(encoded.bytes.len() <= MAX_DATAGRAM_BYTES);
                        let envelope: Envelope = codec::decode(&encoded.bytes).unwrap();
                        for delta in envelope.gossip {
                            if let orishu_membership::DeltaBody::MembershipUpdate(member) =
                                delta.body
                            {
                                seen.insert(member.id);
                            }
                        }
                        if !deferred.is_empty() {
                            driver.apply(Message::Outcome(EffectOutcome::GossipDeferred {
                                deltas: deferred,
                            }));
                        }
                    }
                }
            }
            assert_eq!(
                seen, expected,
                "every fitting record must get a wire opportunity before retirement"
            );
            assert!(
                driver.model().gossip().is_empty(),
                "submitted gossip still retires"
            );
            if oversized {
                assert!(
                    driver
                        .model()
                        .member(&NodeId::new("large").unwrap())
                        .is_some(),
                    "anti-entropy still owns the oversized record"
                );
            }
        }
    }

    #[test]
    fn datagram_trimming_matches_repeated_encoding() {
        let ctx = context();
        for count in 0..=11 {
            for padding in [0, 23, 24, 255, 700, 1100, 4097] {
                let gossip: Vec<_> = (0..count)
                    .map(|index| {
                        let mut member = testing::member(&format!("node-{index}"), index as u64);
                        // Mix individually unfit and small deltas, including a
                        // malformed record that must fail even if it would be cut.
                        if index % 2 == 0 {
                            member.capabilities.architecture = "x".repeat(padding);
                        }
                        GossipDelta {
                            hops: index,
                            body: orishu_membership::DeltaBody::MembershipUpdate(member),
                        }
                    })
                    .collect();
                let mut envelope = Envelope {
                    proto: ProtocolVersion::CURRENT,
                    sender_id: "n".into(),
                    formation_id: ctx.formation.clone(),
                    seq: 7,
                    body: Body::Ping(Probe {
                        probe: ProbeId(3),
                        incarnation: Incarnation::INITIAL,
                    }),
                    gossip: gossip.clone(),
                    _trace_parent: serde::de::IgnoredAny,
                };
                let reference = (|| -> Result<_, WireError> {
                    let mut deferred = Vec::new();
                    loop {
                        let bytes = codec::encode(&envelope)?;
                        if bytes.len() <= MAX_DATAGRAM_BYTES {
                            let omitted = deferred.len();
                            envelope.gossip.clear();
                            let base = codec::encode(&envelope)?.len();
                            deferred.retain(|delta| {
                                codec::encode(delta)
                                    .is_ok_and(|bytes| base + bytes.len() <= MAX_DATAGRAM_BYTES)
                            });
                            return Ok((bytes, omitted, deferred));
                        }
                        deferred.push(envelope.gossip.pop().ok_or(CodecError::TooLarge)?);
                    }
                })();
                let actual = encode(
                    OutboundMessage {
                        body: OutboundBody::Ping {
                            probe: ProbeId(3),
                            incarnation: Incarnation::INITIAL,
                        },
                        seq: 7,
                        gossip,
                    },
                    ctx.formation.clone(),
                    ctx.sender.clone(),
                    None,
                )
                .map(|packet| {
                    assert_eq!(packet.transport, Transport::Datagram);
                    assert!(decode(&packet.bytes, &ctx, packet.transport).is_ok());
                    (packet.bytes, packet.deferred_gossip, packet.deferred)
                });
                assert_eq!(actual, reference, "count={count}, padding={padding}");
            }
        }
    }

    #[test]
    fn oversized_gossip_is_deferred_without_promoting_swim() {
        let ctx = context();
        let gossip = (0..10)
            .map(|index| GossipDelta {
                hops: 0,
                body: orishu_membership::DeltaBody::MembershipUpdate(testing::member(
                    &format!("node-{index}"),
                    0,
                )),
            })
            .collect();
        let packet = encode(
            OutboundMessage {
                body: OutboundBody::Ping {
                    probe: ProbeId(1),
                    incarnation: Incarnation::INITIAL,
                },
                seq: 8,
                gossip,
            },
            ctx.formation.clone(),
            ctx.sender.clone(),
            None,
        )
        .unwrap();
        assert_eq!(packet.transport, Transport::Datagram);
        assert!(packet.bytes.len() <= MAX_DATAGRAM_BYTES);
        assert!(packet.deferred_gossip > 0);
        let decoded = decode(&packet.bytes, &ctx, packet.transport).unwrap();
        assert_eq!(
            decoded.input.context.gossip.len() + packet.deferred_gossip,
            10
        );
    }

    #[test]
    fn profile_four_refuses_trace_extension_without_changing_domain_decode() {
        let mut value = serde_json::json!({ "proto": 1, "senderId": "n", "formationId": "f", "seq": 1, "type": "Ping", "payload": {"probeId": 1,"incarnation": 0}, "gossip": [] });
        assert_eq!(crate::peer::tls::ALPN, b"orishu-membership/5");
        let ctx = context();
        assert!(decode(&codec::encode(&value).unwrap(), &ctx, Transport::Datagram).is_ok());
        for parent in [
            serde_json::json!("00-11111111111111111111111111111111-2222222222222222-01"),
            serde_json::json!(null),
            serde_json::json!("invalid"),
        ] {
            value["traceParent"] = parent;
            assert!(matches!(
                decode_inner(
                    &codec::encode(&value).unwrap(),
                    &ctx,
                    Transport::Datagram,
                    false
                ),
                Err(WireError::Codec(CodecError::Schema))
            ));
        }
    }

    #[test]
    fn schema_and_field_limits_are_checked_on_actual_cbor() {
        let ctx = context();
        let value = serde_json::json!({ "proto": 1, "senderId": "n", "formationId": "f", "seq": 1, "type": "Ping", "payload": {"probeId": 1, "incarnation": 0}, "gossip": [] });
        for (key, replacement) in [
            ("proto", serde_json::json!(9)),
            ("type", serde_json::json!("Unknown")),
            (
                "payload",
                serde_json::json!({"probeId":1,"incarnation":0,"extra":true}),
            ),
        ] {
            let mut malformed = value.clone();
            malformed[key] = replacement;
            assert!(
                decode(
                    &codec::encode(&malformed).unwrap(),
                    &ctx,
                    Transport::Datagram
                )
                .is_err()
            );
        }
        // Structural preflight rejects a field-specific count before reserving
        // the collection, even when the declared array has no elements present.
        for bytes in [
            &b"\xa1\x66gossip\x8b"[..],
            &b"\xa1\x67engines\x91"[..],
            &b"\xa1\x65peers\x89"[..],
        ] {
            assert_eq!(
                codec::decode::<serde_json::Value>(bytes).unwrap_err(),
                CodecError::Limit
            );
        }
    }
}
