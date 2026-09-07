//! Actual startup validation and collector mTLS; not an in-process worker substitute.
use super::*;
use prost::Message;
use std::{io::Read, os::unix::fs::PermissionsExt, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn tracing_files_fail_before_startup_and_valid_files_deliver_over_mtls() {
    tokio::time::timeout(Duration::from_secs(20), async {
        for case in ["invalid-ca", "wrong-key", "public-token", "linked-token", "wrong-trust", "wrong-name", "wrong-client", "system-roots", "valid"] {
            if case == "system-roots" && !cfg!(target_os = "linux") { continue; }
            let startup_failure = matches!(case, "invalid-ca" | "wrong-key" | "public-token" | "linked-token");
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let server = rcgen::generate_simple_self_signed(vec![if case == "wrong-name" { "other.example" } else { "127.0.0.1" }.into()]).unwrap();
            let client = rcgen::generate_simple_self_signed(vec!["collector-client".into()]).unwrap();
            let other = rcgen::generate_simple_self_signed(vec!["unrelated-client".into()]).unwrap();
            let ca = root.path().join("ca.pem");
            let certificate = root.path().join("client.pem");
            let key = root.path().join("secret-key-marker.pem");
            let token = root.path().join("secret-token-marker");
            let ambient_ca = root.path().join("ambient-ca.pem");
            std::fs::write(&ambient_ca, server.cert.pem()).unwrap();
            for (path, bytes) in [
                (&ca, if case == "invalid-ca" { "invalid-ca-secret-marker".into() } else if case == "wrong-trust" { other.cert.pem() } else { server.cert.pem() }),
                (&certificate, if case == "wrong-client" { other.cert.pem() } else { client.cert.pem() }),
                (&key, if matches!(case, "wrong-key" | "wrong-client") { other.signing_key.serialize_pem() } else { client.signing_key.serialize_pem() }),
                (&token, "collector-secret-marker\n".into()),
            ] {
                std::fs::write(path, bytes).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            if case == "public-token" {
                std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            if case == "linked-token" {
                let target = root.path().join("token-target");
                std::fs::rename(&token, &target).unwrap();
                std::os::unix::fs::symlink(&target, &token).unwrap();
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("https://{}/v1/traces", listener.local_addr().unwrap());
            let state = root.path().join("state");
            let socket = root.path().join("worker.sock");
            let mut command = Command::new(env!("CARGO_BIN_EXE_orishu-worker"));
            for (name, _) in std::env::vars_os() {
                if name.to_string_lossy().starts_with("ORISHU_") { command.env_remove(name); }
            }
            command.env("SSL_CERT_FILE", &ambient_ca).env("SSL_CERT_DIR", root.path());
            if case != "system-roots" { command.arg("--tracing.ca-file").arg(&ca); }
            let mut worker = Worker(command.arg("--state-dir").arg(&state)
                .arg("--listen.clients").arg(&socket)
                .args(["--tracing.enabled", "true", "--tracing.endpoint", &endpoint,
                    "--tracing.sample-ppm", "1000000", "--tracing.batch-size", "1",
                    "--tracing.export-timeout-ms", "1000", "--tracing.shutdown-timeout-ms", "100"])
                .arg("--tracing.client-cert-file").arg(&certificate)
                .arg("--tracing.client-key-file").arg(&key)
                .arg("--tracing.bearer-token-file").arg(&token)
                .stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap());
            if startup_failure {
                let status = tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        if let Some(status) = worker.0.try_wait().unwrap() { break status; }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }).await.unwrap();
                assert_eq!(status.code(), Some(2), "{case}");
                assert!(!state.exists() && !socket.exists(), "{case}");
                assert!(tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_err());
            } else {
                let before = summary(&mut worker, &socket).await;
                let mut roots = rustls::RootCertStore::empty();
                roots.add(client.cert.der().clone()).unwrap();
                let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
                let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().unwrap();
                let tls = rustls::ServerConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions().unwrap()
                    .with_client_cert_verifier(verifier)
                    .with_single_cert(vec![server.cert.der().clone()], rustls::pki_types::PrivatePkcs8KeyDer::from(server.signing_key.serialize_der()).into()).unwrap();
                let (stream, _) = listener.accept().await.unwrap();
                let accepted = tokio_rustls::TlsAcceptor::from(Arc::new(tls)).accept(stream).await;
                if case == "valid" {
                let mut stream = accepted.unwrap();
                assert_eq!(stream.get_ref().1.peer_certificates().unwrap()[0], *client.cert.der());
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    assert!(head.len() < 4096);
                    head.push(stream.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                assert!(head.starts_with("post /v1/traces http/1.1\r\n"));
                assert!(head.contains("authorization: bearer collector-secret-marker\r\n"));
                let length: usize = head.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                assert!(length <= 4096);
                let mut bytes = vec![0; length];
                stream.read_exact(&mut bytes).await.unwrap();
                let request = opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(bytes.as_slice()).unwrap();
                assert_eq!(request.resource_spans[0].scope_spans[0].spans[0].name, "orishu.client.request");
                assert!(!String::from_utf8_lossy(&bytes).contains("secret-marker"));
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n").await.unwrap();
                drop(stream);
                } else {
                    assert!(accepted.is_err(), "{case}: unauthorized TLS accepted");
                }
                drop(listener);
                let after = summary(&mut worker, &socket).await;
                assert_eq!(after.formation_id, before.formation_id);
                assert_eq!(after.source_node_id, before.source_node_id);
                assert_eq!(after.member_count, before.member_count);
                worker.terminate().await;
            }
            let mut stderr = String::new();
            worker.0.stderr.take().unwrap().take(8193).read_to_string(&mut stderr).unwrap();
            assert!(stderr.len() <= 8192);
            for secret in ["secret-marker", "secret-key-marker", "secret-token-marker", "PRIVATE KEY", &endpoint] {
                assert!(!stderr.contains(secret), "credential detail leaked in {case}");
            }
            if startup_failure { assert!(stderr.contains("invalid trace exporter configuration"), "{case}: {stderr}"); }
        }
    }).await.expect("worker collector credential matrix deadline");
}
