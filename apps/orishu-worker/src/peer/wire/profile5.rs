//! ADR 0025 context grammar for the single negotiated profile 5.
//!
//! All feature builds understand this codec. There is no old-profile negotiation
//! fallback; ordinary context-free encoding remains valid under the same grammar.
use super::*;
use crate::trace_context::TraceParent;

/// Decode with the same authenticated domain checks and one optional context
/// field. Invalid context alone is discarded; malformed CBOR/domain input fails.
pub fn decode(
    bytes: &[u8],
    session: &PeerContext,
    transport: Transport,
) -> Result<Decoded, WireError> {
    super::decode_inner(bytes, session, transport, true)
}

/// Select the ordinary domain packet/gossip first, then add context only if it
/// fits. No context omission changes delivery, payload or deferred gossip.
/// The append adds 69 encoded bytes and validates in O(encoded packet bytes).
pub fn encode(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
    token: Option<&SecretToken>,
    parent: Option<TraceParent>,
) -> Result<Encoded, WireError> {
    let packet = super::encode(message, formation, sender, token)?;
    Ok(attach(packet, parent))
}

/// Preserve the original admission attempt and digest while attaching optional
/// IO-only context. Caller must independently apply local tracing policy.
pub fn encode_join(
    message: OutboundMessage,
    formation: FormationId,
    sender: SenderIdentity,
    token: &SecretToken,
    attempt: &NodeId,
    parent: Option<TraceParent>,
) -> Result<Encoded, WireError> {
    let packet = super::encode_join(message, formation, sender, token, attempt)?;
    Ok(attach(packet, parent))
}

pub(super) fn attach(mut packet: Encoded, parent: Option<TraceParent>) -> Encoded {
    let Some(parent) = parent else {
        return packet;
    };
    // The golden-tested domain encoder emits one indefinite envelope map.
    // Preserve its selected bytes rather than re-encoding/trimming domain data.
    // Any future encoder shape change safely sheds context until reviewed.
    const FIELD: &[u8] = b"\x6btraceParent\x78\x37";
    let extra = FIELD.len() + crate::trace_context::TRACE_PARENT_BYTES;
    let limit = match packet.transport {
        Transport::Stream => codec::MAX_FRAME_BYTES,
        Transport::Datagram => MAX_DATAGRAM_BYTES,
    };
    if packet.bytes.first() != Some(&0xbf)
        || packet.bytes.last() != Some(&0xff)
        || extra > limit.saturating_sub(packet.bytes.len())
        || packet.bytes.try_reserve_exact(extra).is_err()
    {
        return packet;
    }
    packet.bytes.pop();
    packet.bytes.extend_from_slice(FIELD);
    packet.bytes.extend_from_slice(&parent.encode());
    packet.bytes.push(0xff);
    // Context also counts against the pre-existing structural work budget.
    // Revert the exact append if a domain packet had already exhausted it.
    if codec::validate(&packet.bytes).is_err() {
        packet.bytes.truncate(packet.bytes.len() - extra - 1);
        packet.bytes.push(0xff);
    }
    packet
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::{SessionId, testing};

    const PARENT: &str = "00-11111111111111111111111111111111-2222222222222222-01";
    const PING: &[u8] = b"\xbf\x65proto\x01\x68senderId\x61n\x6bformationId\x61f\x63seq\x07\x64type\x64Ping\x67payload\xa2\x67probeId\x03\x6bincarnation\x00\x66gossip\x80\xff";

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

    fn parent() -> Option<TraceParent> {
        TraceParent::parse(PARENT.as_bytes())
    }

    fn ping(gossip: Vec<GossipDelta>) -> OutboundMessage {
        OutboundMessage {
            body: OutboundBody::Ping {
                probe: ProbeId(3),
                incarnation: Incarnation::INITIAL,
            },
            seq: 7,
            gossip,
        }
    }

    fn send(message: OutboundMessage, parent: Option<TraceParent>) -> Encoded {
        let ctx = context();
        encode(message, ctx.formation, ctx.sender, None, parent).unwrap()
    }

    #[test]
    fn golden_context_preserves_all_domain_fields_and_profile_four_refusal() {
        let absent = send(ping(vec![]), None);
        assert_eq!(absent.bytes, PING);
        let present = send(ping(vec![]), parent());
        let mut golden = PING[..PING.len() - 1].to_vec();
        golden.extend_from_slice(b"\x6btraceParent\x78\x37");
        golden.extend_from_slice(PARENT.as_bytes());
        golden.push(0xff);
        assert_eq!(present.bytes, golden);
        let ctx = context();
        let absent = decode(&absent.bytes, &ctx, Transport::Datagram).unwrap();
        let present = decode(&present.bytes, &ctx, Transport::Datagram).unwrap();
        assert_eq!(present.input, absent.input);
        assert!(absent.trace_parent.is_none());
        assert_eq!(present.trace_parent, parent());
        assert_eq!(crate::peer::tls::ALPN, b"orishu-membership/5");
        assert!(matches!(
            super::super::decode_inner(&golden, &ctx, Transport::Datagram, false),
            Err(WireError::Codec(CodecError::Schema))
        ));
        for end in 0..golden.len() {
            assert!(decode(&golden[..end], &ctx, Transport::Datagram).is_err());
        }
        // Duplicate optional keys remain a structural rejection, not ignored metadata.
        golden.pop();
        golden.extend_from_slice(b"\x6btraceParent\xf6\xff");
        assert!(matches!(
            decode(&golden, &ctx, Transport::Datagram),
            Err(WireError::Codec(CodecError::DuplicateKey))
        ));
    }

    #[test]
    fn invalid_metadata_is_ignored_but_domain_and_session_errors_are_not() {
        let ctx = context();
        let plain: serde_json::Value = codec::decode(PING).unwrap();
        let expected = decode(PING, &ctx, Transport::Datagram).unwrap().input;
        for value in [
            serde_json::json!(null),
            serde_json::json!(3),
            serde_json::json!([1, 2]),
            serde_json::json!({"vendor":"secret"}),
            serde_json::json!("bad"),
            serde_json::json!("00-00000000000000000000000000000000-2222222222222222-01"),
            serde_json::json!(format!("{PARENT},{PARENT}")),
            serde_json::json!(format!("01{}-{}", &PARENT[2..], "x".repeat(73))),
        ] {
            let mut packet = plain.clone();
            packet["traceParent"] = value;
            let decoded =
                decode(&codec::encode(&packet).unwrap(), &ctx, Transport::Datagram).unwrap();
            assert_eq!(decoded.input, expected);
            assert!(decoded.trace_parent.is_none());
        }
        let mut packet = plain;
        packet["traceParent"] = serde_json::json!(format!("01{}-{}", &PARENT[2..], "x".repeat(72)));
        let bytes = codec::encode(&packet).unwrap();
        assert_eq!(
            decode(&bytes, &ctx, Transport::Datagram)
                .unwrap()
                .trace_parent,
            parent()
        );
        for wrong in [
            PeerContext {
                authenticated: false,
                ..ctx.clone()
            },
            PeerContext {
                formation: FormationId::new("wrong").unwrap(),
                ..ctx.clone()
            },
            PeerContext {
                sender: SenderIdentity::Applicant(WorkerName::new("n").unwrap()),
                ..ctx.clone()
            },
        ] {
            assert!(matches!(
                decode(&bytes, &wrong, Transport::Datagram),
                Err(WireError::Session)
            ));
        }
        assert!(matches!(
            decode(&bytes, &ctx, Transport::Stream),
            Err(WireError::Transport)
        ));
        packet["unexpected"] = serde_json::json!(true);
        assert!(matches!(
            decode(&codec::encode(&packet).unwrap(), &ctx, Transport::Datagram),
            Err(WireError::Codec(CodecError::Schema))
        ));
        packet.as_object_mut().unwrap().remove("unexpected");
        packet["payload"]["probeId"] = serde_json::json!("bad");
        assert!(decode(&codec::encode(&packet).unwrap(), &ctx, Transport::Datagram).is_err());
    }

    #[test]
    fn optional_context_cannot_change_admission_attempt_or_digest() {
        let mut ctx = context();
        let name = WorkerName::new("worker").unwrap();
        ctx.sender = SenderIdentity::Applicant(name.clone());
        let token = SecretToken::parse("a".repeat(64)).unwrap();
        let attempt = NodeId::new("attempt").unwrap();
        let message = OutboundMessage {
            body: OutboundBody::JoinRequest {
                name,
                cert_fingerprint: ctx.cert_fingerprint,
                protocol: ctx.protocol,
                endpoints: Endpoints::default(),
                accepts: Accepts::default(),
                capacity: NodeCapacity::default(),
                capabilities: testing::capabilities(),
            },
            seq: 2,
            gossip: vec![],
        };
        let absent = encode_join(
            message.clone(),
            ctx.formation.clone(),
            ctx.sender.clone(),
            &token,
            &attempt,
            None,
        )
        .unwrap();
        let present = encode_join(
            message,
            ctx.formation.clone(),
            ctx.sender.clone(),
            &token,
            &attempt,
            parent(),
        )
        .unwrap();
        let present_bytes = present.bytes;
        let absent = decode(&absent.bytes, &ctx, Transport::Stream).unwrap();
        let present = decode(&present_bytes, &ctx, Transport::Stream).unwrap();
        assert_eq!(present.input, absent.input);
        assert_eq!(present.join_attempt, absent.join_attempt);
        assert!(present.join_token.unwrap().matches(token.expose()));
        assert_eq!(present.trace_parent, parent());
        ctx.cert_fingerprint = testing::fingerprint(2);
        // Correctly bound session certificate is checked before returning context.
        assert!(matches!(
            decode(&present_bytes, &ctx, Transport::Stream),
            Err(WireError::Session)
        ));
    }

    #[test]
    fn stream_boundary_preserves_snapshot_and_measured_snapshot_bytes() {
        let ctx = context();
        let snapshot: Vec<_> = (0..16)
            .map(|index| {
                let mut member = testing::member(&format!("member-{index}"), 0);
                member.endpoints.peers = vec![Address("x".repeat(3900)); 8];
                member.endpoints.clients = member.endpoints.peers.clone();
                member
            })
            .collect();
        let base = OutboundMessage {
            body: OutboundBody::JoinAccepted {
                formation: ctx.formation.clone(),
                cluster_name: ClusterName::new("cluster").unwrap(),
                assigned: snapshot[0].id.clone(),
                snapshot,
            },
            seq: 1,
            gossip: vec![],
        };
        let base_bytes = send(base.clone(), None).bytes.len();
        for target in [
            codec::MAX_FRAME_BYTES - 69,
            codec::MAX_FRAME_BYTES - 68,
            codec::MAX_FRAME_BYTES,
        ] {
            let mut message = base.clone();
            let mut extra = target
                .checked_sub(base_bytes)
                .expect("fixture below byte cap");
            let OutboundBody::JoinAccepted { snapshot, .. } = &mut message.body else {
                unreachable!()
            };
            for member in snapshot {
                for address in member
                    .endpoints
                    .peers
                    .iter_mut()
                    .chain(&mut member.endpoints.clients)
                {
                    let append = extra.min(4096 - address.0.len());
                    address.0.extend(std::iter::repeat_n('x', append));
                    extra -= append;
                }
            }
            assert_eq!(extra, 0, "fixture text limits must allow exact target");
            let absent = send(message.clone(), None);
            assert_eq!(absent.bytes.len(), target);
            let present = send(message, parent());
            assert_eq!(present.transport, Transport::Stream);
            let decoded = decode(&present.bytes, &ctx, Transport::Stream).unwrap();
            let plain = decode(&absent.bytes, &ctx, Transport::Stream).unwrap();
            assert_eq!(decoded.input, plain.input); // Includes measured snapshot byte count.
            if target == codec::MAX_FRAME_BYTES - 69 {
                assert_eq!(present.bytes.len(), codec::MAX_FRAME_BYTES);
                assert_eq!(decoded.trace_parent, parent());
            } else {
                assert_eq!(present.bytes, absent.bytes);
                assert!(decoded.trace_parent.is_none());
            }
        }
    }

    #[test]
    fn optional_append_cannot_exceed_structural_work_budget() {
        // Exercise the append primitive at the CBOR work limit independently
        // of any particular domain DTO. This is not a membership-message fixture.
        let mut bytes = b"\xbf\x61x".to_vec();
        let mut arrays = vec![vec![false; 4096]; 7];
        arrays.push(vec![false; 4085]);
        bytes.extend(codec::encode(&arrays).unwrap());
        bytes.push(0xff);
        codec::validate(&bytes).unwrap();
        let packet = attach(
            Encoded {
                bytes: bytes.clone(),
                transport: Transport::Stream,
                deferred_gossip: 0,
                deferred: Vec::new(),
            },
            parent(),
        );
        assert_eq!(packet.bytes, bytes);
    }

    #[test]
    fn datagram_boundary_preserves_selected_gossip_and_never_promotes_transport() {
        let mut found = 0;
        for padding in 0..1200 {
            let mut member = testing::member("node", 0);
            member.endpoints.clients =
                vec![Address(format!("http://127.0.0.1/{}", "x".repeat(padding)))];
            let message = ping(vec![GossipDelta {
                hops: 0,
                body: orishu_membership::DeltaBody::MembershipUpdate(member),
            }]);
            let absent = send(message.clone(), None);
            if ![1131, 1132, 1200].contains(&absent.bytes.len()) {
                continue;
            }
            let present = send(message, parent());
            assert_eq!(present.transport, Transport::Datagram);
            assert_eq!(present.deferred_gossip, absent.deferred_gossip);
            let decoded = decode(&present.bytes, &context(), Transport::Datagram).unwrap();
            assert_eq!(
                decoded.input,
                decode(&absent.bytes, &context(), Transport::Datagram)
                    .unwrap()
                    .input
            );
            if absent.bytes.len() == 1131 {
                assert_eq!(present.bytes.len(), 1200);
                assert_eq!(decoded.trace_parent, parent());
            } else {
                assert_eq!(present.bytes, absent.bytes);
                assert!(decoded.trace_parent.is_none());
            }
            found += 1;
        }
        assert_eq!(
            found, 3,
            "fixture must exercise exact fit and omission boundaries"
        );
        let message = ping(
            (0..10)
                .map(|i| GossipDelta {
                    hops: 0,
                    body: orishu_membership::DeltaBody::MembershipUpdate(testing::member(
                        &format!("node-{i}"),
                        0,
                    )),
                })
                .collect(),
        );
        let absent = send(message.clone(), None);
        let present = send(message, parent());
        assert!(absent.deferred_gossip > 0);
        assert_eq!(present.deferred_gossip, absent.deferred_gossip);
        assert_eq!(
            decode(&present.bytes, &context(), present.transport)
                .unwrap()
                .input,
            decode(&absent.bytes, &context(), absent.transport)
                .unwrap()
                .input
        );
    }
}
