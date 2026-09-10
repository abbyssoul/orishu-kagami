use super::*;
use crate::{
    driver::Generation,
    peer::{
        exchange::{ExchangePool, Phase},
        handshake,
        session::AuthenticatedSession,
        tls::{PeerIdentity, SERVER_NAME},
    },
};
use orishu_membership::{
    Incarnation, Membership, OutboundBody, OutboundMessage, PeerBody, ProbeId, SenderIdentity,
    testing,
};
use std::{sync::Arc, time::Duration};

fn packet(bytes: Vec<u8>, transport: wire::Transport) -> wire::Encoded {
    wire::Encoded {
        bytes,
        transport,
        deferred_gossip: 0,
        deferred: Vec::new(),
    }
}

// Fixture-only limits make the application's 1200-byte and 16-task gates
// reachable independently of the stricter default transport credit/MTU.
fn limits(datagrams: bool) -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config
        .initial_mtu(1400)
        .min_mtu(1400)
        .mtu_discovery_config(None)
        .max_concurrent_bidi_streams(32_u32.into())
        .datagram_receive_buffer_size(datagrams.then_some(64 * 1400));
    Arc::new(config)
}

#[tokio::test]
async fn real_datagram_submissions_distinguish_refusal_failure_and_local_success() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for supported in [true, false] {
            let identity = PeerIdentity::generate().unwrap();
            let applicant = PeerIdentity::generate().unwrap();
            let mut server_config = identity.server_config().unwrap();
            server_config.transport_config(limits(supported));
            let server =
                quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let mut config = applicant.client_config(identity.certificate()).unwrap();
            config.transport_config(limits(true));
            let connecting = client
                .connect_with(config, server.local_addr().unwrap(), SERVER_NAME)
                .unwrap();
            let (outgoing, incoming) =
                tokio::join!(connecting, async { server.accept().await.unwrap().await });
            let outgoing = outgoing.unwrap();
            let incoming = incoming.unwrap();
            let traffic = Traffic::enabled();
            for value in [
                packet(vec![0xf5], wire::Transport::Stream),
                packet(
                    vec![0; wire::MAX_DATAGRAM_BYTES + 1],
                    wire::Transport::Datagram,
                ),
            ] {
                assert!(traffic.submit(&outgoing, value).is_err());
            }
            let result = traffic.submit(&outgoing, packet(vec![0xf5], wire::Transport::Datagram));
            if supported {
                result.unwrap();
                assert_eq!(incoming.read_datagram().await.unwrap().as_ref(), &[0xf5]);
                outgoing.close(0_u32.into(), b"fixture close");
                outgoing.closed().await;
                assert!(outgoing.max_datagram_size().is_some());
                assert!(
                    traffic
                        .submit(&outgoing, packet(vec![0xf4], wire::Transport::Datagram))
                        .is_err()
                );
            } else {
                assert!(result.is_err());
                assert!(outgoing.max_datagram_size().is_none());
            }
            let snapshot = traffic.snapshot().unwrap();
            for event in Event::ALL {
                let expected = match event {
                    Event::DatagramSubmitted
                    | Event::DatagramBytesSubmitted
                    | Event::DatagramSubmitFailed => u64::from(supported),
                    Event::DatagramSubmitRefused => {
                        if supported {
                            2
                        } else {
                            3
                        }
                    }
                    _ => 0,
                };
                assert_eq!(snapshot.get(event), expected, "{event:?}");
            }
            client.close(0_u32.into(), b"fixture complete");
            server.close(0_u32.into(), b"fixture complete");
        }
    })
    .await
    .expect("bounded real datagram submission outcomes");
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

async fn wait_for(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !check() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("bounded adapter observation");
}

#[tokio::test]
async fn registered_io_counts_hostile_datagrams_and_pre_pool_stream_refusal() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let identity = PeerIdentity::generate().unwrap();
        let applicant = PeerIdentity::generate().unwrap();
        let mut receiver = model(&identity, "receiver");
        let initial = model(&applicant, "sender");
        let mut sender = Membership::standalone(
            receiver.formation().clone(),
            receiver.cluster_name().clone(),
            initial.local().clone(),
            Default::default(),
            Default::default(),
        )
        .unwrap();
        testing::insert_member(&mut receiver, sender.members()[sender.local_id()].clone());
        testing::insert_member(&mut sender, receiver.members()[receiver.local_id()].clone());
        let traffic = Traffic::enabled();
        let (owner, owner_task) = crate::driver::spawn_standalone_observed(
            receiver,
            None,
            crate::peer::exchange_metrics::ExchangeMetrics::enabled(),
            traffic.clone(),
            Default::default(),
        );
        let pool = owner.exchange_pool();
        let mut config = identity.server_config().unwrap();
        config.transport_config(limits(true));
        let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let address = endpoint.local_addr().unwrap();
        let dispatcher = crate::peer::server::spawn(endpoint, owner.clone());
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut config = applicant.client_config(identity.certificate()).unwrap();
        config.transport_config(limits(true));
        let connection = client
            .connect_with(config, address, SERVER_NAME)
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
            orishu_membership::SessionId(1),
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

        // Raw adversarial writes intentionally bypass the sender's membership
        // cap, not the receiver's authenticated production IO/owner boundaries.
        assert!(connection.max_datagram_size().unwrap() > wire::MAX_DATAGRAM_BYTES);
        connection
            .send_datagram(vec![0; wire::MAX_DATAGRAM_BYTES + 1].into())
            .unwrap();
        wait_for(|| traffic.snapshot().unwrap().get(Event::DatagramOversized) == 1).await;
        assert_eq!(owner.view().unwrap().peer_decode_rejections, 0);
        connection.send_datagram(vec![0xf5].into()).unwrap();
        wait_for(|| owner.view().unwrap().peer_decode_rejections == 1).await;
        let ping = wire::encode(
            OutboundMessage {
                seq: 10,
                gossip: vec![],
                body: OutboundBody::Ping {
                    probe: ProbeId(100),
                    incarnation: Incarnation(0),
                },
            },
            sender.formation().clone(),
            SenderIdentity::Admitted(sender.local_id().clone()),
            None,
        )
        .unwrap();
        let ping_bytes = ping.bytes.len();
        Traffic::default().submit(&connection, ping).unwrap();
        let ack = connection.read_datagram().await.unwrap();
        assert!(matches!(
            binding
                .decode(&ack, &sender, Generation(0), wire::Transport::Datagram)
                .unwrap()
                .input
                .body,
            PeerBody::Ack {
                probe: ProbeId(100),
                ..
            }
        ));
        let snapshot = traffic.snapshot().unwrap();
        assert_eq!(snapshot.get(Event::DatagramReceived), 3);
        assert_eq!(
            snapshot.get(Event::DatagramBytesReceived),
            (wire::MAX_DATAGRAM_BYTES + 2 + ping_bytes) as u64
        );
        assert_eq!(snapshot.get(Event::DatagramSubmitted), 1);
        assert_eq!(
            snapshot.get(Event::DatagramBytesSubmitted),
            ack.len() as u64
        );

        let mut held = Vec::new();
        for _ in 0..16 {
            let (mut send, receive) = connection.open_bi().await.unwrap();
            send.write_all(&[0, 0, 16, 0, 0xa0]).await.unwrap();
            held.push((send, receive));
        }
        wait_for(|| pool.available_exchanges() == 48).await;
        let (mut extra, mut reply) = connection.open_bi().await.unwrap();
        extra.write_all(&[0, 0, 0, 1, 0xf5]).await.unwrap();
        extra.finish().unwrap();
        assert!(reply.read_to_end(64).await.is_err());
        wait_for(|| {
            traffic
                .snapshot()
                .unwrap()
                .get(Event::StreamCapacityRefused)
                == 1
        })
        .await;
        assert_eq!(
            pool.available_exchanges(),
            48,
            "refusal must precede pool admission"
        );
        use crate::peer::exchange_metrics::{ExchangeOutcome, ExchangeRole};
        let reliable = pool.counters().unwrap();
        for outcome in ExchangeOutcome::ALL {
            assert_eq!(
                reliable.role(ExchangeRole::Serve).outcome(outcome),
                u64::from(outcome == ExchangeOutcome::Completed),
                "only the initial handshake has terminated; pre-pool refusal is separate"
            );
        }
        for locked in [true, false] {
            owner
                .set_membership_lock(Generation(0), sender.formation().clone(), locked)
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            assert_eq!(owner.view().unwrap().summary.membership_locked, locked);
        }
        let (mut released_send, mut released_receive) = held.pop().unwrap();
        released_send.reset(0_u32.into()).unwrap();
        released_receive.stop(0_u32.into()).unwrap();
        wait_for(|| pool.available_exchanges() == 49).await;
        let pull = wire::encode(
            OutboundMessage {
                seq: 20,
                gossip: vec![],
                body: OutboundBody::PullRequest {
                    round: 2,
                    digest: orishu_membership::antientropy::MembershipTree::build(&sender).digest(),
                    buckets: vec![],
                    cursor: None,
                },
            },
            sender.formation().clone(),
            SenderIdentity::Admitted(sender.local_id().clone()),
            None,
        )
        .unwrap();
        let reply = ExchangePool::new(1)
            .unwrap()
            .request(&connection, &pull.bytes, Phase::Membership)
            .await
            .unwrap();
        assert!(matches!(
            binding
                .decode(&reply, &sender, Generation(0), wire::Transport::Stream)
                .unwrap()
                .input
                .body,
            PeerBody::PullReply { round: 2, .. }
        ));
        assert_eq!(
            traffic
                .snapshot()
                .unwrap()
                .get(Event::StreamCapacityRefused),
            1
        );
        // No periodic probe/anti-entropy maintenance runs in this fixture.
        // The accepted leave therefore accounts for exactly one submission to
        // the sole peer, not unrelated traffic between two process scrapes.
        let before_leave = traffic.snapshot().unwrap();
        let leave = orishu::model::cluster::LeaveRequest {
            schema_version: 1,
            operation_id: "traffic-departure".parse().unwrap(),
            formation_id: sender.formation().clone(),
        };
        let receipt = owner
            .leave_operation(leave.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(receipt.changed);
        assert_ne!(receipt.current.formation_id, *sender.formation());
        let after_leave = traffic.snapshot().unwrap();
        assert_eq!(
            after_leave.get(Event::DatagramSubmitted),
            before_leave.get(Event::DatagramSubmitted) + 1
        );
        assert!(
            after_leave.get(Event::DatagramBytesSubmitted)
                > before_leave.get(Event::DatagramBytesSubmitted)
        );
        assert_eq!(
            owner
                .leave_operation(leave)
                .unwrap()
                .await
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(
            traffic.snapshot().unwrap().get(Event::DatagramSubmitted),
            after_leave.get(Event::DatagramSubmitted)
        );
        connection.closed().await;
        wait_for(|| pool.available_exchanges() == 64).await;
        owner.shutdown().await.unwrap();
        assert_eq!(owner_task.await.unwrap(), Ok(()));
        dispatcher.await.unwrap();
        connection.closed().await;
        assert_eq!(pool.available_exchanges(), 64);
        // Keep incomplete streams through server-driven leave/shutdown cleanup.
        drop(held);
        client.close(0_u32.into(), b"fixture complete");
    })
    .await
    .expect("bounded registered IO, capacity reuse, control and shutdown");
}
