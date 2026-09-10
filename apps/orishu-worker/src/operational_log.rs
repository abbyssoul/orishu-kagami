//! Bounded operational log output, owned entirely by the worker IO shell.
//!
//! Producers encode only reviewed fixed fields and never wait for a lock or IO.
//! One dedicated OS thread owns the standard `Write` sink. No Tokio blocking
//! task, global stdout lock, ambient logger or fallback diagnostic is involved.
//! A sink may block forever: explicit shutdown sheds queued records at its
//! deadline and detaches the thread, reporting an in-flight record as uncertain.
//! Process termination, not thread cancellation, releases a permanently blocked
//! sink. This is best-effort diagnostics, not durable or exactly-once delivery.
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

mod record;
use record::Frame;
pub use record::{Event, Outcome, RECORD_BYTES, Record, TraceCounter};

/// Default number of preallocated completed-record slots (80 KiB payload).
pub const DEFAULT_QUEUE_RECORDS: usize = 256;
/// Hard upper bound for completed-record slots (1.25 MiB payload).
pub const MAX_QUEUE_RECORDS: usize = 4096;
/// Default maximum explicit shutdown drain time.
pub const DEFAULT_SHUTDOWN: Duration = Duration::from_millis(250);
/// Hard upper bound for a configured shutdown drain.
pub const MAX_SHUTDOWN: Duration = Duration::from_secs(2);
/// Requested writer-thread stack; OS minimums and guard pages are additional.
pub const WRITER_STACK_BYTES: usize = 256 * 1024;

/// Invalid budgets, allocation failure or failure to start the dedicated writer.
/// Errors deliberately contain no sink paths, credentials or OS error text.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Queue slots must be within the published fixed memory budget.
    #[error("invalid operational log queue budget")]
    QueueBudget,
    /// Shutdown must be between zero and the published maximum.
    #[error("invalid operational log shutdown budget")]
    ShutdownBudget,
    /// Bounded queue backing storage could not be reserved.
    #[error("operational log queue allocation failed")]
    Allocation,
    /// The OS writer thread could not be created.
    #[error("operational log writer thread could not start")]
    Thread,
    /// Duplicating standard output failed; no fallback destination is selected.
    #[error("operational log stdout descriptor unavailable")]
    Stdout,
}

#[derive(Default)]
struct Counters {
    accepted: AtomicU64,
    written: AtomicU64,
    queue_full: AtomicU64,
    contended: AtomicU64,
    encoding_failed: AtomicU64,
    invalid_source: AtomicU64,
    output_failed: AtomicU64,
    closed: AtomicU64,
    shutdown_dropped: AtomicU64,
}

fn add(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(amount))
    });
}

/// Constant-work independent counter snapshot, not a transactional audit.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Encoded records accepted into the queue, not delivered records.
    pub accepted: u64,
    /// Complete writes and flushes acknowledged by the sink, not durable storage.
    pub written: u64,
    /// Records refused because all queued slots were occupied.
    pub queue_full: u64,
    /// Records shed rather than waiting for another queue user.
    pub contended: u64,
    /// Records refused by the fixed encoder.
    pub encoding_failed: u64,
    /// Lifecycle records shed because Unix nanosecond time was unavailable.
    pub invalid_source: u64,
    /// Records not fully written/flushed, including queued records after sink failure.
    /// A failed record may have emitted a prefix; no subsequent records are written.
    pub output_failed: u64,
    /// Calls refused after shutdown, output failure, or queue poisoning.
    pub closed: u64,
    /// Queued, never-started records discarded at shutdown cutoff or guard drop.
    pub shutdown_dropped: u64,
}

struct State {
    queue: VecDeque<Frame>,
    closing: bool,
    abort: bool,
    failed: bool,
    in_flight: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    counters: Counters,
    capacity: usize,
}

/// Cloneable producer. No sink IO, sink locks or capacity waits occur here.
#[derive(Clone)]
pub struct Log {
    shared: Arc<Shared>,
}

impl Log {
    /// Emit fixed final trace accounting using a checked local clock. Output is
    /// best-effort through the same queue, never a fallback stderr report.
    pub fn trace_summary(&self, counters: [(TraceCounter, u64); 12]) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok());
        match timestamp {
            Some(timestamp) => {
                let frames = counters.map(|(counter, value)| {
                    Record::trace_counter(counter, value, timestamp).encode()
                });
                if frames.iter().any(Result::is_err) {
                    add(&self.shared.counters.encoding_failed, 12);
                } else {
                    let _ = self
                        .try_frames(frames.map(|frame| frame.expect("checked bounded encoding")));
                }
            }
            None => add(&self.shared.counters.invalid_source, 12),
        }
    }
    /// Emit a lifecycle event with a checked local timestamp, without trace
    /// ancestry. Call only when logging is enabled; there is no ambient logger.
    pub fn lifecycle(&self, event: Event, outcome: Outcome) {
        if let Some(record) = self.lifecycle_record(event, outcome) {
            let _ = self.try_record(record);
        }
    }

    fn lifecycle_record(&self, event: Event, outcome: Outcome) -> Option<Record> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok());
        match timestamp {
            Some(timestamp) => Some(Record::new(event, outcome, timestamp, None)),
            None => {
                add(&self.shared.counters.invalid_source, 1);
                None
            }
        }
    }
    /// Best-effort, constant-bounded encoding and enqueue. `true` means queued,
    /// never output acknowledgement. Contended queue access drops immediately.
    pub fn try_record(&self, record: Record) -> bool {
        let Ok(frame) = record.encode() else {
            add(&self.shared.counters.encoding_failed, 1);
            return false;
        };
        self.try_frames([frame])
    }

    // Only single events and the twelve-record final snapshot call this. One
    // queue transaction prevents the writer competing between summary fields.
    fn try_frames<const N: usize>(&self, frames: [Frame; N]) -> bool {
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::WouldBlock) => {
                add(&self.shared.counters.contended, N as u64);
                return false;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                add(&self.shared.counters.closed, N as u64);
                return false;
            }
        };
        let accepted = self.enqueue(&mut state, frames);
        drop(state);
        if accepted {
            self.shared.wake.notify_one();
        }
        accepted
    }

    fn enqueue<const N: usize>(&self, state: &mut State, frames: [Frame; N]) -> bool {
        if state.closing || state.failed {
            add(&self.shared.counters.closed, N as u64);
            return false;
        }
        if state.queue.len() + N > self.shared.capacity {
            add(&self.shared.counters.queue_full, N as u64);
            return false;
        }
        state.queue.extend(frames);
        add(&self.shared.counters.accepted, N as u64);
        true
    }

    /// Read independent saturating counts without acquiring any mutex.
    pub fn stats(&self) -> Stats {
        let c = &self.shared.counters;
        Stats {
            accepted: c.accepted.load(Ordering::Relaxed),
            written: c.written.load(Ordering::Relaxed),
            queue_full: c.queue_full.load(Ordering::Relaxed),
            contended: c.contended.load(Ordering::Relaxed),
            encoding_failed: c.encoding_failed.load(Ordering::Relaxed),
            invalid_source: c.invalid_source.load(Ordering::Relaxed),
            output_failed: c.output_failed.load(Ordering::Relaxed),
            closed: c.closed.load(Ordering::Relaxed),
            shutdown_dropped: c.shutdown_dropped.load(Ordering::Relaxed),
        }
    }
}

/// Result captured at shutdown cutoff. Counters may advance if a detached write
/// later completes; an uncertain record must never be reported as a definite loss.
#[derive(Debug)]
pub struct Shutdown {
    /// The writer finished, including sink destruction, before the drain deadline.
    pub writer_finished: bool,
    /// One write/flush was outstanding at cutoff (otherwise zero).
    pub unconfirmed: usize,
    /// Independent counters at cutoff.
    pub stats: Stats,
}

/// Owns process-lifetime output shutdown, never the sink itself. Drop aborts
/// queued work without joining the writer; use `shutdown` for a bounded drain.
pub struct Output {
    log: Log,
    finished: mpsc::Receiver<()>,
    shutdown: Duration,
}

impl Output {
    /// Start one replaceable standard-library writer on its own OS thread.
    /// Adapters must not log, panic, retain unbounded buffers, or hold global
    /// stdout/stderr locks. Once started, all writes, flushes and sink destruction
    /// stay there. A startup failure can drop the supplied sink on the caller.
    pub fn start<W: Write + Send + 'static>(
        sink: W,
        queued: usize,
        shutdown: Duration,
    ) -> Result<(Log, Self), StartError> {
        if !(1..=MAX_QUEUE_RECORDS).contains(&queued) {
            return Err(StartError::QueueBudget);
        }
        if shutdown > MAX_SHUTDOWN {
            return Err(StartError::ShutdownBudget);
        }
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(queued)
            .map_err(|_| StartError::Allocation)?;
        // Do not silently accept excess backing capacity from a collection implementation.
        if queue.capacity() > MAX_QUEUE_RECORDS {
            return Err(StartError::Allocation);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue,
                closing: false,
                abort: false,
                failed: false,
                in_flight: false,
            }),
            wake: Condvar::new(),
            counters: Counters::default(),
            capacity: queued,
        });
        let log = Log {
            shared: Arc::clone(&shared),
        };
        let (finished, receiver) = mpsc::sync_channel(1);
        // Dropping JoinHandle detaches; shutdown never joins a possibly stuck Write or Drop.
        let handle = thread::Builder::new()
            .name("orishu-log-output".into())
            .stack_size(WRITER_STACK_BYTES)
            .spawn(move || {
                pump(sink, &shared);
                let _ = finished.send(());
            })
            .map_err(|_| StartError::Thread)?;
        drop(handle);
        Ok((
            log.clone(),
            Self {
                log,
                finished: receiver,
                shutdown,
            },
        ))
    }

    /// Use an owned duplicate of stdout, not Rust's global buffered stdout lock.
    /// The duplicate preserves inherited flags; it does not change O_NONBLOCK on
    /// the shared open-file description. No file is opened or truncated.
    #[cfg(unix)]
    pub fn stdout(queued: usize, shutdown: Duration) -> Result<(Log, Self), StartError> {
        let descriptor = rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 0)
            .map_err(|_| StartError::Stdout)?;
        Self::start(std::fs::File::from(descriptor), queued, shutdown)
    }

    /// Stop accepting records and drain for at most the configured sink-wait
    /// interval. Call after stopping producers. Runtime scheduling and bounded
    /// queue bookkeeping are outside this IO wait budget. No fallback is printed.
    pub fn shutdown(self) -> Shutdown {
        self.finish(None)
    }

    /// Append `Stopped` while closing the queue after successful worker shutdown.
    /// Uses the same bookkeeping lock already needed by `shutdown`, not the
    /// lossy producer try-lock. Capacity, failed-output and drain limits remain
    /// unchanged: this does not promise delivery or wait for sink capacity.
    pub fn shutdown_stopped(self) -> Shutdown {
        let record = self
            .log
            .lifecycle_record(Event::Stopped, Outcome::Completed);
        self.finish(record)
    }

    fn finish(self, final_record: Option<Record>) -> Shutdown {
        let frame = final_record.and_then(|record| match record.encode() {
            Ok(frame) => Some(frame),
            Err(_) => {
                add(&self.log.shared.counters.encoding_failed, 1);
                None
            }
        });
        {
            let (mut state, poisoned) = match self.log.shared.state.lock() {
                Ok(state) => (state, false),
                Err(error) => (error.into_inner(), true),
            };
            if let Some(frame) = frame {
                if poisoned {
                    add(&self.log.shared.counters.closed, 1);
                } else {
                    let _ = self.log.enqueue(&mut state, [frame]);
                }
            }
            state.closing = true;
        }
        self.log.shared.wake.notify_one();
        let writer_finished = self.finished.recv_timeout(self.shutdown).is_ok();
        let report = {
            let mut state = self
                .log
                .shared
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.abort = true;
            add(
                &self.log.shared.counters.shutdown_dropped,
                state.queue.len() as u64,
            );
            state.queue.clear();
            Shutdown {
                writer_finished,
                unconfirmed: usize::from(state.in_flight),
                // Capture output counters while the writer cannot acknowledge
                // completion, keeping the pending-versus-written cut consistent.
                stats: self.log.stats(),
            }
        };
        self.log.shared.wake.notify_one();
        report
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let mut state = self
            .log
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.closing = true;
        state.abort = true;
        add(
            &self.log.shared.counters.shutdown_dropped,
            state.queue.len() as u64,
        );
        state.queue.clear();
        drop(state);
        self.log.shared.wake.notify_one();
    }
}

fn pump(mut sink: impl Write, shared: &Shared) {
    loop {
        let frame = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while state.queue.is_empty() && !state.closing {
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if state.abort {
                return;
            }
            let Some(frame) = state.queue.pop_front() else {
                return;
            };
            state.in_flight = true;
            frame
        };
        let result = write_record(&mut sink, frame.bytes());
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.in_flight = false;
        if result.is_err() {
            state.failed = true;
            // Never append a later JSON record after a possibly truncated prefix.
            add(&shared.counters.output_failed, 1 + state.queue.len() as u64);
            state.queue.clear();
            return;
        }
        add(&shared.counters.written, 1);
    }
}

// Unlike Write::write_all, repeated Interrupted responses have a finite work
// budget. Positive short writes consume at least one of <=320 remaining bytes.
fn write_record(sink: &mut impl Write, mut bytes: &[u8]) -> io::Result<()> {
    let mut interrupted = 0;
    while !bytes.is_empty() {
        match sink.write(bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) if count <= bytes.len() => bytes = &bytes[count..],
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted && interrupted < 8 => {
                interrupted += 1;
            }
            Err(error) => return Err(error),
        }
    }
    sink.flush()
}

#[cfg(test)]
mod tests;
