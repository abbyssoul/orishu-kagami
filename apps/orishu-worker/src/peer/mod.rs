//! Bounded peer framing and session mechanics for the formation transport.
pub mod admission;
pub(crate) mod admission_replay;
pub mod catchup;
pub mod codec;
pub mod dial;
pub mod dial_metrics;
pub mod exchange;
pub mod exchange_metrics;
pub mod handshake;
pub mod ingress;
pub mod registry;
pub mod server;
pub mod session;
pub mod tls;
pub mod traffic;
pub mod wire;

#[cfg(test)]
mod readmission_timing_tests;

use std::time::Duration;

/// Per-operation deadline including the length prefix, payload and stream FIN.
pub const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// A bounded transfer error without peer payload or credential material.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    /// The peer did not complete the operation before its deadline.
    #[error("peer operation deadline exceeded")]
    Timeout,
    /// QUIC rejected or interrupted the operation.
    #[error("peer stream interrupted")]
    Transport,
    /// The bytes do not satisfy the bounded profile/schema.
    #[error(transparent)]
    Codec(#[from] codec::CodecError),
}

/// Receive exactly one bounded frame followed by FIN. Length is checked before
/// allocation; incomplete prefixes, payloads and missing FIN share one deadline.
pub async fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut quinn::RecvStream,
) -> Result<T, TransferError> {
    Ok(codec::decode(&read_payload(stream).await?)?)
}

/// Receive bounded raw CBOR for staged envelope/session validation in `wire`.
/// The caller must decode it before using any peer claims.
pub async fn read_payload(stream: &mut quinn::RecvStream) -> Result<Vec<u8>, TransferError> {
    read_payload_bounded(stream, codec::MAX_FRAME_BYTES).await
}

/// Receive a phase-specific frame cap, checked before payload allocation.
pub async fn read_payload_bounded(
    stream: &mut quinn::RecvStream,
    maximum: usize,
) -> Result<Vec<u8>, TransferError> {
    read_payload_metered(stream, maximum, &mut Default::default()).await
}

async fn read_payload_metered(
    stream: &mut quinn::RecvStream,
    maximum: usize,
    meter: &mut exchange_metrics::TransferCount,
) -> Result<Vec<u8>, TransferError> {
    tokio::time::timeout(FRAME_DEADLINE, async {
        let mut prefix = [0; 4];
        read_exact_metered(stream, &mut prefix, meter).await?;
        let length = codec::frame_length(prefix)?;
        if length > maximum {
            return Err(codec::CodecError::TooLarge.into());
        }
        let mut payload = vec![0; length];
        read_exact_metered(stream, &mut payload, meter).await?;
        let mut extra = [0];
        if let Some(bytes) = stream
            .read(&mut extra)
            .await
            .map_err(|_| TransferError::Transport)?
        {
            if meter.enabled() {
                meter.received(bytes);
            }
            return Err(codec::CodecError::Trailing.into());
        }
        Ok(payload)
    })
    .await
    .map_err(|_| TransferError::Timeout)?
}

/// Write one capped frame and finish this direction, without waiting indefinitely
/// for a slow peer to drain its receive window.
pub async fn write_frame<T: serde::Serialize>(
    stream: &mut quinn::SendStream,
    value: &T,
) -> Result<(), TransferError> {
    write_payload(stream, &codec::encode(value)?).await
}

/// Send pre-encoded CBOR (for example, `wire::Encoded`) without encoding a
/// second CBOR byte-string/array around the protocol envelope.
pub async fn write_payload(
    stream: &mut quinn::SendStream,
    payload: &[u8],
) -> Result<(), TransferError> {
    write_payload_metered(stream, payload, &mut Default::default()).await
}

async fn write_payload_metered(
    stream: &mut quinn::SendStream,
    payload: &[u8],
    meter: &mut exchange_metrics::TransferCount,
) -> Result<(), TransferError> {
    codec::validate(payload)?;
    tokio::time::timeout(FRAME_DEADLINE, async {
        write_all_metered(stream, &(payload.len() as u32).to_be_bytes(), meter).await?;
        write_all_metered(stream, payload, meter).await
    })
    .await
    .map_err(|_| TransferError::Timeout)??;
    stream.finish().map_err(|_| TransferError::Transport)
}

async fn read_exact_metered(
    stream: &mut quinn::RecvStream,
    mut bytes: &mut [u8],
    meter: &mut exchange_metrics::TransferCount,
) -> Result<(), TransferError> {
    if !meter.enabled() {
        return stream
            .read_exact(bytes)
            .await
            .map_err(|_| TransferError::Transport);
    }
    while !bytes.is_empty() {
        let count = stream
            .read(bytes)
            .await
            .map_err(|_| TransferError::Transport)?
            .filter(|count| *count != 0)
            .ok_or(TransferError::Transport)?;
        meter.received(count);
        bytes = &mut bytes[count..];
    }
    Ok(())
}

async fn write_all_metered(
    stream: &mut quinn::SendStream,
    mut bytes: &[u8],
    meter: &mut exchange_metrics::TransferCount,
) -> Result<(), TransferError> {
    if !meter.enabled() {
        return stream
            .write_all(bytes)
            .await
            .map_err(|_| TransferError::Transport);
    }
    while !bytes.is_empty() {
        let count = stream
            .write(bytes)
            .await
            .map_err(|_| TransferError::Transport)?;
        if count == 0 {
            return Err(TransferError::Transport);
        }
        meter.sent(count);
        bytes = &bytes[count..];
    }
    Ok(())
}
