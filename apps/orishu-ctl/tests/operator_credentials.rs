//! Exercise credential-file parsing and bearer attachment through the CLI binary.
#![cfg(unix)]

use std::{
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, net::UnixListener},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn cli_sends_file_credential_without_printing_it() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operator.token");
    let token = "ab".repeat(32);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    file.write_all(token.as_bytes()).unwrap();
    let socket = root.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_orishuctl"))
        .arg("--host")
        .arg(&socket)
        .arg("--operator-token-file")
        .arg(&path)
        .args(["--output", "json", "cluster", "info"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = (|| -> std::io::Result<String> {
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") && headers.len() < 8192 && Instant::now() < deadline {
            let mut byte = [0];
            stream.read_exact(&mut byte)?;
            headers.push(byte[0]);
        }
        stream.write_all(
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        Ok(String::from_utf8(headers).unwrap())
    })();
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let timed_out = child.try_wait().unwrap().is_none();
    if timed_out || result.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    assert!(!timed_out, "CLI completion deadline");
    let headers = result.unwrap();
    assert!(headers.starts_with("GET /api/v1/cluster HTTP/1.1\r\n"));
    assert!(
        headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {token}")))
    );
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&token));
}
