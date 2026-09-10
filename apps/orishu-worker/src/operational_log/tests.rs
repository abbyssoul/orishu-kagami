use super::*;
use std::time::Instant;

fn record() -> Record {
    Record::new(Event::Ready, Outcome::Completed, 123, None)
}

fn enqueue(log: &Log) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !log.try_record(record()) {
        assert!(
            Instant::now() < deadline,
            "test producer did not enqueue within budget"
        );
        thread::yield_now();
    }
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Held {
    entered: Option<mpsc::SyncSender<()>>,
    release: mpsc::Receiver<()>,
    output: Captured,
}

impl Write for Held {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            // The producer and shutdown must not wait for this test-controlled sink.
            self.release.recv().unwrap();
        }
        self.output.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

fn held(
    shutdown: Duration,
) -> (
    Log,
    Output,
    mpsc::Receiver<()>,
    mpsc::SyncSender<()>,
    Captured,
) {
    let (entered, reception) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let captured = Captured::default();
    let (log, output) = Output::start(
        Held {
            entered: Some(entered),
            release: gate,
            output: captured.clone(),
        },
        2,
        shutdown,
    )
    .unwrap();
    (log, output, reception, release, captured)
}

#[test]
fn queue_and_contention_shed_without_waiting_then_output_recovers() {
    let (log, output, entered, release, captured) = held(Duration::from_secs(1));
    enqueue(&log);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    enqueue(&log);
    enqueue(&log);
    assert!(!log.try_record(record()));
    assert_eq!(log.stats().queue_full, 1);
    let lock = log.shared.state.lock().unwrap();
    assert!(!log.try_record(record()));
    assert_eq!(log.stats().contended, 1);
    drop(lock);
    release.send(()).unwrap();
    let report = output.shutdown();
    assert!(report.writer_finished);
    assert_eq!(report.unconfirmed, 0);
    assert_eq!(report.stats.accepted, 3);
    assert_eq!(report.stats.written, 3);
    assert_eq!(report.stats.output_failed, 0);
    assert_eq!(report.stats.shutdown_dropped, 0);
    let bytes = captured.0.lock().unwrap();
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 3);
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let json: serde_json::Value = serde_json::from_slice(line).unwrap();
        assert_eq!(json["event"], "orishu.worker.ready");
    }
    assert!(!log.try_record(record()));
    assert_eq!(log.stats().closed, 1);
}

#[test]
fn final_trace_snapshot_enqueues_all_twelve_records_or_sheds_the_whole_batch() {
    let (entered, notification) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let captured = Captured::default();
    let (log, output) = Output::start(
        Held {
            entered: Some(entered),
            release: gate,
            output: captured.clone(),
        },
        12,
        Duration::from_secs(1),
    )
    .unwrap();
    enqueue(&log);
    notification.recv_timeout(Duration::from_secs(2)).unwrap();
    log.trace_summary([(TraceCounter::Accepted, u64::MAX); 12]);
    assert_eq!(log.stats().accepted, 13);
    log.trace_summary([(TraceCounter::Accepted, 0); 12]);
    assert_eq!(log.stats().accepted, 13);
    assert_eq!(log.stats().queue_full, 12);
    release.send(()).unwrap();
    assert_eq!(output.shutdown().stats.written, 13);
    let bytes = captured.0.lock().unwrap();
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 13);
}

#[test]
fn final_lifecycle_is_not_shed_by_queue_bookkeeping_contention() {
    let captured = Captured::default();
    let (log, output) = Output::start(captured.clone(), 32, Duration::from_secs(1)).unwrap();
    // Mirror main's final trace snapshot, then the successful shutdown event.
    log.trace_summary([(TraceCounter::Accepted, 0); 12]);
    let deadline = Instant::now() + Duration::from_secs(2);
    while log.stats().written != 12 {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    let queue = log.shared.state.lock().unwrap();
    let (entered, notification) = mpsc::sync_channel(1);
    let (finished, completion) = mpsc::sync_channel(1);
    let closer = thread::spawn(move || {
        entered.send(()).unwrap();
        finished.send(output.shutdown_stopped()).unwrap();
    });
    notification.recv_timeout(Duration::from_secs(2)).unwrap();
    // Closing already requires this bookkeeping lock. The final event must
    // participate in that transaction, not attempt a lossy producer beforehand.
    assert!(completion.recv_timeout(Duration::from_millis(100)).is_err());
    drop(queue);
    let report = completion.recv_timeout(Duration::from_secs(2)).unwrap();
    closer.join().unwrap();
    assert!(report.writer_finished);
    assert_eq!(report.unconfirmed, 0);
    assert_eq!(report.stats.contended, 0);
    assert_eq!(report.stats.written, 13);
    assert_eq!(report.stats.shutdown_dropped, 0);
    let bytes = captured.0.lock().unwrap();
    let last = bytes.split(|byte| *byte == b'\n').rev().nth(1).unwrap();
    let record: serde_json::Value = serde_json::from_slice(last).unwrap();
    assert_eq!(record["event"], "orishu.worker.stopped");
}

#[test]
fn final_lifecycle_preserves_capacity_and_held_sink_cutoff() {
    for queued in 0..=2 {
        let (log, output, entered, release, _) = held(Duration::from_millis(20));
        enqueue(&log);
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..queued {
            enqueue(&log);
        }
        let final_accepted = u64::from(queued < 2);
        let started = Instant::now();
        let report = output.shutdown_stopped();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!report.writer_finished);
        assert_eq!(report.unconfirmed, 1);
        assert_eq!(report.stats.accepted, 1 + queued + final_accepted);
        assert_eq!(report.stats.shutdown_dropped, queued + final_accepted);
        assert_eq!(report.stats.queue_full, u64::from(queued == 2));
        assert_eq!(report.stats.contended, 0);
        assert_eq!(report.stats.written, 0);
        release.send(()).unwrap();
    }
}

#[test]
fn final_lifecycle_does_not_reopen_failed_output() {
    let writes = Arc::new(AtomicU64::new(0));
    let (log, output) = Output::start(
        Broken {
            writes: writes.clone(),
            prefix: true,
        },
        2,
        Duration::from_secs(1),
    )
    .unwrap();
    enqueue(&log);
    let deadline = Instant::now() + Duration::from_secs(2);
    while log.stats().output_failed != 1 {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    let report = output.shutdown_stopped();
    assert!(report.writer_finished);
    assert_eq!(report.stats.closed, 1);
    assert_eq!(report.stats.output_failed, 1);
    assert_eq!(report.stats.written, 0);
    assert_eq!(writes.load(Ordering::Relaxed), 2);
}

#[test]
fn cutoff_distinguishes_never_started_loss_from_unconfirmed_write() {
    let (log, output, entered, release, _) = held(Duration::from_millis(20));
    enqueue(&log);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    enqueue(&log);
    enqueue(&log);
    let start = Instant::now();
    let report = output.shutdown();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!report.writer_finished);
    assert_eq!(report.unconfirmed, 1);
    assert_eq!(report.stats.shutdown_dropped, 2);
    assert_eq!(report.stats.written, 0);
    assert_eq!(report.stats.output_failed, 0);
    assert!(!log.try_record(record()));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while log.stats().written != 1 {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(log.stats().shutdown_dropped, 2);
}

#[test]
fn guard_drop_does_not_join_blocked_sink() {
    let (log, output, entered, release, _) = held(Duration::from_secs(2));
    enqueue(&log);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    enqueue(&log);
    let start = Instant::now();
    drop(output);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(log.stats().shutdown_dropped, 1);
    assert!(!log.try_record(record()));
    release.send(()).unwrap();
}

struct Broken {
    writes: Arc<AtomicU64>,
    prefix: bool,
}
impl Write for Broken {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        add(&self.writes, 1);
        if self.prefix {
            self.prefix = false;
            return Ok(bytes.len().min(3));
        }
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        unreachable!()
    }
}

#[test]
fn closed_or_partial_output_is_terminal_without_corrupt_following_lines() {
    for prefix in [false, true] {
        let writes = Arc::new(AtomicU64::new(0));
        let (log, output) = Output::start(
            Broken {
                writes: Arc::clone(&writes),
                prefix,
            },
            2,
            Duration::from_secs(1),
        )
        .unwrap();
        enqueue(&log);
        let report = output.shutdown();
        assert!(report.writer_finished);
        assert_eq!(report.unconfirmed, 0);
        assert_eq!(report.stats.written, 0);
        assert_eq!(report.stats.output_failed, 1);
        assert_eq!(writes.load(Ordering::Relaxed), if prefix { 2 } else { 1 });
        assert!(!log.try_record(record()));
        assert_eq!(log.stats().closed, 1);
    }
}

#[test]
fn writes_handle_short_progress_but_bound_interruption_work() {
    struct Short {
        bytes: Vec<u8>,
    }
    impl Write for Short {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.push(bytes[0]);
            Ok(1)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut short = Short { bytes: Vec::new() };
    write_record(&mut short, b"complete\n").unwrap();
    assert_eq!(short.bytes, b"complete\n");
    struct Interrupted(usize);
    impl Write for Interrupted {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            Err(io::ErrorKind::Interrupted.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            unreachable!()
        }
    }
    let mut interrupted = Interrupted(0);
    assert_eq!(
        write_record(&mut interrupted, b"one").unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(interrupted.0, 9);
}

#[test]
fn flush_failure_is_not_counted_as_written() {
    struct FailedFlush;
    impl Write for FailedFlush {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    let (log, output) = Output::start(FailedFlush, 1, Duration::from_secs(1)).unwrap();
    enqueue(&log);
    let report = output.shutdown();
    assert!(report.writer_finished);
    assert_eq!(report.stats.written, 0);
    assert_eq!(report.stats.output_failed, 1);
}

#[test]
fn closed_os_pipe_counts_failure_without_fallback_output() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let (log, output) = Output::start(writer, 1, Duration::from_secs(1)).unwrap();
    enqueue(&log);
    let report = output.shutdown();
    assert!(report.writer_finished);
    assert_eq!(report.unconfirmed, 0);
    assert_eq!(report.stats.written, 0);
    assert_eq!(report.stats.output_failed, 1);
}

#[test]
fn blocking_flush_and_sink_destructor_do_not_extend_shutdown_wait() {
    struct HeldCleanup {
        in_flush: bool,
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Write for HeldCleanup {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.in_flush {
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(())
        }
    }
    impl Drop for HeldCleanup {
        fn drop(&mut self) {
            if !self.in_flush {
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
            }
        }
    }
    for in_flush in [false, true] {
        let (entered, notification) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let (log, output) = Output::start(
            HeldCleanup {
                in_flush,
                entered,
                release: gate,
            },
            1,
            Duration::from_millis(20),
        )
        .unwrap();
        if in_flush {
            enqueue(&log);
            notification.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        let start = Instant::now();
        let report = output.shutdown();
        assert!(start.elapsed() < Duration::from_secs(1));
        if !in_flush {
            notification.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        assert!(!report.writer_finished);
        assert_eq!(report.unconfirmed, usize::from(in_flush));
        assert_eq!(report.stats.shutdown_dropped, 0);
        release.send(()).unwrap();
    }
}

#[test]
fn startup_checks_bounds_and_reserves_all_record_slots() {
    assert!(matches!(
        Output::start(io::sink(), 0, DEFAULT_SHUTDOWN),
        Err(StartError::QueueBudget)
    ));
    assert!(matches!(
        Output::start(io::sink(), MAX_QUEUE_RECORDS + 1, DEFAULT_SHUTDOWN),
        Err(StartError::QueueBudget)
    ));
    assert!(matches!(
        Output::start(io::sink(), 1, MAX_SHUTDOWN + Duration::from_millis(1)),
        Err(StartError::ShutdownBudget)
    ));
    let (log, output) =
        Output::start(io::sink(), MAX_QUEUE_RECORDS, Duration::from_secs(1)).unwrap();
    assert_eq!(
        log.shared.state.lock().unwrap().queue.capacity(),
        MAX_QUEUE_RECORDS
    );
    assert!(std::mem::size_of::<Frame>() <= RECORD_BYTES + std::mem::size_of::<usize>());
    assert!(output.shutdown().writer_finished);
}

#[cfg(unix)]
#[test]
fn unread_stdout_stays_open_through_subprocess_exit() {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "operational_log::tests::held_stdout_child",
            "--ignored",
            "--nocapture",
        ])
        .env("ORISHU_LOG_HELD_STDOUT_CHILD", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let unread_stdout = child.stdout.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("logging subprocess did not exit while stdout remained unread");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut evidence = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .take(4096)
        .read_to_string(&mut evidence)
        .unwrap();
    assert!(status.success(), "child failed: {evidence}");
    assert!(
        evidence.contains("held stdout: cutoff with unconfirmed=1 and queued loss"),
        "{evidence}"
    );
    // Crucially, do not drain or close the pipe to make the child exit.
    drop(unread_stdout);
}

#[cfg(unix)]
#[test]
#[ignore = "subprocess-only held stdout fixture"]
fn held_stdout_child() {
    if std::env::var_os("ORISHU_LOG_HELD_STDOUT_CHILD").is_none() {
        return;
    }
    let (log, output) = Output::stdout(2, Duration::from_millis(20)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut stable = Instant::now();
    let mut written = 0;
    loop {
        let _ = log.try_record(record());
        let now_written = log.stats().written;
        if now_written != written {
            written = now_written;
            stable = Instant::now();
        }
        if written > 0 && stable.elapsed() >= Duration::from_millis(100) {
            let state = log.shared.state.lock().unwrap();
            if state.in_flight && state.queue.len() == 2 {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "stdout did not reach observed pressure"
        );
        thread::yield_now();
    }
    let report = output.shutdown();
    assert!(!report.writer_finished);
    assert_eq!(report.unconfirmed, 1);
    assert_eq!(report.stats.shutdown_dropped, 2);
    assert!(report.stats.queue_full > 0);
    eprintln!("held stdout: cutoff with unconfirmed=1 and queued loss");
    // Avoid libtest's own post-test stdout report; this still executes Rust's
    // standard process-exit cleanup while the real log write remains blocked.
    std::process::exit(0);
}
