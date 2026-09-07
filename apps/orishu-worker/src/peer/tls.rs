//! Certificate-pinned QUIC with mandatory proof of client key possession.
//! An unknown authenticated certificate is an applicant, not an admitted node.

use std::sync::Arc;
use std::time::Duration;

use orishu_membership::CertFingerprint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, RootCertStore, SignatureScheme};
use sha2::{Digest, Sha256};

/// Negotiated profile; excludes historical policy-unaware peers.
pub const ALPN: &[u8] = b"orishu-membership/4";
/// Certificate DNS identity, independent of display labels and dial addresses.
pub const SERVER_NAME: &str = "orishu-worker";
const MAX_CERTIFICATE_BYTES: usize = 16_384;

/// Errors contain no key material or peer payloads.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// A cryptographic configuration or certificate was invalid.
    #[error("invalid peer TLS configuration: {0}")]
    Tls(#[from] rustls::Error),
    /// Certificate generation failed.
    #[error("peer certificate generation failed: {0}")]
    Generation(#[from] rcgen::Error),
    /// The requested profile cannot be provided by this crypto configuration.
    #[error("unsupported peer TLS configuration")]
    Configuration,
}

/// Private certificate identity. Deliberately does not implement Debug/serde.
pub struct PeerIdentity {
    certificate: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl PeerIdentity {
    /// Generate an identity for secure first-start persistence by the worker shell.
    pub fn generate() -> Result<Self, TlsError> {
        let generated = rcgen::generate_simple_self_signed(vec![SERVER_NAME.to_owned()])?;
        Self::from_der(
            generated.cert.der().to_vec(),
            generated.signing_key.serialize_der(),
        )
    }

    /// Load a certificate and PKCS#8 private key, verifying that they match.
    pub fn from_der(certificate: Vec<u8>, key: Vec<u8>) -> Result<Self, TlsError> {
        if certificate.len() > MAX_CERTIFICATE_BYTES || key.len() > MAX_CERTIFICATE_BYTES {
            return Err(TlsError::Configuration);
        }
        let identity = Self {
            certificate: CertificateDer::from(certificate),
            key: PrivatePkcs8KeyDer::from(key).into(),
        };
        identity.server_config()?;
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let now = UnixTime::now();
        PinnedServer {
            certificate: Some(identity.certificate.clone()),
            fingerprint: identity.fingerprint(),
            provider: provider.clone(),
        }
        .verify_server_cert(
            &identity.certificate,
            &[],
            &ServerName::try_from(SERVER_NAME).expect("static server name"),
            &[],
            now,
        )?;
        ApplicantVerifier { provider }.verify_client_cert(&identity.certificate, &[], now)?;
        Ok(identity)
    }

    /// Public DER certificate for explicitly authenticated join material.
    pub fn certificate(&self) -> &[u8] {
        self.certificate.as_ref()
    }

    /// Borrow private bytes only for secure persistence, never diagnostics.
    pub fn private_key(&self) -> &[u8] {
        self.key.secret_der()
    }

    /// SHA-256 identity used by the shared membership binding.
    pub fn fingerprint(&self) -> CertFingerprint {
        CertFingerprint::from_bytes(Sha256::digest(self.certificate()).into())
    }

    /// Server TLS configuration. Every connection must prove its certificate key.
    /// Admission remains a separate, required application transition.
    pub fn server_config(&self) -> Result<quinn::ServerConfig, TlsError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut tls = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_client_cert_verifier(Arc::new(ApplicantVerifier { provider }))
            .with_single_cert(vec![self.certificate.clone()], self.key.clone_key())?;
        tls.alpn_protocols = vec![ALPN.to_vec()];
        tls.max_early_data_size = 0;
        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
            .map_err(|_| TlsError::Configuration)?;
        let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        config.transport_config(transport());
        Ok(config)
    }

    /// Client TLS configuration with an exact operator-supplied certificate pin.
    pub fn client_config(&self, pinned: &[u8]) -> Result<quinn::ClientConfig, TlsError> {
        if pinned.len() > MAX_CERTIFICATE_BYTES {
            return Err(TlsError::Configuration);
        }
        let certificate = CertificateDer::from(pinned.to_vec());
        let mut roots = RootCertStore::empty();
        roots.add(certificate.clone())?;
        self.client_with_pin(
            Some(certificate),
            CertFingerprint::from_bytes(Sha256::digest(pinned).into()),
        )
    }

    /// Reconnect to a member using the fingerprint held by validated membership.
    /// Unlike operator join material, gossip need not distribute entire certificates.
    /// The presented certificate must match this pin and pass validity/name/usage
    /// checks; TLS must still prove possession of its private key.
    pub fn member_client_config(
        &self,
        pinned: CertFingerprint,
    ) -> Result<quinn::ClientConfig, TlsError> {
        self.client_with_pin(None, pinned)
    }

    fn client_with_pin(
        &self,
        certificate: Option<CertificateDer<'static>>,
        fingerprint: CertFingerprint,
    ) -> Result<quinn::ClientConfig, TlsError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedServer {
                certificate,
                fingerprint,
                provider,
            }))
            .with_client_auth_cert(vec![self.certificate.clone()], self.key.clone_key())?;
        tls.alpn_protocols = vec![ALPN.to_vec()];
        tls.enable_early_data = false;
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
            .map_err(|_| TlsError::Configuration)?;
        let mut config = quinn::ClientConfig::new(Arc::new(crypto));
        config.transport_config(transport());
        Ok(config)
    }
}

fn transport() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config.max_concurrent_uni_streams(0_u8.into());
    config.max_concurrent_bidi_streams(16_u8.into());
    config.stream_receive_window((super::codec::MAX_FRAME_BYTES as u32 + 4).into());
    config.receive_window((4_u32 * super::codec::MAX_FRAME_BYTES as u32).into());
    config.send_window(4 * super::codec::MAX_FRAME_BYTES as u64);
    config.datagram_receive_buffer_size(Some(64 * 1200));
    config.datagram_send_buffer_size(64 * 1200);
    config.max_idle_timeout(Some(
        Duration::from_secs(15).try_into().expect("fixed timeout"),
    ));
    config.keep_alive_interval(Some(Duration::from_secs(3)));
    Arc::new(config)
}

#[derive(Debug)]
struct PinnedServer {
    certificate: Option<CertificateDer<'static>>,
    fingerprint: CertFingerprint,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        chain: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end.len() > MAX_CERTIFICATE_BYTES || !chain.is_empty() {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        }
        if self
            .certificate
            .as_ref()
            .is_some_and(|expected| end.as_ref() != expected.as_ref())
            || CertFingerprint::from_bytes(Sha256::digest(end.as_ref()).into()) != self.fingerprint
        {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ));
        }
        // The trust anchor is admitted only after the operator/member pin matches.
        // This must never be used as an accept-any-certificate bootstrap policy.
        let mut roots = RootCertStore::empty();
        roots.add(end.clone().into_owned())?;
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            self.provider.clone(),
        )
        .build()
        .map_err(|_| rustls::Error::General("invalid pinned certificate".into()))?;
        verifier.verify_server_cert(end, chain, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Debug)]
struct ApplicantVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ClientCertVerifier for ApplicantVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end: &CertificateDer<'_>,
        chain: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if end.len() > MAX_CERTIFICATE_BYTES || !chain.is_empty() {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        }
        // The applicant chooses its identity. Validate its certificate/usage/time
        // here; rustls separately verifies the CertificateVerify signature below.
        // This is not membership authorization or trust in its advertised node ID.
        let mut roots = RootCertStore::empty();
        roots.add(end.clone().into_owned())?;
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            self.provider.clone(),
        )
        .build()
        .map_err(|_| rustls::Error::General("invalid applicant trust anchor".into()))?;
        verifier.verify_client_cert(end, chain, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_or_malformed_private_identity_is_rejected() {
        let first = PeerIdentity::generate().unwrap();
        let second = PeerIdentity::generate().unwrap();
        assert!(
            PeerIdentity::from_der(first.certificate().to_vec(), second.private_key().to_vec())
                .is_err()
        );
        assert!(PeerIdentity::from_der(vec![0; 8], vec![0; 8]).is_err());
        let reloaded =
            PeerIdentity::from_der(first.certificate().to_vec(), first.private_key().to_vec())
                .unwrap();
        assert_eq!(reloaded.fingerprint(), first.fingerprint());
        let wrong_name =
            rcgen::generate_simple_self_signed(vec!["not-orishu-worker".to_owned()]).unwrap();
        assert!(
            PeerIdentity::from_der(
                wrong_name.cert.der().to_vec(),
                wrong_name.signing_key.serialize_der()
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn historical_profile_is_rejected_even_with_valid_mutual_credentials() {
        let identity = PeerIdentity::generate().unwrap();
        let peer = PeerIdentity::generate().unwrap();
        let server = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(identity.certificate.clone()).unwrap();
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_client_auth_cert(vec![peer.certificate.clone()], peer.key.clone_key())
        .unwrap();
        tls.alpn_protocols = vec![b"orishu-membership/3".to_vec()];
        let config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        ));
        let outgoing = client
            .connect_with(config, server.local_addr().unwrap(), SERVER_NAME)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            let (client_result, server_result) =
                tokio::join!(outgoing, async { server.accept().await.unwrap().await });
            assert!(server_result.is_err());
            assert!(client_result.is_err());
        })
        .await
        .unwrap();
        client.close(0_u32.into(), b"done");
        server.close(0_u32.into(), b"done");
    }

    #[tokio::test]
    async fn client_without_certificate_never_reaches_an_application_stream() {
        let identity = PeerIdentity::generate().unwrap();
        let server = quinn::Endpoint::server(
            identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(identity.certificate().to_vec()))
            .unwrap();
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        ));
        let connect = client
            .connect_with(config, server.local_addr().unwrap(), SERVER_NAME)
            .unwrap();
        let exercise = async {
            let (outgoing, incoming) =
                tokio::join!(connect, async { server.accept().await.unwrap().await });
            assert!(incoming.is_err());
            // A TLS client can finish its local handshake before the server's
            // rejection arrives. That does not mean the server admitted it.
            if let Ok(connection) = outgoing {
                connection.closed().await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), exercise)
            .await
            .unwrap();
        client.close(0_u8.into(), b"done");
        server.close(0_u8.into(), b"done");
    }

    #[tokio::test]
    async fn pinned_mutual_tls_exchanges_real_quic_bytes() {
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
        let exercise = async {
            let (outgoing, incoming) =
                tokio::join!(connect, async { server.accept().await.unwrap().await });
            let outgoing = outgoing.unwrap();
            let incoming = incoming.unwrap();
            let presented = incoming
                .peer_identity()
                .unwrap()
                .downcast::<Vec<CertificateDer<'static>>>()
                .unwrap();
            assert_eq!(presented[0].as_ref(), client_identity.certificate());
            use crate::peer::{read_payload, wire, write_payload};
            use orishu_membership::{Effect, Message, OutboundBody, SessionId, testing};
            let token = crate::credentials::SecretToken::generate().unwrap();
            let mut local = testing::local_identity("applicant");
            local.cert_fingerprint = client_identity.fingerprint();
            let mut server_local = testing::local_identity("server");
            server_local.cert_fingerprint = server_identity.fingerprint();
            let model = orishu_membership::Membership::standalone(
                testing::formation(),
                orishu_membership::ClusterName::new("server").unwrap(),
                server_local,
                orishu_membership::AdmissionPolicy::default(),
                orishu_membership::Limits::default(),
            )
            .unwrap();
            let mut session = crate::peer::session::AuthenticatedSession::from_connection(
                &incoming,
                SessionId(1),
                crate::driver::Generation(0),
                testing::formation(),
            )
            .unwrap();
            use crate::peer::{
                exchange::{ExchangePool, Phase},
                handshake,
            };
            let client_pool = ExchangePool::new(4).unwrap();
            let server_pool = ExchangePool::new(4).unwrap();
            let hello = handshake::request(&local, testing::formation(), true).unwrap();
            let (acknowledgement, served) = tokio::join!(
                client_pool.request(&outgoing, &hello, Phase::Handshake),
                async {
                    let (send, receive) = incoming.accept_bi().await.unwrap();
                    let session_ref = &mut session;
                    let model_ref = &model;
                    server_pool
                        .serve(send, receive, Phase::Handshake, |received| async move {
                            let mut wrong_protocol = received.clone();
                            wrong_protocol[7] = 2;
                            assert!(
                                handshake::accept_request(
                                    &wrong_protocol,
                                    session_ref,
                                    model_ref,
                                    crate::driver::Generation(0)
                                )
                                .is_err()
                            );
                            handshake::accept_request(
                                &received,
                                session_ref,
                                model_ref,
                                crate::driver::Generation(0),
                            )
                            .unwrap();
                            assert!(
                                handshake::accept_request(
                                    &received,
                                    session_ref,
                                    model_ref,
                                    crate::driver::Generation(0)
                                )
                                .is_err()
                            );
                            Ok(
                                handshake::reply(model_ref.local(), testing::formation(), None)
                                    .unwrap(),
                            )
                        })
                        .await
                }
            );
            served.unwrap();
            let acknowledgement = acknowledgement.unwrap();
            let client_model = orishu_membership::Membership::standalone(
                orishu_membership::FormationId::new("client-formation").unwrap(),
                orishu_membership::ClusterName::new("client").unwrap(),
                local.clone(),
                orishu_membership::AdmissionPolicy::default(),
                orishu_membership::Limits::default(),
            )
            .unwrap();
            let mut introducer = crate::peer::session::AuthenticatedSession::from_connection(
                &outgoing,
                SessionId(2),
                crate::driver::Generation(0),
                client_model.formation().clone(),
            )
            .unwrap();
            handshake::accept_reply(
                &acknowledgement,
                &mut introducer,
                &client_model,
                crate::driver::Generation(0),
                testing::formation(),
                server_identity.fingerprint(),
            )
            .unwrap();
            let context = session
                .context(&model, crate::driver::Generation(0))
                .unwrap();
            let joining = orishu_membership::update(
                client_model,
                Message::Local(orishu_membership::Command::BeginJoin {
                    session: SessionId(2),
                    target_formation: testing::formation(),
                }),
            );
            let join_request = joining
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Send { message, .. }
                        if matches!(message.body, OutboundBody::JoinRequest { .. }) =>
                    {
                        Some(message.clone())
                    }
                    _ => None,
                })
                .expect("core emits join request");
            let client_model = joining.model;
            let request = wire::encode_join(
                join_request,
                context.formation.clone(),
                context.sender.clone(),
                &token,
                client_model.local_id(),
            )
            .unwrap();
            let (mut send, mut join_reply) = outgoing.open_bi().await.unwrap();
            write_payload(&mut send, &request.bytes).await.unwrap();
            let (mut reply_send, mut receive) = incoming.accept_bi().await.unwrap();
            let bytes = read_payload(&mut receive).await.unwrap();
            let decoded = session
                .decode(
                    &bytes,
                    &model,
                    crate::driver::Generation(0),
                    wire::Transport::Stream,
                )
                .unwrap();
            let presented_token = decoded.join_token.unwrap();
            assert!(presented_token.matches(token.expose()));
            let transition = orishu_membership::update(model, Message::Peer(decoded.input));
            let (request, session, networks) = transition
                .effects
                .iter()
                .find_map(|effect| {
                    if let Effect::VerifyCredential {
                        request,
                        session,
                        kind:
                            orishu_membership::CredentialKind::JoinToken {
                                blocked_networks, ..
                            },
                    } = effect
                    {
                        Some((*request, *session, blocked_networks))
                    } else {
                        None
                    }
                })
                .expect("core requests credential evidence");
            let verification = crate::peer::admission::verify(
                &token,
                Some(&presented_token),
                incoming.remote_address().ip(),
                true,
                networks,
            );
            assert_eq!(verification.policy_error, None);
            let transition = orishu_membership::update(
                transition.model,
                Message::Outcome(orishu_membership::EffectOutcome::CredentialVerified {
                    request,
                    session,
                    evidence: verification.evidence,
                }),
            );
            let (request, session) = transition
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::AllocateNodeId { request, session } => Some((*request, *session)),
                    _ => None,
                })
                .expect("core requests allocation");
            let assigned = orishu_membership::NodeId::new(
                crate::credentials::SecretToken::generate()
                    .unwrap()
                    .expose(),
            )
            .unwrap();
            let admitted = orishu_membership::update(
                transition.model,
                Message::Outcome(orishu_membership::EffectOutcome::NodeIdAllocated {
                    request,
                    session,
                    node_id: assigned.clone(),
                }),
            );
            let acceptance = admitted
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Send { message, .. }
                        if matches!(message.body, OutboundBody::JoinAccepted { .. }) =>
                    {
                        Some(message.clone())
                    }
                    _ => None,
                })
                .expect("acceptance follows insertion");
            let reply = wire::encode(
                acceptance,
                admitted.model.formation().clone(),
                orishu_membership::SenderIdentity::Admitted(admitted.model.local_id().clone()),
                None,
            )
            .unwrap();
            write_payload(&mut reply_send, &reply.bytes).await.unwrap();
            let bytes = read_payload(&mut join_reply).await.unwrap();
            let reply = introducer
                .decode(
                    &bytes,
                    &client_model,
                    crate::driver::Generation(0),
                    wire::Transport::Stream,
                )
                .unwrap();
            let adopted = orishu_membership::update(client_model, Message::Peer(reply.input));
            assert_eq!(adopted.model.formation(), admitted.model.formation());
            assert_eq!(adopted.model.local_id(), &assigned);
            assert_eq!(adopted.model.members().len(), 2);
        };
        tokio::time::timeout(Duration::from_secs(5), exercise)
            .await
            .unwrap();
        client.close(0_u8.into(), b"done");
        server.close(0_u8.into(), b"done");
    }

    #[tokio::test]
    async fn membership_fingerprint_pin_accepts_only_the_matching_server() {
        let identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        for expected in [identity.fingerprint(), client_identity.fingerprint()] {
            let server = quinn::Endpoint::server(
                identity.server_config().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
            )
            .unwrap();
            let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            let connect = client
                .connect_with(
                    client_identity.member_client_config(expected).unwrap(),
                    server.local_addr().unwrap(),
                    SERVER_NAME,
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                let (outgoing, incoming) =
                    tokio::join!(connect, async { server.accept().await.unwrap().await });
                if expected == identity.fingerprint() {
                    assert!(outgoing.is_ok());
                    assert!(incoming.is_ok());
                } else {
                    assert!(outgoing.is_err());
                    assert!(incoming.is_err());
                }
            })
            .await
            .unwrap();
            client.close(0_u8.into(), b"done");
            server.close(0_u8.into(), b"done");
        }
    }

    #[tokio::test]
    async fn wrong_introducer_pin_fails_before_application_exchange() {
        let server_identity = PeerIdentity::generate().unwrap();
        let client_identity = PeerIdentity::generate().unwrap();
        let unrelated = PeerIdentity::generate().unwrap();
        let server = quinn::Endpoint::server(
            server_identity.server_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connect = client
            .connect_with(
                client_identity
                    .client_config(unrelated.certificate())
                    .unwrap(),
                server.local_addr().unwrap(),
                SERVER_NAME,
            )
            .unwrap();
        let exercise = async {
            let (outgoing, incoming) =
                tokio::join!(connect, async { server.accept().await.unwrap().await });
            assert!(outgoing.is_err());
            assert!(incoming.is_err());
        };
        tokio::time::timeout(Duration::from_secs(5), exercise)
            .await
            .unwrap();
        client.close(0_u8.into(), b"done");
        server.close(0_u8.into(), b"done");
    }
}
