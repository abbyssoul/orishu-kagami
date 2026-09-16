//! One complete portable body, reserved before reads. This is an IO adapter,
//! not an authenticated HTTP endpoint, resumable transfer or durable receipt.
use super::{AdmissionCandidate, AdmissionError, CancelOnDrop, PreparedAdmission, runtime};
use orishu_workload::WorkloadDigest;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::Instant;

const READ_QUANTUM: usize = 64 * 1024;
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// Host-owned delivery budget, never supplied as policy by an untrusted body.
#[derive(Clone, Copy, Debug)]
pub struct DeliveryLimits {
    /// Complete archive bytes, additionally bounded by admission archive policy.
    pub bytes: usize,
    /// Absolute wall-time bound including EOF; progress does not renew it.
    pub timeout: Duration,
}
impl Default for DeliveryLimits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024 * 1024,
            timeout: Duration::from_secs(30),
        }
    }
}

/// Stable input failures without untrusted bytes, paths or transport messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DeliveryError {
    /// Missing/truncated body or any bytes beyond its announced length.
    #[error("workload body length does not match the announced length")]
    Length,
    /// Underlying IO failed, before any scientific admission/epoch allocation.
    #[error("workload body transport failed")]
    Io,
    /// The absolute delivery window elapsed, including waiting for body EOF.
    #[error("workload body delivery deadline exceeded")]
    Deadline,
}

impl PreparedAdmission {
    /// Receive one complete portable workload body under this existing exclusive
    /// formation lease. Authenticate/authorize and validate request identity before
    /// preparing the lease. EOF must mean end of this body, not idle bytes on a
    /// reusable connection; HTTP/peer adapters must supply a body-bounded reader.
    ///
    /// Announced length is required, positive, host-bounded and checked before
    /// allocating or polling the reader. Storage is reserved once with a fallible
    /// allocation and filled without zero-initializing the unused region. Reads
    /// are capped at 64 KiB, plus one byte to reject overrun. O(body bytes) work,
    /// at most announced bytes of payload storage, no growing queue or disk cache.
    ///
    /// Cancellation/revocation/deadlines are checked during stalled reads as well
    /// as between chunks. Drop closes the owned reader and releases the lease;
    /// after JIT starts the blocking validation job instead retains the lease
    /// until it exits. Delivery success is not command/run acceptance. Root mismatch
    /// is refused after closure verification but before epoch allocation or JIT.
    pub async fn receive<R: AsyncRead + Unpin>(
        self,
        mut reader: R,
        announced_bytes: u64,
        expected_workload: WorkloadDigest,
        limits: DeliveryLimits,
    ) -> Result<AdmissionCandidate, AdmissionError> {
        runtime!(self.control.check())?;
        let length = usize::try_from(announced_bytes)
            .ok()
            .filter(|n| *n > 0 && *n <= limits.bytes && *n <= self.input_limit())
            .ok_or(AdmissionError::Limit)?;
        let deadline = Instant::now()
            .checked_add(limits.timeout)
            .ok_or(AdmissionError::Limit)?;
        if limits.timeout.is_zero() {
            return Err(DeliveryError::Deadline.into());
        }
        let mut cancellation = CancelOnDrop(Some(self.control.clone()));
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| AdmissionError::Limit)?;
        let mut pulse = tokio::time::interval(CANCEL_POLL);
        pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while bytes.len() < length {
            runtime!(self.control.check())?;
            if Instant::now() >= deadline {
                return Err(DeliveryError::Deadline.into());
            }
            let quantum = (length - bytes.len()).min(READ_QUANTUM) as u64;
            // The limiter cannot ask AsyncRead for space beyond this quantum;
            // Vec has capacity for the full bounded announced length already.
            let mut chunk = (&mut reader).take(quantum);
            let count = tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => return Err(DeliveryError::Deadline.into()),
                _ = pulse.tick() => continue,
                result = chunk.read_buf(&mut bytes) => result.map_err(|_| DeliveryError::Io)?,
            };
            if count == 0 {
                return Err(DeliveryError::Length.into());
            }
            // A ready in-memory/hostile reader must not monopolize the async
            // owner thread while delivering a large body without Pending.
            tokio::task::yield_now().await;
        }
        let mut extra = [0; 1];
        loop {
            runtime!(self.control.check())?;
            if Instant::now() >= deadline {
                return Err(DeliveryError::Deadline.into());
            }
            let count = tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => return Err(DeliveryError::Deadline.into()),
                _ = pulse.tick() => continue,
                result = reader.read(&mut extra) => result.map_err(|_| DeliveryError::Io)?,
            };
            if count != 0 {
                return Err(DeliveryError::Length.into());
            }
            break;
        }
        // The transport resource is not retained through guest compilation.
        drop(reader);
        runtime!(self.control.check())?;
        if Instant::now() >= deadline {
            return Err(DeliveryError::Deadline.into());
        }
        let result = self.validate_input(bytes, Some(expected_workload)).await;
        cancellation.0 = None;
        result
    }
}
