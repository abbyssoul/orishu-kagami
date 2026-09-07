//! Real collector authentication without the peer protocol's custom verifier.
use super::*;
use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, Issuer, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Authority {
    cert: CertificateDer<'static>,
    #[cfg(unix)]
    pem: String,
    issuer: Issuer<'static, KeyPair>,
}

impl Authority {
    fn new() -> Self {
        let mut params = CertificateParams::default();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        Self {
            cert: certificate.der().clone(),
            #[cfg(unix)]
            pem: certificate.pem(),
            issuer: Issuer::new(params, key),
        }
    }

    fn leaf(
        &self,
        name: &str,
        usage: ExtendedKeyUsagePurpose,
    ) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &self.issuer).unwrap();
        (
            cert.der().clone(),
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
    }

    fn roots(&self) -> rustls::RootCertStore {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.cert.clone()).unwrap();
        roots
    }
}

#[tokio::test]
async fn collector_tls_verifies_server_and_client_before_sending_credentials() {
    // Each failure has its own listener and handshake, not a mocked verifier.
    for case in [
        "valid",
        "wrong-root",
        "wrong-name",
        "no-client",
        "wrong-client",
    ] {
        let collector_ca = Authority::new();
        let client_ca = Authority::new();
        let unrelated_ca = Authority::new();
        let name = if case == "wrong-name" {
            "other.example"
        } else {
            "127.0.0.1"
        };
        let (cert, key) = collector_ca.leaf(name, ExtendedKeyUsagePurpose::ServerAuth);
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(client_ca.roots()),
            provider.clone(),
        )
        .build()
        .unwrap();
        let server_config = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![cert], key)
            .unwrap();
        let trusted = if case == "wrong-root" {
            &unrelated_ca
        } else {
            &collector_ca
        };
        let client_builder = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(trusted.roots());
        let client_config = if case == "no-client" {
            client_builder.with_no_client_auth()
        } else {
            let issuer = if case == "wrong-client" {
                &unrelated_ca
            } else {
                &client_ca
            };
            let (cert, key) = issuer.leaf("collector-client", ExtendedKeyUsagePurpose::ClientAuth);
            client_builder
                .with_client_auth_cert(vec![cert], key)
                .unwrap()
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("https://{}/v1/traces", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(3), async move {
                let (stream, _) = listener.accept().await.unwrap();
                let accepted = tokio_rustls::TlsAcceptor::from(Arc::new(server_config))
                    .accept(stream)
                    .await;
                if case != "valid" {
                    assert!(accepted.is_err(), "{case}: unauthorized TLS accepted");
                    return;
                }
                let mut stream = accepted.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    assert!(head.len() < 4096);
                    head.push(stream.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                assert!(head.starts_with("post /v1/traces http/1.1\r\n"));
                assert!(head.contains("authorization: bearer collector-only-test-token\r\n"));
                let length: usize = head.lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap().parse().unwrap();
                assert!(length <= 1024);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let decoded = super::super::ExportTraceServiceRequest::decode(body.as_slice()).unwrap();
                assert_eq!(decoded.resource_spans[0].scope_spans[0].spans[0].trace_id, vec![1; 16]);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n").await.unwrap();
            }).await.unwrap();
        });
        let mut delivery = HttpDelivery::new(
            &endpoint,
            Some(client_config),
            Some("collector-only-test-token"),
            Duration::from_secs(2),
            1024,
        )
        .unwrap();
        #[cfg(unix)]
        let _files = if case == "valid" {
            use std::os::unix::fs::PermissionsExt;
            let temp = tempfile::tempdir().unwrap();
            std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut params = CertificateParams::new(vec!["collector-client".into()]).unwrap();
            params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
            let key = KeyPair::generate().unwrap();
            let cert = params.signed_by(&key, &client_ca.issuer).unwrap();
            let ca_path = temp.path().join("collector-ca.pem");
            let cert_path = temp.path().join("client.pem");
            let key_path = temp.path().join("client.key");
            let token_path = temp.path().join("token");
            for (path, contents) in [
                (&ca_path, collector_ca.pem.clone()),
                (&cert_path, cert.pem()),
                (&key_path, key.serialize_pem()),
                (&token_path, "collector-only-test-token\n".into()),
            ] {
                std::fs::write(path, contents).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            delivery = crate::trace_export::CollectorFiles {
                ca: Some(&ca_path),
                certificate: Some(&cert_path),
                key: Some(&key_path),
                token: Some(&token_path),
            }
            .prepare(&endpoint, Duration::from_secs(2), 1024)
            .unwrap();
            Some(temp)
        } else {
            None
        };
        let outcome = delivery.deliver(super::tests::batch()).await;
        server.await.unwrap();
        if case == "valid" {
            assert_eq!(
                outcome,
                Ok(DeliveryOutcome {
                    rejected: 0,
                    warning: false
                })
            );
        } else {
            assert_eq!(outcome, Err(DeliveryError::Transport), "{case}");
            assert!(!format!("{outcome:?}").contains("collector-only-test-token"));
        }
    }
}
