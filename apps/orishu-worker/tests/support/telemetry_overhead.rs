//! Manual Linux control-plane measurement; never a timing assertion in CI.
use super::*;
use std::{io::Read, os::unix::fs::PermissionsExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn proc_status(pid: u32) -> (u64, u64) {
    let mut text = String::new();
    std::fs::File::open(format!("/proc/{pid}/status"))
        .unwrap()
        .take(65537)
        .read_to_string(&mut text)
        .unwrap();
    assert!(text.len() <= 65536);
    let value = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    (value("VmRSS:"), value("VmHWM:"))
}

fn cpu_ticks(pid: u32) -> u64 {
    let mut text = String::new();
    std::fs::File::open(format!("/proc/{pid}/stat"))
        .unwrap()
        .take(4097)
        .read_to_string(&mut text)
        .unwrap();
    assert!(text.len() <= 4096);
    let fields: Vec<_> = text
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

async fn collect(listener: tokio::net::TcpListener) {
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                assert!(head.len() < 4096);
                head.push(stream.read_u8().await.unwrap());
            }
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            let length: usize = head.lines().find_map(|line| line.strip_prefix("content-length: "))
                .unwrap().parse().unwrap();
            assert!(length <= 1_048_576);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            use prost::Message;
            let request = opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(body.as_slice()).unwrap();
            let spans = &request.resource_spans[0].scope_spans[0].spans;
            assert!(!spans.is_empty() && spans.len() <= 128);
            assert!(spans.iter().all(|span| span.name == "orishu.client.request"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        }).await.expect("bounded benchmark collector exchange");
    }
}

// Aborting/reaping the collector is explicit on success; Drop also cancels it
// when a request/assertion fails, instead of leaving a fixture running.
struct Collector(tokio::task::JoinHandle<()>);
impl Drop for Collector {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
#[ignore = "manual Linux measurement; run optimized, alone, with --nocapture"]
async fn measure_local_telemetry_overhead() {
    require_optimized_build();
    // V1 depends on the old synchronous final stderr accounting. Never
    // substitute pre-shutdown counters or silently change that report schema.
    let legacy_worker = std::env::var_os("ORISHU_BENCH_LEGACY_WORKER")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .expect("v1 overhead requires ORISHU_BENCH_LEGACY_WORKER: an explicit historical release binary with final stderr trace accounting; current M4 measurements require the reviewed v2 plan");
    // Explicit create-new artifact: preserve partial rounds on failure and
    // never replace another result. This IO is outside every timed window.
    let mut artifact = std::env::var_os("ORISHU_BENCH_REPORT").map(|path| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(180), async {
        let modes = [
            ("disabled", false, false, "0"),
            ("metrics", true, false, "0"),
            ("zero_sample", true, true, "0"),
            ("sample_1000ppm", true, true, "1000"),
            ("sample_all", true, true, "1000000"),
        ];
        for round in 0..3 {
            for offset in 0..modes.len() {
                let (name, metrics, tracing, sampling) = modes[(offset + round) % modes.len()];
                let root = tempfile::tempdir().unwrap();
                std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
                let state = root.path().join("state");
                let socket = root.path().join("api.sock");
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
                let mut collector = Collector(tokio::spawn(collect(listener)));
                let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let diagnostics = reservation.local_addr().unwrap();
                drop(reservation);
                let mut command = Command::new(&legacy_worker);
                for (key, _) in std::env::vars_os() {
                    if key.to_string_lossy().starts_with("ORISHU_") { command.env_remove(key); }
                }
                let mut worker = Worker(command.arg("--state-dir").arg(&state)
                    .arg("--listen.clients").arg(&socket)
                    .args(["--observability.enabled", if metrics {"true"} else {"false"},
                        "--observability.bind", &diagnostics.to_string(),
                        "--tracing.enabled", if tracing {"true"} else {"false"},
                        "--tracing.endpoint", &endpoint, "--tracing.sample-ppm", sampling])
                    .stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap());
                let initial = summary(&mut worker, &socket).await;
                let client = HttpClusterClient::new(ClusterAddress::UnixSocket(socket.clone()),
                    HttClientOptions { timeout: Some(Duration::from_secs(1)), ..Default::default() }).unwrap();
                for _ in 0..64 { client.cluster().summary().await.unwrap(); }
                let mut latency = Vec::with_capacity(100_000);
                let mut scrapes = Vec::with_capacity(100);
                let cpu_start = cpu_ticks(worker.0.id());
                let started = std::time::Instant::now();
                let mut next_scrape = Duration::ZERO;
                while latency.len() < 100_000 && started.elapsed() < Duration::from_secs(2) {
                    let request_start = std::time::Instant::now();
                    let view = client.cluster().summary().await.unwrap();
                    latency.push(request_start.elapsed().as_nanos() as u64);
                    assert_eq!(view.formation_id, initial.formation_id);
                    assert_eq!(view.source_node_id, initial.source_node_id);
                    assert_eq!(view.member_count, 1);
                    assert_eq!(view.alive_count, 1);
                    assert!(!view.membership_locked);
                    if metrics && started.elapsed() >= next_scrape {
                        let scrape_start = std::time::Instant::now();
                        let response = diagnostics_request_bounded(diagnostics, "/metrics", 32768).await;
                        assert!(response.starts_with("HTTP/1.1 200"));
                        scrapes.push(scrape_start.elapsed().as_nanos() as u64);
                        next_scrape = started.elapsed() + Duration::from_millis(250);
                    }
                }
                let elapsed = started.elapsed().as_secs_f64();
                let ticks = cpu_ticks(worker.0.id()) - cpu_start;
                let (rss, peak_rss) = proc_status(worker.0.id());
                assert!(!collector.0.is_finished(), "collector failed during measurement");
                worker.terminate().await;
                let mut reports = String::new();
                worker.0.stderr.take().unwrap().take(8193).read_to_string(&mut reports).unwrap();
                assert!(reports.len() <= 8192);
                assert!(!collector.0.is_finished(), "collector failed during drain");
                collector.0.abort();
                assert!((&mut collector.0).await.unwrap_err().is_cancelled());
                // Only fixed, process-generated trace accounting is included.
                let reports: Vec<_> = reports.lines().filter(|line|
                    line.starts_with("trace exporter stopped: ExportStats {") ||
                    line.starts_with("trace queue stopped: QueueStats {")).collect();
                assert_eq!(reports.len(), if tracing {2} else {0});
                latency.sort_unstable();
                scrapes.sort_unstable();
                let record = serde_json::json!({
                    "schema_version": 1,
                    "round": round, "mode": name, "requests": latency.len(), "seconds": elapsed,
                    "requests_per_second": latency.len() as f64 / elapsed,
                    "median_us": latency[latency.len()/2] as f64 / 1000.0,
                    "p95_us": latency[(latency.len()-1)*95/100] as f64 / 1000.0,
                    "worker_cpu_ticks": ticks, "rss_kib": rss, "peak_rss_kib": peak_rss,
                    "scrapes": scrapes.len(), "median_scrape_us": scrapes.get(scrapes.len()/2).map(|n| *n as f64 / 1000.0),
                    "final_trace_accounting": reports,
                });
                if let Some(file) = &mut artifact {
                    use std::io::Write;
                    serde_json::to_writer(&mut *file, &record).unwrap();
                    file.write_all(b"\n").unwrap();
                    file.flush().unwrap();
                }
                println!("OVERHEAD {record}");
            }
        }
    }).await.expect("whole overhead experiment deadline");
}

fn require_optimized_build() {
    #[cfg(debug_assertions)]
    panic!("measure with --release, not debug binaries");
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "measure with --release, not debug binaries")]
fn debug_measurement_is_refused() {
    require_optimized_build();
}
