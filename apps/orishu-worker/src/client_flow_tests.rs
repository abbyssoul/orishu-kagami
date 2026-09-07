//! Real HTTP/2 decoder/transport limits, not a membership or process journey.
//! A test handler deliberately does not consume bodies so receive credit
//! cannot be renewed by application reads while a boundary is being tested.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

struct HoldBody(Arc<AtomicUsize>);

#[handler]
impl HoldBody {
    async fn handle(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await;
    }
}

#[handler]
async fn available() -> &'static str {
    "available"
}

async fn frame(stream: &mut UnixStream, kind: u8, flags: u8, id: u32, payload: &[u8]) {
    assert!(payload.len() <= 16384);
    let mut bytes = Vec::with_capacity(9 + payload.len());
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes()[1..]);
    bytes.extend_from_slice(&[kind, flags]);
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(payload);
    stream.write_all(&bytes).await.unwrap();
}

async fn read_frame(stream: &mut UnixStream) -> (u8, u8, u32, Vec<u8>) {
    let mut header = [0; 9];
    stream.read_exact(&mut header).await.unwrap();
    let len = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
    assert!(len <= 16384);
    let mut payload = vec![0; len];
    stream.read_exact(&mut payload).await.unwrap();
    (
        header[3],
        header[4],
        u32::from_be_bytes(header[5..].try_into().unwrap()) & 0x7fff_ffff,
        payload,
    )
}

async fn ping(stream: &mut UnixStream, allow_reclaimed_credit: bool) {
    frame(stream, 6, 0, 0, b"boundary").await;
    for _ in 0..16 {
        let (kind, flags, id, payload) = read_frame(stream).await;
        if kind == 6 {
            assert_eq!((flags, id), (1, 0));
            assert_eq!(payload, b"boundary");
            return;
        }
        if allow_reclaimed_credit && kind == 8 {
            assert_eq!(id, 0);
            assert_eq!(payload.len(), 4);
            assert!(u32::from_be_bytes(payload.try_into().unwrap()) > 0);
            continue;
        }
        // No body consumer exists, so there must be no renewed receive credit.
        assert_eq!((kind, flags, id), (4, 1, 0));
    }
    panic!("PING response frame budget");
}

#[derive(Clone, Copy)]
enum Violation {
    ConnectionData,
    StreamData,
    WindowUpdate { stream: u32, increment: u32 },
    ShortWindowUpdate,
}

async fn exercise(violation: Violation) {
    tokio::time::timeout(Duration::from_secs(6), async {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("flow.sock");
        let acceptor = UnixListener::new(path.clone()).bind().await;
        let mut server = client_server(acceptor);
        let enlarged = matches!(violation, Violation::StreamData);
        if enlarged {
            // Isolate the unchanged production stream limit. Equal initial
            // windows otherwise make connection exhaustion win this ordering.
            server.http2_mut().initial_connection_window_size(131070);
        }
        let handle = server.handle();
        let entered = Arc::new(AtomicUsize::new(0));
        let router = Router::new()
            .push(Router::with_path("hold").post(HoldBody(entered.clone())))
            .push(Router::with_path("available").get(available));
        let task = tokio::spawn(server.try_serve(router));
        let mut stream = UnixStream::connect(&path).await.unwrap();
        stream
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .unwrap();
        frame(&mut stream, 4, 0, 0, &[]).await;
        let mut configured_window = 65535;
        let mut connection_window = 65535;
        loop {
            let (kind, flags, id, payload) = read_frame(&mut stream).await;
            assert_eq!(id, 0);
            match kind {
                4 if flags == 1 => assert!(payload.is_empty()),
                4 => {
                    assert_eq!(flags, 0);
                    assert_eq!(payload.len() % 6, 0);
                    for setting in payload.chunks_exact(6) {
                        if setting[..2] == [0, 4] {
                            configured_window =
                                u32::from_be_bytes(setting[2..].try_into().unwrap());
                        }
                    }
                    frame(&mut stream, 4, 1, 0, &[]).await;
                    if !enlarged {
                        break;
                    }
                }
                8 if enlarged => {
                    assert_eq!(payload.len(), 4);
                    connection_window += u32::from_be_bytes(payload.try_into().unwrap());
                    break;
                }
                _ => panic!("unexpected initial frame {kind}"),
            }
        }
        assert_eq!(configured_window, 65535);
        assert_eq!(connection_window, if enlarged { 131070 } else { 65535 });
        let streams = if matches!(violation, Violation::ConnectionData) {
            2
        } else {
            1
        };
        for id in (1..=streams * 2).step_by(2) {
            // POST /hold, no content-length: only flow control bounds DATA.
            frame(
                &mut stream,
                1,
                4,
                id,
                b"\x83\x86\x04\x05/hold\x01\x09localhost",
            )
            .await;
        }
        while entered.load(Ordering::SeqCst) != streams as usize {
            tokio::task::yield_now().await;
        }
        // Legal updates, including the ignored reserved high bit, must not
        // be rejected by an implementation that simply refuses this frame.
        for id in [0, 1] {
            for increment in [1_u32, 0x8000_0001] {
                frame(&mut stream, 8, 0, id, &increment.to_be_bytes()).await;
            }
        }
        ping(&mut stream, false).await;
        let (expected_kind, expected_id, expected_code) = match violation {
            Violation::ConnectionData | Violation::StreamData => {
                for index in 0..4 {
                    let id = if streams == 2 && index % 2 == 1 { 3 } else { 1 };
                    let len = if index == 3 { 16383 } else { 16384 };
                    frame(&mut stream, 0, 0, id, &vec![0; len]).await;
                }
                // Exactly 65535 bytes are accepted, without any body reads.
                ping(&mut stream, false).await;
                if !enlarged {
                    // Empty END_STREAM remains legal at zero connection
                    // credit. Overrun the other still-open stream afterward.
                    frame(&mut stream, 0, 1, 1, &[]).await;
                    ping(&mut stream, false).await;
                }
                frame(&mut stream, 0, 0, if enlarged { 1 } else { 3 }, &[0]).await;
                if enlarged { (3, 1, 3) } else { (7, 0, 3) }
            }
            Violation::WindowUpdate {
                stream: id,
                increment,
            } => {
                ping(&mut stream, false).await;
                frame(&mut stream, 8, 0, id, &increment.to_be_bytes()).await;
                // The pinned decoder promotes zero increments to connection
                // errors, permitted by RFC 9113 section 5.4.1.
                (
                    if id == 0 || increment == 0 { 7 } else { 3 },
                    if increment == 0 { 0 } else { id },
                    if increment == 0 { 1 } else { 3 },
                )
            }
            Violation::ShortWindowUpdate => {
                ping(&mut stream, false).await;
                frame(&mut stream, 8, 0, 0, &[0; 3]).await;
                // The decoder uses the permitted generic PROTOCOL_ERROR for
                // this malformed frame (RFC 9113 section 5.4), not code 6.
                (7, 0, 1)
            }
        };
        let (kind, _, id, payload) = read_frame(&mut stream).await;
        assert_eq!((kind, id), (expected_kind, expected_id));
        let code = if kind == 7 {
            assert!(payload.len() >= 8);
            u32::from_be_bytes(payload[4..8].try_into().unwrap())
        } else {
            assert_eq!(payload.len(), 4);
            u32::from_be_bytes(payload.try_into().unwrap())
        };
        assert_eq!(code, expected_code);
        if kind == 3 {
            // Stream refusal must not destroy unrelated connection control.
            ping(&mut stream, true).await;
            frame(
                &mut stream,
                1,
                5,
                5,
                b"\x82\x86\x04\x0a/available\x01\x09localhost",
            )
            .await;
            let mut status = false;
            let mut complete = false;
            for _ in 0..16 {
                let (kind, flags, id, payload) = read_frame(&mut stream).await;
                assert_eq!(id, 5);
                match kind {
                    1 => {
                        assert_eq!(payload.first(), Some(&0x88));
                        status = true;
                    }
                    0 => assert_eq!(payload, b"available"),
                    _ => panic!("unexpected response frame {kind}"),
                }
                if flags & 1 != 0 {
                    complete = true;
                    break;
                }
            }
            assert!(status && complete);
        } else {
            // Retain the offending client until the server closes it.
            let mut tail = Vec::new();
            let result = (&mut stream).take(8193).read_to_end(&mut tail).await;
            if let Err(error) = result {
                assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
            }
            assert!(tail.len() <= 8192);
        }
        let mut other = UnixStream::connect(&path).await.unwrap();
        other
            .write_all(b"GET /available HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        other.take(8193).read_to_end(&mut response).await.unwrap();
        assert!(response.len() <= 8192 && response.starts_with(b"HTTP/1.1 200 "));
        drop(stream);
        handle.stop_graceful(Some(Duration::from_millis(100)));
        task.await.unwrap().unwrap();
    })
    .await
    .expect("flow-control violation fixture budget");
}

#[tokio::test]
async fn connection_receive_credit_accepts_exact_limit_then_refuses_one_byte() {
    exercise(Violation::ConnectionData).await;
}

#[tokio::test]
async fn stream_receive_credit_is_independent_of_connection_credit() {
    exercise(Violation::StreamData).await;
}

#[tokio::test]
async fn invalid_window_updates_have_stream_and_connection_error_scope() {
    for stream in [0, 1] {
        for increment in [0, 0x7fff_ffff] {
            exercise(Violation::WindowUpdate { stream, increment }).await;
        }
    }
    exercise(Violation::ShortWindowUpdate).await;
}
