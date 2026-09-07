//! Controlled transport timing, not a public admission/recovery journey.

use super::{
    exchange::{ExchangePool, Phase},
    handshake, read_payload, server,
    session::AuthenticatedSession,
    tls::{PeerIdentity, SERVER_NAME},
    wire::{self, Transport},
    write_payload,
};
use crate::driver::{self, Generation};
use orishu_membership::{
    AdmissionPolicy, Announcement, Command, DeltaBody, GossipDelta, Limits, Liveness, Membership,
    Message, OutboundBody, OutboundMessage, PeerBody, ProbeId, SenderIdentity, SessionId,
    antientropy::MembershipTree, testing,
};
use std::{net::SocketAddr, time::Duration};
use tokio::time::Instant;

fn model(
    identity: &PeerIdentity,
    name: &str,
    formation: orishu_membership::FormationId,
) -> Membership {
    let fixture = testing::standalone(name);
    let mut local = fixture.local().clone();
    local.cert_fingerprint = identity.fingerprint();
    Membership::standalone(
        formation,
        fixture.cluster_name().clone(),
        local,
        AdmissionPolicy::default(),
        Limits::default(),
    )
    .unwrap()
}

/// A real pinned and application-authenticated test peer which deliberately
/// omits piggyback gossip, so the fixture must repair through explicit pulls.
struct Peer {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    binding: AuthenticatedSession,
    model: Membership,
    seq: u64,
}

impl Peer {
    async fn connect(
        address: SocketAddr,
        owner: &PeerIdentity,
        identity: &PeerIdentity,
        model: Membership,
    ) -> Self {
        let endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connection = endpoint
            .connect_with(
                identity.client_config(owner.certificate()).unwrap(),
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
                &handshake::request(model.local(), model.formation().clone(), false).unwrap(),
                Phase::Handshake,
            )
            .await
            .unwrap();
        let mut binding = AuthenticatedSession::from_connection(
            &connection,
            SessionId(1),
            Generation(0),
            model.formation().clone(),
        )
        .unwrap();
        handshake::accept_reply(
            &reply,
            &mut binding,
            &model,
            Generation(0),
            model.formation().clone(),
            owner.fingerprint(),
        )
        .unwrap();
        Self {
            _endpoint: endpoint,
            connection,
            binding,
            model,
            seq: 100,
        }
    }

    fn encode(&mut self, body: OutboundBody, gossip: Vec<GossipDelta>) -> wire::Encoded {
        self.seq += 1;
        wire::encode(
            OutboundMessage {
                seq: self.seq,
                body,
                gossip,
            },
            self.model.formation().clone(),
            SenderIdentity::Admitted(self.model.local_id().clone()),
            None,
        )
        .unwrap()
    }

    fn answer_probe(&mut self, packet: &[u8]) {
        let decoded = self
            .binding
            .decode(packet, &self.model, Generation(0), Transport::Datagram)
            .unwrap();
        let probe = match &decoded.input.body {
            PeerBody::Ping { probe, .. } => Some(*probe),
            _ => None,
        };
        // Run actual core refutation semantics: startup may have probed before
        // this connection existed. A test peer must not remain falsely suspected
        // merely because its adapter omitted an Alive response.
        let previous = self.model.incarnation();
        self.model =
            orishu_membership::update(self.model.clone(), Message::Peer(decoded.input)).model;
        if self.model.incarnation() != previous {
            let alive = self.encode(
                OutboundBody::Announce {
                    announcement: Announcement::Alive,
                    target: self.model.local_id().clone(),
                    incarnation: self.model.incarnation(),
                },
                vec![],
            );
            self.connection.send_datagram(alive.bytes.into()).unwrap();
        }
        if let Some(probe) = probe {
            let ack = self.encode(
                OutboundBody::Ack {
                    probe,
                    incarnation: self.model.incarnation(),
                },
                vec![],
            );
            self.connection.send_datagram(ack.bytes.into()).unwrap();
        }
    }

    async fn next_pull(&mut self) -> (quinn::SendStream, u64) {
        loop {
            tokio::select! {
                packet = self.connection.read_datagram() => self.answer_probe(&packet.unwrap()),
                streams = self.connection.accept_bi() => {
                    let (send, mut receive) = streams.unwrap();
                    let bytes = read_payload(&mut receive).await.unwrap();
                    let decoded = self.binding.decode(&bytes, &self.model, Generation(0), Transport::Stream).unwrap();
                    let PeerBody::PullRequest { round, .. } = decoded.input.body else { panic!("expected actual reconciliation request"); };
                    return (send, round);
                }
            }
        }
    }

    async fn answer_pull(
        &mut self,
        mut send: quinn::SendStream,
        round: u64,
        include_records: bool,
    ) {
        let deltas = if include_records {
            self.model
                .members()
                .values()
                .cloned()
                .map(|member| GossipDelta {
                    hops: 0,
                    body: DeltaBody::MembershipUpdate(member),
                })
                .collect()
        } else {
            vec![]
        };
        let reply = self.encode(
            OutboundBody::PullReply {
                round,
                digest: MembershipTree::build(&self.model).digest(),
                deltas,
                complete: true,
                cursor: None,
            },
            vec![],
        );
        write_payload(&mut send, &reply.bytes).await.unwrap();
    }
}

#[tokio::test]
async fn leave_disposes_real_outgoing_pull_and_reclaims_exchange_capacity() {
    outgoing_pull_disposal(Disposal::Leave).await;
}

#[tokio::test]
async fn shutdown_disposes_real_outgoing_pull_and_reclaims_exchange_capacity() {
    outgoing_pull_disposal(Disposal::Shutdown).await;
}

#[tokio::test]
async fn wire_ejection_disposes_real_outgoing_pull_and_reclaims_exchange_capacity() {
    outgoing_pull_disposal(Disposal::Ejection).await;
}

enum Disposal {
    Leave,
    Shutdown,
    Ejection,
}

async fn outgoing_pull_disposal(disposal: Disposal) {
    tokio::time::timeout(Duration::from_secs(20), async {
        let identity = PeerIdentity::generate().unwrap();
        let peer_identity = PeerIdentity::generate().unwrap();
        let formation = testing::formation();
        let mut local = model(&identity, "send-owner", formation.clone());
        let mut remote = model(&peer_identity, "send-peer", formation);
        testing::insert_member(&mut remote, local.members()[local.local_id()].clone());
        testing::insert_member(&mut local, remote.members()[remote.local_id()].clone());
        let (owner, owner_task) = driver::spawn_standalone(local);
        let endpoint = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let address = endpoint.local_addr().unwrap();
        let dispatcher = server::spawn(endpoint, owner.clone());
        let mut peer = Peer::connect(address, &identity, &peer_identity, remote).await;
        // This is a production owner send task, not a test-owned pool request.
        // Keep the remote reply stream open until after lifecycle replacement.
        let (mut held_reply, _) = peer.next_pull().await;
        let pool = owner.exchange_pool();
        assert!(pool.available_exchanges() < 64);
        assert!(peer.connection.close_reason().is_none());
        let previous = owner.view().unwrap();
        if matches!(disposal, Disposal::Shutdown) {
            tokio::time::timeout(Duration::from_secs(1), async {
                owner.shutdown().await.unwrap();
                assert_eq!(owner_task.await.unwrap(), Ok(()));
                dispatcher.await.unwrap();
                peer.connection.closed().await;
                assert_eq!(pool.available_exchanges(), 64);
            })
            .await
            .expect("shutdown disposes the actual outgoing exchange without its reply");
            assert!(held_reply.write_all(b"reply-after-shutdown").await.is_err());
            assert_eq!(
                owner.counters().completed_exchanges,
                previous.completed_exchanges
            );
            assert_eq!(owner.counters().failed_sends, previous.failed_sends);
            return;
        }
        if matches!(disposal, Disposal::Ejection) {
            let removal = peer.encode(
                OutboundBody::Ping {
                    probe: ProbeId(9000),
                    incarnation: peer.model.incarnation(),
                },
                vec![GossipDelta {
                    hops: 0,
                    body: DeltaBody::TombstoneUpdate(orishu_membership::MembershipTombstone {
                        node_id: previous.summary.source_node_id.clone(),
                        name: "send-owner".parse().unwrap(),
                        cert_fingerprint: identity.fingerprint(),
                        removal_mode: orishu_membership::RemovalMode::Force,
                        version: testing::version(1_000_000),
                        cleared: false,
                        reason: None,
                    }),
                }],
            );
            assert_eq!(removal.deferred_gossip, 0);
            peer.connection.send_datagram(removal.bytes.into()).unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                peer.connection.closed().await;
                while pool.available_exchanges() != 64 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("wire ejection reclaims the real outgoing exchange");
            let ejected = owner.view().unwrap();
            assert_eq!(
                ejected.summary.participation,
                orishu::model::cluster::Participation::Ejected
            );
            assert_ne!(ejected.generation, previous.generation);
            assert_eq!(ejected.summary.formation_id, previous.summary.formation_id);
            assert_eq!(
                ejected.summary.source_node_id,
                previous.summary.source_node_id
            );
            assert!(!ejected.summary.introducer_ready);
            assert_eq!(ejected.completed_exchanges, previous.completed_exchanges);
            assert_eq!(ejected.failed_sends, previous.failed_sends);
            assert!(held_reply.write_all(b"reply-after-ejection").await.is_err());
            owner.shutdown().await.unwrap();
            assert_eq!(owner_task.await.unwrap(), Ok(()));
            dispatcher.await.unwrap();
            return;
        }
        owner
            .try_submit(
                previous.generation,
                Message::Local(Command::Leave {
                    replacement_formation: "send-replacement".parse().unwrap(),
                    replacement_node_id: "send-new-node".parse().unwrap(),
                    replacement_cluster_name: "send-new-label".parse().unwrap(),
                }),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            peer.connection.closed().await;
            while pool.available_exchanges() != 64
                || owner.view().unwrap().generation == previous.generation
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owner leave closes the old session and reclaims its outgoing exchange");
        assert!(held_reply.write_all(b"late-old-reply").await.is_err());
        let replacement = owner.view().unwrap();
        assert_eq!(
            replacement.summary.formation_id.as_str(),
            "send-replacement"
        );
        assert_eq!(replacement.summary.member_count, 1);
        assert_eq!(
            replacement.completed_exchanges,
            previous.completed_exchanges
        );
        assert_eq!(replacement.failed_sends, previous.failed_sends);
        assert!(owner.model_snapshot().await.anti_entropy().is_none());
        // Normal control still works after cancellation; the test has not
        // aborted the owner, its send tasks, dispatcher or remote stream.
        owner
            .set_membership_lock(
                replacement.generation,
                replacement.summary.formation_id,
                true,
            )
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        owner.shutdown().await.unwrap();
        assert_eq!(owner_task.await.unwrap(), Ok(()));
        dispatcher.await.unwrap();
        assert_eq!(pool.available_exchanges(), 64);
    })
    .await
    .expect("real outgoing exchange lifecycle fixture deadline");
}

#[tokio::test]
async fn departed_reconciliation_round_can_delay_learning_a_readmitted_identity() {
    tokio::time::timeout(Duration::from_secs(40), async {
        let identity = PeerIdentity::generate().unwrap();
        let departing_identity = PeerIdentity::generate().unwrap();
        let surviving_identity = PeerIdentity::generate().unwrap();
        let formation = testing::formation();
        let mut receiver = model(&identity, "timing-a", formation.clone());
        let mut departing = model(&departing_identity, "timing-c", formation.clone());
        let mut surviving = model(&surviving_identity, "timing-b", formation.clone());
        let old_id = departing.local_id().clone();
        let old_record = departing.members()[&old_id].clone();
        let b_record = surviving.members()[surviving.local_id()].clone();
        testing::insert_member(
            &mut departing,
            receiver.members()[receiver.local_id()].clone(),
        );
        testing::insert_member(
            &mut surviving,
            receiver.members()[receiver.local_id()].clone(),
        );
        testing::insert_member(&mut receiver, old_record.clone());
        // Setup models are already admitted, initially with a partial A view.
        // Wire gossip establishes all three old records before the departure.
        let (owner, owner_task) = driver::spawn_standalone(receiver);
        let endpoint = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let address = endpoint.local_addr().unwrap();
        let dispatcher = server::spawn(endpoint, owner.clone());
        let mut c = Peer::connect(address, &identity, &departing_identity, departing).await;

        // Clear any startup missing-route round by completing a real pull.
        let (send, round) = c.next_pull().await;
        c.answer_pull(send, round, false).await;
        while owner.model_snapshot().await.anti_entropy().is_some() {
            tokio::task::yield_now().await;
        }
        // Place the controlled round one second after the five-second cadence,
        // rather than relying on timer/tick ordering at the same instant.
        let start = Instant::now() + Duration::from_secs(1);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(start) => break,
                packet = c.connection.read_datagram() => c.answer_probe(&packet.unwrap()),
            }
        }
        owner
            .try_submit(
                Generation(0),
                Message::Local(Command::StartAntiEntropyRound),
            )
            .unwrap();
        let (held_reply, held_round) = c.next_pull().await;
        assert_eq!(
            owner.model_snapshot().await.anti_entropy().unwrap().round,
            held_round
        );
        let introduce_b = c.encode(
            OutboundBody::Ping {
                probe: ProbeId(9000),
                incarnation: c.model.incarnation(),
            },
            vec![GossipDelta {
                hops: 0,
                body: DeltaBody::MembershipUpdate(b_record),
            }],
        );
        c.connection
            .send_datagram(introduce_b.bytes.into())
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let bytes = c.connection.read_datagram().await.unwrap();
                let decoded = c
                    .binding
                    .decode(&bytes, &c.model, Generation(0), Transport::Datagram)
                    .unwrap();
                if matches!(
                    decoded.input.body,
                    PeerBody::Ack {
                        probe: ProbeId(9000),
                        ..
                    }
                ) {
                    break;
                }
                c.answer_probe(&bytes);
            }
        })
        .await
        .expect("B is learned before departure through correlated real wire traffic");
        let before_leave = owner.model_snapshot().await;
        assert_eq!(before_leave.members().len(), 3);
        assert!(
            before_leave
                .members()
                .values()
                .all(|member| member.liveness == Liveness::Alive)
        );
        assert_eq!(before_leave.anti_entropy().unwrap().round, held_round);
        let leave = c.encode(
            OutboundBody::Announce {
                announcement: Announcement::Leave,
                target: old_id.clone(),
                incarnation: c.model.incarnation(),
            },
            vec![],
        );
        c.connection.send_datagram(leave.bytes.into()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), c.connection.closed())
            .await
            .expect("owner retires departed session");
        let after_leave = owner.model_snapshot().await;
        assert_eq!(after_leave.members()[&old_id].liveness, Liveness::Dead);
        assert_eq!(after_leave.anti_entropy().unwrap().round, held_round);

        let mut dead = old_record.clone();
        dead.liveness = Liveness::Dead;
        testing::insert_member(&mut surviving, dead);
        let mut readmitted = old_record;
        readmitted.id = "timing-c-readmitted".parse().unwrap();
        readmitted.version.actor = readmitted.id.clone();
        let new_id = readmitted.id.clone();
        testing::insert_member(&mut surviving, readmitted);
        let mut b = Peer::connect(address, &identity, &surviving_identity, surviving).await;
        let observing = Instant::now();
        assert!(
            tokio::time::timeout(Duration::from_secs(10), b.next_pull())
                .await
                .is_err(),
            "candidate delay did not reproduce the ten-second missing-record window"
        );
        let delayed = owner.model_snapshot().await;
        assert_eq!(delayed.members().len(), 3);
        assert!(!delayed.members().contains_key(&new_id));
        // The unchanged ten-second round timeout plus the next five-second
        // cadence permits repair; this is observation, not a larger pass budget
        // for the separate public process journey.
        let (send, round) = tokio::time::timeout(Duration::from_secs(7), b.next_pull())
            .await
            .expect("next reconciliation is bounded");
        assert_ne!(round, held_round);
        b.answer_pull(send, round, true).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while owner.view().unwrap().summary.member_count != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let repaired = owner.model_snapshot().await;
        assert_eq!(
            repaired.members()[&new_id].cert_fingerprint,
            departing_identity.fingerprint()
        );
        assert_eq!(repaired.members()[&new_id].liveness, Liveness::Alive);
        assert_eq!(repaired.members()[&old_id].liveness, Liveness::Dead);
        assert_eq!(owner.view().unwrap().summary.alive_count, 3);
        assert_eq!(repaired.formation(), &formation);
        eprintln!(
            "controlled reconciliation repair after {:?}",
            observing.elapsed()
        );
        drop(held_reply);
        owner.shutdown().await.unwrap();
        assert_eq!(owner_task.await.unwrap(), Ok(()));
        dispatcher.await.unwrap();
    })
    .await
    .expect("controlled departure/reconciliation fixture deadline");
}
