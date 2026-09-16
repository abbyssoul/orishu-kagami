use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Host-owned interruption, never scientific time or input to a kernel.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum Interruption {
    /// The caller cancelled this operation, not every store using the engine.
    #[error("kernel operation cancelled")]
    Cancelled,
    /// Host wall-time policy expired without publishing a candidate.
    #[error("kernel operation deadline exceeded")]
    DeadlineExceeded,
}
/// Independently cancellable operation with a finite wall-time deadline.
#[derive(Clone, Debug)]
pub struct OperationControl {
    cancelled: Cancellation,
    parent: Option<Cancellation>,
    deadline: Instant,
}

/// Monotonic, thread-safe host cancellation. It has no clock, reset or guest
/// authority; cloned handles share one lifetime. Cancelling an operation linked
/// to it does not cancel this parent lifetime.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    /// Permanently revoke this lifetime, including all linked operations.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    /// Read a cancellation hint; this is not an atomic scientific commit guard.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
impl OperationControl {
    /// Bound a new operation. Excessively large durations are rejected explicitly.
    pub fn new(timeout: Duration) -> wasmtime::Result<Self> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| wasmtime::Error::msg("operation deadline overflow"))?;
        Ok(Self {
            cancelled: Cancellation::default(),
            parent: None,
            deadline,
        })
    }
    /// Cancel only operations sharing this token. Safe from another OS thread.
    pub fn cancel(&self) {
        self.cancelled.cancel();
    }
    /// Link one additional host lifetime without changing this operation's
    /// deadline or cancellation token. Reject replacing an existing parent;
    /// parents cannot be silently dropped or chained into an unbounded graph.
    pub fn with_parent(mut self, parent: Cancellation) -> wasmtime::Result<Self> {
        if self.parent.is_some() {
            return Err(wasmtime::Error::msg(
                "operation already has a parent cancellation",
            ));
        }
        self.parent = Some(parent);
        Ok(self)
    }
    pub(crate) fn bounded(&self, timeout: Duration) -> wasmtime::Result<Self> {
        let policy = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| wasmtime::Error::msg("operation deadline overflow"))?;
        Ok(Self {
            cancelled: self.cancelled.clone(),
            parent: self.parent.clone(),
            deadline: self.deadline.min(policy),
        })
    }
    /// Refuse work whose caller cancelled it or whose deadline has expired.
    /// Adapters should check before expensive preparation as well as invoking
    /// guest operations. This does not make native JIT compilation interruptible.
    pub fn check(&self) -> wasmtime::Result<()> {
        if self.cancelled.is_cancelled()
            || self.parent.as_ref().is_some_and(Cancellation::is_cancelled)
        {
            return Err(Interruption::Cancelled.into());
        }
        if Instant::now() >= self.deadline {
            return Err(Interruption::DeadlineExceeded.into());
        }
        Ok(())
    }
}

/// One engine heartbeat. Each store decides independently whether to trap;
/// observer cancellation cannot interrupt another operation's scientific work.
pub(crate) struct EpochTicker {
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl EpochTicker {
    pub(crate) fn new(engine: wasmtime::Engine) -> wasmtime::Result<Self> {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let worker = stop.clone();
        let thread = std::thread::Builder::new()
            .name("orishu-wasm-epoch".into())
            .spawn(move || {
                let (mutex, wake) = &*worker;
                let mut stopped = mutex.lock().unwrap_or_else(|p| p.into_inner());
                while !*stopped {
                    let (next, _) = wake
                        .wait_timeout(stopped, Duration::from_millis(5))
                        .unwrap_or_else(|p| p.into_inner());
                    stopped = next;
                    if !*stopped {
                        engine.increment_epoch();
                    }
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for EpochTicker {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.stop;
        *mutex.lock().unwrap_or_else(|p| p.into_inner()) = true;
        wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_monotonic_bounded_and_preserves_child_independence() {
        let parent = Cancellation::default();
        let first = OperationControl::new(Duration::from_secs(1))
            .unwrap()
            .with_parent(parent.clone())
            .unwrap();
        let second = OperationControl::new(Duration::from_secs(1))
            .unwrap()
            .with_parent(parent.clone())
            .unwrap();
        first.cancel();
        assert!(!parent.is_cancelled());
        assert!(first.check().is_err());
        assert!(second.check().is_ok());
        let bounded = second.bounded(Duration::from_millis(500)).unwrap();
        assert!(second.clone().with_parent(Cancellation::default()).is_err());
        parent.cancel();
        assert!(second.check().is_err());
        assert!(bounded.check().is_err());
        assert!(
            OperationControl::new(Duration::from_secs(1))
                .unwrap()
                .with_parent(parent)
                .unwrap()
                .check()
                .is_err()
        );
    }
}
