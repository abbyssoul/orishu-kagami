//! Absolute HTTP/2 input assembly and response delivery budgets on plaintext IO.
//!
//! This observer is not an HTTP validator: Hyper owns frame/HPACK semantics.
//! It retains frame headers, counters and bounded stream/deadline pairs, never
//! payloads, credentials or formation identity data.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use salvo::conn::{Accepted, Acceptor, ConnCtrl, Holding, tcp::TcpCoupler};
use salvo::fuse::ArcFusePolicy;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

mod delivery;
use delivery::Delivery;

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const ASSEMBLY_BUDGET: Duration = Duration::from_secs(5);

/// Applies the same observer outside Unix IO and the TLS handshake stream.
pub(crate) struct AssemblyAcceptor<A>(pub(crate) A);

impl<A> Acceptor for AssemblyAcceptor<A>
where
    A: Acceptor,
    A::Stream: AsyncRead + AsyncWrite,
{
    type Stream = AssemblyStream<A::Stream>;
    type Coupler = TcpCoupler<Self::Stream>;

    fn holdings(&self) -> &[Holding] {
        self.0.holdings()
    }

    async fn accept(
        &mut self,
        policy: Option<ArcFusePolicy>,
    ) -> io::Result<Accepted<Self::Coupler, Self::Stream>> {
        let accepted = self.0.accept(policy).await?;
        let control = accepted.conn_ctrl.clone();
        Ok(accepted.map_into(
            |_| TcpCoupler::new(),
            |stream| AssemblyStream::new(stream, control),
        ))
    }
}

#[derive(Default)]
struct Assembly {
    preface: usize,
    bypass: bool,
    header: [u8; 9],
    header_len: usize,
    remaining: usize,
    started: Option<Instant>,
    block_deadline: Option<Instant>,
}

impl Assembly {
    fn deadline(&self) -> Option<Instant> {
        let frame = self.started.map(|start| start + ASSEMBLY_BUDGET);
        match (frame, self.block_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// O(bytes), fixed space. Payload bytes are skipped without inspection.
    #[cfg(test)]
    fn observe(&mut self, bytes: &[u8], now: Instant) {
        self.observe_with(bytes, now, |_| {});
    }

    fn observe_with(
        &mut self,
        mut bytes: &[u8],
        now: Instant,
        mut completed: impl FnMut(&[u8; 9]),
    ) {
        while !bytes.is_empty() && !self.bypass {
            if self.preface < PREFACE.len() {
                self.started.get_or_insert(now);
                if bytes[0] != PREFACE[self.preface] {
                    // HTTP/1 has its own absolute head deadline. No upgrades
                    // to HTTP/2 are supported by the formation client surface.
                    self.bypass = true;
                    self.started = None;
                    return;
                }
                self.preface += 1;
                bytes = &bytes[1..];
                if self.preface == PREFACE.len() {
                    self.started = None;
                }
            } else if self.header_len < self.header.len() {
                self.started.get_or_insert(now);
                let count = bytes.len().min(self.header.len() - self.header_len);
                self.header[self.header_len..self.header_len + count]
                    .copy_from_slice(&bytes[..count]);
                self.header_len += count;
                bytes = &bytes[count..];
                if self.header_len == self.header.len() {
                    self.remaining = (usize::from(self.header[0]) << 16)
                        | (usize::from(self.header[1]) << 8)
                        | usize::from(self.header[2]);
                    if self.header[3] == 1 && self.block_deadline.is_none() {
                        self.block_deadline = self.started.map(|start| start + ASSEMBLY_BUDGET);
                    }
                    if self.remaining == 0 {
                        self.finish_frame(&mut completed);
                    }
                }
            } else {
                let count = bytes.len().min(self.remaining);
                self.remaining -= count;
                bytes = &bytes[count..];
                if self.remaining == 0 {
                    self.finish_frame(&mut completed);
                }
            }
        }
    }

    fn finish_frame(&mut self, completed: &mut impl FnMut(&[u8; 9])) {
        completed(&self.header);
        if matches!(self.header[3], 1 | 9) && self.header[4] & 4 != 0 {
            self.block_deadline = None;
        }
        self.header_len = 0;
        self.started = None;
    }
}

pub(crate) struct AssemblyStream<S> {
    inner: S,
    assembly: Assembly,
    delivery: Delivery,
    deadline: watch::Sender<Option<Instant>>,
    control: ConnCtrl,
    watchdog: JoinHandle<()>,
}

impl<S> AssemblyStream<S> {
    fn new(inner: S, control: ConnCtrl) -> Self {
        let (deadline, receiver) = watch::channel(None);
        let watchdog = tokio::spawn(supervise(receiver, control.clone()));
        Self {
            inner,
            assembly: Assembly::default(),
            delivery: Delivery::default(),
            deadline,
            control,
            watchdog,
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.assembly
            .deadline()
            .into_iter()
            .chain(self.delivery.deadline())
            .min()
    }

    fn publish_deadline(&self) {
        let next = self.next_deadline();
        self.deadline.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }

    fn check_deadline(&self) -> io::Result<()> {
        if self.control.is_aborted()
            || self.delivery.exhausted()
            || self
                .next_deadline()
                .is_some_and(|end| Instant::now() >= end)
        {
            self.control.abort();
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "client HTTP/2 assembly or delivery budget exhausted",
            ))
        } else {
            Ok(())
        }
    }
}

/// One bounded watcher per accepted connection, independent of decoder polling
/// and writes. The stream owns cancellation; no task survives connection drop.
async fn supervise(mut receiver: watch::Receiver<Option<Instant>>, control: ConnCtrl) {
    loop {
        let deadline = *receiver.borrow_and_update();
        if let Some(end) = deadline {
            tokio::select! {
                _ = tokio::time::sleep_until(end) => {
                    // Completion may have cleared/replaced the deadline in the
                    // same scheduler turn. Never expire the old snapshot.
                    if receiver.borrow().is_some_and(|latest| Instant::now() >= latest) {
                        control.abort();
                        return;
                    }
                }
                changed = receiver.changed() => if changed.is_err() { return; },
            }
        } else if receiver.changed().await.is_err() {
            return;
        }
    }
}

impl<S> Drop for AssemblyStream<S> {
    fn drop(&mut self) {
        self.watchdog.abort();
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for AssemblyStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline()?;
        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                this.check_deadline()?;
                this.assembly
                    .observe_with(&buf.filled()[before..], Instant::now(), |header| {
                        if header[3] == 3 {
                            this.delivery.cancel(
                                u32::from_be_bytes(header[5..].try_into().expect("fixed header"))
                                    & 0x7fff_ffff,
                            );
                        }
                    });
                this.publish_deadline();
                if this.assembly.bypass {
                    this.watchdog.abort();
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for AssemblyStream<S> {
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.check_deadline()?;
        match Pin::new(&mut this.inner).poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(count)) => {
                this.check_deadline()?;
                if !this.assembly.bypass {
                    let mut left = count;
                    let now = Instant::now();
                    for buf in bufs {
                        let used = left.min(buf.len());
                        this.delivery.observe(&buf[..used], now);
                        left -= used;
                        if left == 0 {
                            break;
                        }
                    }
                    this.publish_deadline();
                    this.check_deadline()?;
                }
                Poll::Ready(Ok(count))
            }
            other => other,
        }
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.check_deadline()?;
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(count)) => {
                this.check_deadline()?;
                if !this.assembly.bypass {
                    this.delivery.observe(&buf[..count], Instant::now());
                    this.publish_deadline();
                    this.check_deadline()?;
                }
                Poll::Ready(Ok(count))
            }
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline()?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmentation_preserves_first_byte_deadline_and_completion_clears_it() {
        let now = Instant::now();
        let mut wire = PREFACE.to_vec();
        wire.extend_from_slice(&[0, 0, 2, 1, 4, 0, 0, 0, 1, 0x82, 0x86]);
        for split in 1..wire.len() {
            let mut assembly = Assembly::default();
            assembly.observe(&wire[..split], now);
            assert_eq!(
                assembly.deadline(),
                (split != PREFACE.len()).then_some(now + ASSEMBLY_BUDGET)
            );
            assembly.observe(&wire[split..], now + Duration::from_secs(4));
            assert_eq!(assembly.deadline(), None);
            assert!(!assembly.bypass);
        }
    }

    #[test]
    fn continued_header_block_keeps_original_budget_between_complete_frames() {
        let now = Instant::now();
        let mut assembly = Assembly::default();
        assembly.observe(PREFACE, now);
        assembly.observe(&[0, 0, 0, 1, 1, 0, 0, 0, 1], now);
        for seconds in [1, 3, 4] {
            assembly.observe(
                &[0, 0, 0, 9, 0, 0, 0, 0, 1],
                now + Duration::from_secs(seconds),
            );
            assert_eq!(assembly.deadline(), Some(now + ASSEMBLY_BUDGET));
        }
        assembly.observe(&[0, 0, 0, 9, 4, 0, 0, 0, 1], now + Duration::from_secs(4));
        assert_eq!(assembly.deadline(), None);
        // A new request has a new budget; there is no total connection age cap.
        assembly.observe(&[0], now + Duration::from_secs(100));
        assert_eq!(assembly.deadline(), Some(now + Duration::from_secs(105)));
    }

    #[test]
    fn payload_and_continuation_trickle_cannot_reset_assembly() {
        let now = Instant::now();
        let mut assembly = Assembly::default();
        assembly.observe(PREFACE, now);
        assembly.observe(&[0, 0x40, 0, 1, 4, 0, 0, 0, 1], now);
        for seconds in 1..5 {
            assembly.observe(&[0x82], now + Duration::from_secs(seconds));
            assert_eq!(assembly.deadline(), Some(now + ASSEMBLY_BUDGET));
        }
        assert_eq!(assembly.remaining, 16380);
    }

    #[test]
    fn coalesced_frames_and_http1_do_not_leave_assembly_pending() {
        let now = Instant::now();
        let mut assembly = Assembly::default();
        assembly.observe(PREFACE, now);
        assembly.observe(&[0, 0, 0, 1, 4, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1], now);
        assert_eq!(assembly.deadline(), None);
        let mut http1 = Assembly::default();
        http1.observe(b"P", now);
        http1.observe(b"OST / HTTP/1.1\r\n", now);
        assert!(http1.bypass);
        assert_eq!(http1.deadline(), None);
    }

    #[tokio::test]
    async fn watchdog_expires_without_further_io_polling_and_drop_cancels_it() {
        let control = ConnCtrl::new();
        let (io, _peer) = tokio::io::duplex(32);
        let stream = AssemblyStream::new(io, control.clone());
        stream.deadline.send_replace(Some(Instant::now()));
        tokio::time::timeout(Duration::from_secs(1), async {
            while !control.is_aborted() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let (io, _peer) = tokio::io::duplex(32);
        let stream = AssemblyStream::new(io, ConnCtrl::new());
        let task = stream.watchdog.abort_handle();
        drop(stream);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn expired_input_is_refused_before_it_can_clear_the_deadline() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (io, mut peer) = tokio::io::duplex(32);
        let mut stream = AssemblyStream::new(io, ConnCtrl::new());
        stream
            .assembly
            .observe(b"P", Instant::now() - ASSEMBLY_BUDGET);
        peer.write_all(&PREFACE[1..]).await.unwrap();
        assert_eq!(
            stream.read_u8().await.unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(stream.assembly.preface, 1);
    }
}
