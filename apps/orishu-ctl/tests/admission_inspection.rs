//! CLI boundary evidence, not a substitute for interrupted-join worker journeys.
#![cfg(unix)]

use orishu::model::{
    ApiResponse, ResponseData,
    cluster::{
        AdmissionInspection, AdmissionInspectionOutcome as Outcome, AdmissionInspectionRequest,
    },
};
use std::{
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, net::UnixListener},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn inspection_preserves_evidence_and_refuses_uncorrelated_reports_without_mutation() {
    let request = AdmissionInspectionRequest {
        schema_version: 1,
        formation_id: "target".parse().unwrap(),
        reference: orishu::model::cluster::JoinRecoveryReference {
            attempt_id: "attempt".parse().unwrap(),
            applicant_fingerprint: "11".repeat(32).parse().unwrap(),
            introducer_node_id: "issuer".parse().unwrap(),
            introducer_fingerprint: "22".repeat(32).parse().unwrap(),
        },
    };
    for case in [
        "current",
        "missing",
        "restricted",
        "wrong-issuer",
        "attempt",
        "formation",
        "issuer",
        "applicant-pin",
        "issuer-pin",
        "version",
        "lookup-error",
        "timeout",
    ] {
        let mut report = AdmissionInspection {
            schema_version: 1,
            request: request.clone(),
            source_formation_id: request.formation_id.clone(),
            source_node_id: request.reference.introducer_node_id.clone(),
            outcome: Outcome::CurrentMember {
                node_id: "assigned".parse().unwrap(),
            },
        };
        match case {
            "missing" => report.outcome = Outcome::RecordUnavailable,
            "restricted" => {
                report.outcome = Outcome::RetiredOrRestricted {
                    node_id: "assigned".parse().unwrap(),
                }
            }
            "wrong-issuer" => {
                report.outcome = Outcome::WrongIssuer;
                report.source_formation_id = "restarted".parse().unwrap();
                report.source_node_id = "new-node".parse().unwrap();
            }
            "attempt" => report.request.reference.attempt_id = "other".parse().unwrap(),
            "formation" => report.source_formation_id = "other".parse().unwrap(),
            "issuer" => report.source_node_id = "other".parse().unwrap(),
            "applicant-pin" => {
                report.request.reference.applicant_fingerprint = "33".repeat(32).parse().unwrap()
            }
            "issuer-pin" => {
                report.request.reference.introducer_fingerprint = "33".repeat(32).parse().unwrap()
            }
            "version" => report.schema_version = 2,
            _ => {}
        }
        let expected_success =
            matches!(case, "current" | "missing" | "restricted" | "wrong-issuer");
        let response = if case == "lookup-error" {
            ApiResponse::Error {
                code: "unavailable".into(),
                message: "inspection unavailable".into(),
            }
        } else {
            ApiResponse::Ok {
                data: Some(ResponseData::AdmissionInspection(report.clone())),
            }
        };
        let mut body = Vec::new();
        ciborium::into_writer(&response, &mut body).unwrap();
        let root = tempfile::tempdir().unwrap();
        let token_path = root.path().join("operator.token");
        let token = "ab".repeat(32);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&token_path)
            .unwrap()
            .write_all(token.as_bytes())
            .unwrap();
        let socket = root.path().join("api.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut child = Reap(
            Command::new(env!("CARGO_BIN_EXE_orishuctl"))
                .arg("--host")
                .arg(&socket)
                .arg("--operator-token-file")
                .arg(&token_path)
                .args([
                    "--output",
                    "json",
                    "--timeout",
                    "1s",
                    "admission-inspect",
                    "--formation-id",
                    "target",
                    "--attempt-id",
                    "attempt",
                    "--applicant-fingerprint",
                    &"11".repeat(32),
                    "--introducer-node-id",
                    "issuer",
                    "--introducer-fingerprint",
                    &"22".repeat(32),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "{case}: request deadline");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{case}: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192 && Instant::now() < deadline);
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with("POST /api/v1/membership/admission-inspections HTTP/1.1\r\n"));
        assert!(
            headers
                .lines()
                .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {token}")))
        );
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        assert!(length <= 4096);
        let mut received = vec![0; length];
        stream.read_exact(&mut received).unwrap();
        let decoded: AdmissionInspectionRequest =
            ciborium::from_reader(received.as_slice()).unwrap();
        assert_eq!(decoded, request);
        let response_wait_started = Instant::now();
        if case != "timeout" {
            let status = if case == "lookup-error" {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
        // Keep the original connection open: do not manufacture timeout or EOF.
        while child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "{case}: CLI completion deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
        if case == "timeout" {
            assert!(
                response_wait_started.elapsed() >= Duration::from_millis(800),
                "lookup must remain pending until the configured request timeout"
            );
        }
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "{case}: unexpected follow-up connection"
        );
        let mut extra = [0];
        assert_eq!(
            stream.read(&mut extra).unwrap(),
            0,
            "{case}: unexpected follow-up request"
        );
        let status = child.0.wait().unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(
            !stdout.contains(&token) && !stderr.contains(&token),
            "{case}: credential disclosure"
        );
        assert_eq!(status.success(), expected_success, "{case}: {stderr}");
        if expected_success {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
                serde_json::to_value(&report).unwrap(),
                "{case}"
            );
        } else {
            assert!(stdout.trim().is_empty(), "{case}: misleading result");
            assert!(!stderr.trim().is_empty(), "{case}: missing error");
        }
    }
}
