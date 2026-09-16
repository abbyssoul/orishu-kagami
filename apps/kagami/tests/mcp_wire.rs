//! End-to-end MCP lifecycle over a real HTTP connection.
//!
//! These drive the embedded server the same way an external agent would: raw
//! HTTP/1.1 to the bound loopback port, bearer token on every request. The
//! mandatory wire smoke test walks the full handshake
//! (`initialize → notifications/initialized → tools/list → tools/call`) against
//! `kagami_status`, because only the real `serde_json` wire path catches
//! serialization bugs the in-process tests miss (source assessment).
//!
//! The windowed checks — enable/disable from the real UI, an agent driving the
//! live session — stay manual; CI cannot drive the window.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use kagami::mcp::{self, McpRunning, McpState, SessionState};
use kagami_catalog::SchemaRegistry;
use kagami_document::Limits;

/// A running server on an ephemeral loopback port, with an empty session.
fn running_server() -> McpRunning {
    let document = kagami::document::Document::new(SchemaRegistry::new(), Limits::DEFAULT);
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("a valid address");
    match mcp::enable(SessionState::from_document(&document), addr) {
        McpState::Running(running) => running,
        McpState::Disabled => panic!("enable returned Disabled"),
        McpState::Failed(reason) => panic!("enable failed: {reason}"),
    }
}

/// What one HTTP response carried that these tests care about.
struct Response {
    status: u16,
    session_id: Option<String>,
    /// JSON payloads from SSE `data:` lines, or the whole body if there were
    /// none.
    payloads: Vec<String>,
}

impl Response {
    fn body_contains(&self, needle: &str) -> bool {
        self.payloads.iter().any(|payload| payload.contains(needle))
    }
}

/// POST one JSON-RPC body to `/mcp` and read the response to completion.
///
/// `Connection: close` asks the server to end the response so the read-to-EOF
/// terminates promptly rather than waiting on an open SSE stream.
fn post(
    addr: SocketAddr,
    token: Option<&str>,
    session_id: Option<&str>,
    body: &str,
) -> std::io::Result<Response> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;

    let mut request = format!(
        "POST /mcp HTTP/1.1\r\n\
         Host: localhost\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n\
         Connection: close\r\n\
         Content-Length: {}\r\n",
        body.len()
    );
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(id) = session_id {
        request.push_str(&format!("mcp-session-id: {id}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);

    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => raw.extend_from_slice(&buf[..read]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == ErrorKind::TimedOut => break,
            Err(error) => return Err(error),
        }
    }

    Ok(parse(&raw))
}

/// Parse the status, session-id header, and `data:` JSON payloads out of a raw
/// HTTP response. Chunk framing is left in place: a single small JSON-RPC
/// message sits contiguously within one chunk, so scanning for `data:` lines
/// recovers it without a full dechunker.
fn parse(raw: &[u8]) -> Response {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));

    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);

    let session_id = head
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("mcp-session-id:"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_owned());

    let mut payloads: Vec<String> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| data.trim().to_owned())
        .filter(|data| !data.is_empty())
        .collect();
    if payloads.is_empty() && !body.trim().is_empty() {
        payloads.push(body.trim().to_owned());
    }

    Response {
        status,
        session_id,
        payloads,
    }
}

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"kagami-wire-test","version":"0"}}}"#;

#[test]
fn the_full_handshake_calls_kagami_status_over_real_http() {
    let server = running_server();
    let addr = server.addr();
    let token = server.token().to_owned();

    // 1. initialize — creates the session and returns its id.
    let initialized = post(addr, Some(&token), None, INITIALIZE).expect("initialize connects");
    assert_eq!(initialized.status, 200, "initialize is accepted");
    assert!(
        initialized.body_contains("protocolVersion"),
        "initialize returns a protocol version: {:?}",
        initialized.payloads
    );
    let session = initialized
        .session_id
        .expect("initialize returns a session id");

    // 2. notifications/initialized — the client half of the handshake.
    let ack = post(
        addr,
        Some(&token),
        Some(&session),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    )
    .expect("initialized notification connects");
    assert!(
        ack.status == 202 || ack.status == 200,
        "the initialized notification is accepted (got {})",
        ack.status
    );

    // 3. tools/list — the one tool is discoverable.
    let listed = post(
        addr,
        Some(&token),
        Some(&session),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    )
    .expect("tools/list connects");
    assert_eq!(listed.status, 200);
    assert!(
        listed.body_contains("kagami_status"),
        "tools/list advertises kagami_status: {:?}",
        listed.payloads
    );

    // 4. tools/call kagami_status — the structured result reflects the session.
    let called = post(
        addr,
        Some(&token),
        Some(&session),
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kagami_status","arguments":{}}}"#,
    )
    .expect("tools/call connects");
    assert_eq!(called.status, 200);
    assert!(
        called.body_contains("kagami") && called.body_contains("revision"),
        "kagami_status returns app and document status: {:?}",
        called.payloads
    );

    server.disable();
}

#[test]
fn a_missing_or_wrong_token_is_rejected() {
    let server = running_server();
    let addr = server.addr();

    let missing = post(addr, None, None, INITIALIZE).expect("connects");
    assert_eq!(missing.status, 401, "a missing token is rejected");

    let wrong = post(addr, Some("not-the-token"), None, INITIALIZE).expect("connects");
    assert_eq!(wrong.status, 401, "a wrong token is rejected");

    // The valid token still works — a rejected request did not wedge the server.
    let ok = post(addr, Some(server.token()), None, INITIALIZE).expect("connects");
    assert_eq!(ok.status, 200);

    server.disable();
}

#[test]
fn a_real_session_is_counted() {
    let mut server = running_server();
    let addr = server.addr();

    server.poll().expect("healthy");
    assert_eq!(server.connection_count(), Some(0), "no clients yet");

    let initialized = post(addr, Some(server.token()), None, INITIALIZE).expect("connects");
    assert_eq!(initialized.status, 200);

    server.poll().expect("still healthy");
    assert_eq!(
        server.connection_count(),
        Some(1),
        "the initialize handshake is counted"
    );

    server.disable();
}

#[test]
fn malformed_json_is_a_bounded_error_and_does_not_wedge_the_server() {
    let server = running_server();
    let addr = server.addr();
    let token = server.token().to_owned();

    let malformed = post(addr, Some(&token), None, "{ this is not json ").expect("connects");
    assert!(
        malformed.status >= 400,
        "malformed JSON is refused with an error status (got {})",
        malformed.status
    );

    // The server is still serving.
    let ok = post(addr, Some(&token), None, INITIALIZE).expect("connects");
    assert_eq!(ok.status, 200, "a good request still succeeds afterwards");

    server.disable();
}

#[test]
fn an_oversized_body_is_refused_and_does_not_crash_the_server() {
    let server = running_server();
    let addr = server.addr();
    let token = server.token().to_owned();

    // Well past the 1 MiB request-body cap.
    let huge = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"blob":"{}"}}}}"#,
        "a".repeat(2 * 1024 * 1024)
    );
    let oversized = post(addr, Some(&token), None, &huge);
    // Either the server answers with an error status, or it closes the
    // connection — both are bounded refusals, neither is a crash.
    if let Ok(response) = oversized {
        assert_ne!(response.status, 200, "an oversized body is not accepted");
    }

    let ok = post(addr, Some(&token), None, INITIALIZE).expect("connects");
    assert_eq!(ok.status, 200, "the server survived the oversized body");

    server.disable();
}

#[test]
fn re_enabling_rotates_the_credential() {
    let first = running_server();
    let first_token = first.token().to_owned();
    first.disable();

    let second = running_server();
    assert_ne!(
        second.token(),
        first_token,
        "a re-enabled server mints a fresh credential"
    );

    // The old credential does not work on the new server.
    let rejected = post(second.addr(), Some(&first_token), None, INITIALIZE).expect("connects");
    assert_eq!(rejected.status, 401);

    second.disable();
}

#[test]
fn disabling_cuts_off_further_requests() {
    let server = running_server();
    let addr = server.addr();
    let token = server.token().to_owned();

    let before = post(addr, Some(&token), None, INITIALIZE).expect("connects while enabled");
    assert_eq!(before.status, 200);

    server.disable();

    // Give the serve loop a moment to unwind and release the port.
    std::thread::sleep(Duration::from_millis(200));

    let after = post(addr, Some(&token), None, INITIALIZE);
    assert!(
        after.is_err() || after.is_ok_and(|response| response.status != 200),
        "requests fail once the server is disabled"
    );
}

#[test]
fn a_non_loopback_bind_leaves_mcp_disabled_with_a_reason() {
    let document = kagami::document::Document::new(SchemaRegistry::new(), Limits::DEFAULT);
    let addr: SocketAddr = "10.0.0.1:8642".parse().expect("a valid address");

    match mcp::enable(SessionState::from_document(&document), addr) {
        McpState::Failed(reason) => assert!(
            reason.contains("loopback"),
            "the failure names the loopback-only posture: {reason}"
        ),
        McpState::Running(_) => panic!("a non-loopback bind must not succeed"),
        McpState::Disabled => panic!("a non-loopback bind should report a reason"),
    }
}
