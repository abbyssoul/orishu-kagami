//! Bounded receiver IO for one complete admission-state transfer. Completion
//! carries private shell credentials, not permission to become an introducer.

use super::{
    Receiver,
    wire::{Action, Outcome, Rejection, Reply, Request},
};
use crate::{
    credentials::SecretToken,
    driver::Generation,
    peer::{
        codec,
        exchange::{ExchangePool, Phase},
        session::AuthenticatedSession,
    },
};
use orishu::model::cluster::OperationId;
use orishu_membership::{AdmissionBaseline, CertFingerprint, FormationId, NodeId, SessionId};
use std::time::Duration;

/// Owner-selected source on a registered member session. A later owner turn
/// must recheck generation/session/formation before adopting the result.
pub struct Binding {
    /// Target formation already adopted by the receiving worker.
    pub formation: FormationId,
    /// Assigned identity of the baseline source.
    pub source: NodeId,
    /// Source certificate held in current membership.
    pub fingerprint: CertFingerprint,
    /// This worker's assigned target-formation identity.
    pub requester: NodeId,
    /// Identifies this read attempt; retries in a new attempt need a new ID.
    pub request: OperationId,
}

/// Verified bytes and separately held target credential. Deliberately lacks
/// Debug/Clone/Serialize; only the receiving owner may consume its contents.
pub struct Completed {
    baseline: AdmissionBaseline,
    token: SecretToken,
}
impl Completed {
    /// Transfer to the serialized owner for revalidation and atomic merge.
    pub fn into_parts(self) -> (AdmissionBaseline, SecretToken) {
        (self.baseline, self.token)
    }
}

/// Redacted failure categories; no returned page, token or remote error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Current peer certificate/profile does not match the selected source.
    #[error("catch-up source binding is invalid")]
    Binding,
    /// Transport failed, timed out or refused the shared IO budget.
    #[error("catch-up exchange unavailable")]
    Transport,
    /// Total transfer time, including all pages and confirmation, expired.
    #[error("catch-up transfer deadline exceeded")]
    Timeout,
    /// Correlation, ordering, completeness or credential binding failed.
    #[error("catch-up response is invalid")]
    Invalid,
    /// Structured source refusal; no arbitrary remote diagnostic payload.
    #[error("catch-up request rejected: {0:?}")]
    Rejected(Rejection),
}

/// Instrument the real receiver result once, without exposing its payload or
/// changing the existing error/adoption contract. Owner jobs use this wrapper.
pub(crate) async fn fetch_observed(
    connection: &quinn::Connection,
    binding: &Binding,
    pool: &ExchangePool,
    observation: &mut crate::formation_metrics::CatchupObservation,
) -> Result<Completed, Error> {
    let result = fetch(connection, binding, pool).await;
    observation.fetched(result.as_ref().map(|_| ()).map_err(|error| *error));
    result
}

/// Fetch sequentially under a 25-second total budget and the shared five-second
/// per-exchange deadline. At most 161 bounded pages plus begin/confirm requests;
/// no retry loop, DNS, new connection or unbounded permit wait is introduced.
pub async fn fetch(
    connection: &quinn::Connection,
    binding: &Binding,
    pool: &ExchangePool,
) -> Result<Completed, Error> {
    let facts = AuthenticatedSession::from_connection(
        connection,
        SessionId(0),
        Generation(0),
        binding.formation.clone(),
    )
    .map_err(|_| Error::Binding)?;
    if facts.fingerprint() != binding.fingerprint || binding.source == binding.requester {
        return Err(Error::Binding);
    }
    tokio::time::timeout(Duration::from_secs(25), async {
        let Outcome::Baseline { descriptor } =
            exchange(connection, binding, pool, Action::Begin).await?
        else {
            return Err(Error::Invalid);
        };
        if descriptor.formation != binding.formation || descriptor.source != binding.source {
            return Err(Error::Binding);
        }
        let mut receiver = Receiver::new(descriptor.clone()).map_err(|_| Error::Invalid)?;
        for index in 0..descriptor.pages {
            let Outcome::Page { page } = exchange(
                connection,
                binding,
                pool,
                Action::Page {
                    snapshot: descriptor.snapshot,
                    index,
                },
            )
            .await?
            else {
                return Err(Error::Invalid);
            };
            receiver = receiver
                .push(&codec::encode(&page).map_err(|_| Error::Invalid)?)
                .map_err(|_| Error::Invalid)?;
        }
        // Never request the secret if the public content proof is incomplete.
        let baseline = receiver.finish_baseline().map_err(|_| Error::Invalid)?;
        let Outcome::Credential {
            snapshot,
            root,
            token,
        } = exchange(
            connection,
            binding,
            pool,
            Action::Confirm {
                snapshot: descriptor.snapshot,
                root: descriptor.root,
            },
        )
        .await?
        else {
            return Err(Error::Invalid);
        };
        if snapshot != descriptor.snapshot || root != descriptor.root {
            return Err(Error::Invalid);
        }
        let token = SecretToken::parse(token.expose().to_owned()).map_err(|_| Error::Invalid)?;
        Ok(Completed { baseline, token })
    })
    .await
    .map_err(|_| Error::Timeout)?
}

async fn exchange(
    connection: &quinn::Connection,
    binding: &Binding,
    pool: &ExchangePool,
    action: Action,
) -> Result<Outcome, Error> {
    let request = Request::new(
        binding.formation.clone(),
        binding.requester.clone(),
        binding.request.clone(),
        action,
    )
    .encode()
    .map_err(|_| Error::Invalid)?;
    let bytes = pool
        .request(connection, &request, Phase::AdmissionState)
        .await
        .map_err(|_| Error::Transport)?;
    let outcome = Reply::decode(
        &bytes,
        &binding.formation,
        &binding.source,
        &binding.request,
    )
    .map_err(|_| Error::Invalid)?
    .outcome;
    match outcome {
        Outcome::Rejected { reason } => Err(Error::Rejected(reason)),
        outcome => Ok(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{
        catchup::{store::Store, wire},
        tls::{PeerIdentity, SERVER_NAME},
    };
    use orishu_membership::{Command, Membership, Message, testing};

    #[derive(Clone, Copy)]
    enum TransferCase {
        ExpiredContinuation,
        TruncatedFrame,
        WrongSnapshot,
        WrongDigest,
        ChangingSource,
        WholeDeadline,
        CancelledPage,
    }

    #[tokio::test]
    async fn expired_continuation_refuses_partial_baseline_and_allows_retry() {
        exercise_transfer(TransferCase::ExpiredContinuation).await;
    }

    #[tokio::test]
    async fn truncated_page_frame_releases_exchange_capacity_for_retry() {
        exercise_transfer(TransferCase::TruncatedFrame).await;
    }

    #[tokio::test]
    async fn cross_snapshot_page_refuses_partial_baseline_and_allows_retry() {
        exercise_transfer(TransferCase::WrongSnapshot).await;
    }

    #[tokio::test]
    async fn incorrect_digest_never_requests_credential_and_allows_retry() {
        exercise_transfer(TransferCase::WrongDigest).await;
    }

    #[tokio::test]
    async fn source_changes_between_pages_preserve_frozen_transfer_and_refresh_next_baseline() {
        exercise_transfer(TransferCase::ChangingSource).await;
    }

    #[tokio::test]
    async fn whole_transfer_deadline_with_progress_is_one_outcome_and_allows_retry() {
        exercise_transfer(TransferCase::WholeDeadline).await;
    }

    #[tokio::test]
    async fn cancelling_an_incomplete_page_records_cancellation_and_releases_the_exchange() {
        exercise_transfer(TransferCase::CancelledPage).await;
    }

    async fn exercise_transfer(case: TransferCase) {
        let slow = matches!(case, TransferCase::WholeDeadline);
        let records = if slow { 320 } else { 40 };
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let fixture = testing::standalone("baseline-source");
        let mut local = fixture.local().clone();
        local.cert_fingerprint = server_identity.fingerprint();
        let mut model = Membership::standalone(
            fixture.formation().clone(),
            fixture.cluster_name().clone(),
            local,
            fixture.policy().clone(),
            fixture.limits().clone(),
        )
        .unwrap();
        let mut requester = testing::member("receiver", 1);
        requester.cert_fingerprint = client_identity.fingerprint();
        let requester_id = requester.id.clone();
        testing::insert_member(&mut model, requester);
        for index in 0..records {
            model = orishu_membership::update(
                model,
                Message::Local(Command::UpdateBlocklist(testing::blocklist_entry(
                    &format!("blocked-{index:02}"),
                ))),
            )
            .model;
        }
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connecting = client
            .connect_with(
                client_identity
                    .client_config(server_identity.certificate())
                    .unwrap(),
                server.local_addr().unwrap(),
                SERVER_NAME,
            )
            .unwrap();
        let (outgoing, incoming) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(connecting, async { server.accept().await.unwrap().await })
        })
        .await
        .unwrap();
        let outgoing = outgoing.unwrap();
        let incoming = incoming.unwrap();
        let facts = AuthenticatedSession::from_connection(
            &incoming,
            SessionId(1),
            Generation(0),
            model.formation().clone(),
        )
        .unwrap();
        assert_eq!(facts.fingerprint(), client_identity.fingerprint());
        let peer = testing::peer_context(&model, &requester_id, 1);
        let token = SecretToken::generate().unwrap();
        let pool = ExchangePool::new(1).unwrap();
        let mut binding = Binding {
            formation: model.formation().clone(),
            source: model.local_id().clone(),
            fingerprint: server_identity.fingerprint(),
            requester: requester_id,
            request: "partial-baseline".parse().unwrap(),
        };
        let (cancel_started, cancel_ready) = tokio::sync::oneshot::channel();
        let (cancel_released, cancel_release) = tokio::sync::oneshot::channel();
        let receiver = async {
            #[cfg(feature = "observability")]
            let metrics = crate::formation_metrics::FormationMetrics::enabled();
            #[cfg(not(feature = "observability"))]
            let metrics = crate::formation_metrics::FormationMetrics::default();
            let fingerprint = binding.fingerprint;
            binding.fingerprint = client_identity.fingerprint();
            let mut observation = metrics.start_catchup();
            assert!(matches!(
                fetch_observed(&outgoing, &binding, &pool, &mut observation).await,
                Err(Error::Binding)
            ));
            drop(observation);
            binding.fingerprint = fingerprint;
            let mut observation = metrics.start_catchup();
            let first = if matches!(case, TransferCase::CancelledPage) {
                let mut fetch =
                    Box::pin(fetch_observed(&outgoing, &binding, &pool, &mut observation));
                tokio::select! {
                    _ = &mut fetch => panic!("incomplete page must not finish before cancellation"),
                    ready = cancel_ready => ready.unwrap(),
                }
                assert_eq!(pool.available_exchanges(), 0);
                drop(fetch);
                assert_eq!(pool.available_exchanges(), 1);
                cancel_released.send(()).unwrap();
                None
            } else {
                Some(fetch_observed(&outgoing, &binding, &pool, &mut observation).await)
            };
            drop(observation);
            match case {
                TransferCase::CancelledPage => assert!(first.is_none()),
                TransferCase::WholeDeadline => assert!(matches!(first, Some(Err(Error::Timeout)))),
                TransferCase::ExpiredContinuation => assert!(matches!(
                    first,
                    Some(Err(Error::Rejected(Rejection::Unavailable)))
                )),
                TransferCase::TruncatedFrame => {
                    assert!(matches!(first, Some(Err(Error::Transport))))
                }
                TransferCase::WrongSnapshot | TransferCase::WrongDigest => {
                    assert!(matches!(first, Some(Err(Error::Invalid))))
                }
                TransferCase::ChangingSource => {
                    let (baseline, received_token) = first.unwrap().unwrap().into_parts();
                    assert_eq!(baseline.blocklist.len(), 40);
                    assert!(baseline.policy.is_none(), "captured unlocked policy");
                    assert!(token.matches(received_token.expose()));
                }
            }
            binding.request = "retry-baseline".parse().unwrap();
            // One shared slot and the same connection: a leaked failed exchange
            // or retained partial receiver would prevent this real retry.
            let mut observation = metrics.start_catchup();
            let (baseline, received_token) =
                fetch_observed(&outgoing, &binding, &pool, &mut observation)
                    .await
                    .unwrap()
                    .into_parts();
            drop(observation);
            #[cfg(feature = "observability")]
            {
                use crate::formation_metrics::CatchupEvent as E;
                let snapshot = metrics.snapshot().unwrap();
                let outcome = match case {
                    TransferCase::ExpiredContinuation => E::TransferRejected,
                    TransferCase::TruncatedFrame => E::TransferUnavailable,
                    TransferCase::WrongSnapshot | TransferCase::WrongDigest => E::TransferInvalid,
                    TransferCase::ChangingSource => E::TransferValidated,
                    TransferCase::WholeDeadline => E::TransferTimedOut,
                    TransferCase::CancelledPage => E::TransferCancelled,
                };
                for event in E::ALL {
                    let expected = match event {
                        E::Started | E::OwnerAbandoned => 3,
                        E::TransferBindingRejected => 1,
                        _ => u64::from(event == E::TransferValidated) + u64::from(event == outcome),
                    };
                    assert_eq!(snapshot.get_catchup(event), expected, "{event:?}");
                }
            }
            if matches!(case, TransferCase::ChangingSource) {
                assert_eq!(baseline.blocklist.len(), 41);
                assert!(baseline.policy.as_ref().unwrap().locked);
            } else {
                assert_eq!(baseline.blocklist.len(), records);
            }
            assert_eq!(baseline.formation_id, *model.formation());
            assert!(token.matches(received_token.expose()));
        };
        let sender = async {
            let mut cancel_started = Some(cancel_started);
            let mut cancel_release = Some(cancel_release);
            let started = tokio::time::Instant::now();
            let mut store = Store::default();
            let mut source_time = std::time::Instant::now();
            let mut source_model = model.clone();
            let mut first_descriptor: Option<crate::peer::catchup::Descriptor> = None;
            // Failed transfer: Begin + Page 0 + Page 1, never Confirm.
            // A changing source still allows all four requests to complete.
            // Fresh transfer: Begin + both pages + Confirm.
            let first_requests = if matches!(case, TransferCase::ChangingSource) {
                4
            } else {
                3
            };
            for index in 0..if slow { 32 } else { first_requests + 4 } {
                let (mut send, mut receive) = incoming.accept_bi().await.unwrap();
                let bytes = crate::peer::read_payload_bounded(&mut receive, 4096)
                    .await
                    .unwrap();
                let request: Request = codec::decode(&bytes).unwrap();
                let initial = if slow {
                    request.request_id == "partial-baseline".parse().unwrap()
                } else {
                    index < first_requests
                };
                if initial {
                    assert_eq!(request.request_id, "partial-baseline".parse().unwrap());
                    if !matches!(case, TransferCase::ChangingSource) {
                        assert!(
                            !matches!(request.action, Action::Confirm { .. }),
                            "partial/invalid content must not request a credential"
                        );
                    }
                } else {
                    assert_eq!(request.request_id, "retry-baseline".parse().unwrap());
                }
                let terminal = !initial && matches!(request.action, Action::Confirm { .. });
                if slow && initial && matches!(request.action, Action::Page { .. }) {
                    // Every exchange makes progress within its five-second
                    // limit, but the complete transfer exceeds 25s.
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                if index == 2 && matches!(case, TransferCase::ExpiredContinuation) {
                    // Controlled source clock at the real retention/wire seam;
                    // no 30-second wall-clock sleep or fabricated rejection.
                    source_time += Duration::from_secs(31);
                }
                if index == 2 && matches!(case, TransferCase::ChangingSource) {
                    // Page 0 has crossed the real wire. Change the source
                    // through domain commands before it serves Page 1.
                    source_model = orishu_membership::update(
                        source_model,
                        Message::Local(Command::SetMembershipLock(true)),
                    )
                    .model;
                    source_model = orishu_membership::update(
                        source_model,
                        Message::Local(Command::UpdateBlocklist(testing::blocklist_entry(
                            "new-block",
                        ))),
                    )
                    .model;
                }
                let mut encoded = wire::serve(
                    &mut store,
                    wire::Source {
                        model: &source_model,
                        generation: Generation(0),
                        peer: &peer,
                        ready: true,
                        token: Some(&token),
                    },
                    &bytes,
                    source_time,
                )
                .unwrap();
                let mut reply = Reply::decode(
                    &encoded,
                    model.formation(),
                    model.local_id(),
                    &request.request_id,
                )
                .unwrap();
                if let Outcome::Baseline { descriptor } = &mut reply.outcome {
                    if slow {
                        assert!(
                            descriptor.pages >= 9,
                            "at least nine 3s pages must exceed the whole-transfer budget"
                        );
                    } else {
                        assert_eq!(
                            descriptor.pages, 2,
                            "exercise a partial multi-page baseline"
                        );
                    }
                    if matches!(case, TransferCase::ChangingSource) {
                        if let Some(first) = &first_descriptor {
                            assert!(descriptor.snapshot > first.snapshot);
                            assert_ne!(descriptor.root, first.root);
                        } else {
                            first_descriptor = Some(descriptor.clone());
                        }
                    }
                    if index == 0 && matches!(case, TransferCase::WrongDigest) {
                        descriptor.root[0] ^= 1;
                        encoded = codec::encode(&reply).unwrap();
                    }
                }
                if index == 2 && matches!(case, TransferCase::WrongSnapshot) {
                    let Outcome::Page { page } = &mut reply.outcome else {
                        panic!("second page expected");
                    };
                    page.snapshot += 1;
                    encoded = codec::encode(&reply).unwrap();
                }
                if index == 2 && matches!(case, TransferCase::CancelledPage) {
                    send.write_all(&(encoded.len() as u32).to_be_bytes())
                        .await
                        .unwrap();
                    send.write_all(&encoded[..1]).await.unwrap();
                    cancel_started.take().unwrap().send(()).unwrap();
                    tokio::time::timeout(Duration::from_secs(2), cancel_release.take().unwrap())
                        .await
                        .expect("receiver cancels while response remains incomplete")
                        .unwrap();
                } else if index == 2 && matches!(case, TransferCase::TruncatedFrame) {
                    send.write_all(&(encoded.len() as u32).to_be_bytes())
                        .await
                        .unwrap();
                    send.write_all(&encoded[..encoded.len() - 1]).await.unwrap();
                    send.finish().unwrap();
                } else if slow && initial {
                    let sent = crate::peer::write_payload(&mut send, &encoded).await;
                    if sent.is_err() {
                        assert!(
                            started.elapsed() >= Duration::from_secs(25),
                            "only the expired transfer may reset a response"
                        );
                    }
                } else {
                    crate::peer::write_payload(&mut send, &encoded)
                        .await
                        .unwrap();
                }
                if terminal {
                    break;
                }
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(if slow { 35 } else { 10 }), async {
            tokio::join!(receiver, sender);
        })
        .await;
        client.close(0_u32.into(), b"test complete");
        server.close(0_u32.into(), b"test complete");
        result.expect("both transfers finish within the bounded test deadline");
    }
}
