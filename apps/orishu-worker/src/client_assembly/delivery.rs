//! Fixed-space HTTP/2 response delivery tracking on accepted plaintext writes.
use tokio::time::Instant;

use super::ASSEMBLY_BUDGET;

#[derive(Default)]
pub(super) struct Delivery {
    header: [u8; 9],
    header_len: usize,
    remaining: usize,
    started: Option<Instant>,
    ending_headers: Option<u32>,
    responses: [Option<(u32, Instant)>; 16],
    exhausted: bool,
}

impl Delivery {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.responses
            .iter()
            .flatten()
            .map(|(_, deadline)| *deadline)
            .chain(self.started.map(|start| start + ASSEMBLY_BUDGET))
            .min()
    }

    pub(super) fn exhausted(&self) -> bool {
        self.exhausted
    }

    pub(super) fn cancel(&mut self, id: u32) {
        if self.ending_headers == Some(id) {
            self.ending_headers = None;
        }
        for slot in &mut self.responses {
            if slot.is_some_and(|(stream, _)| stream == id) {
                *slot = None;
            }
        }
    }

    /// Observe only the prefix actually accepted by the underlying writer.
    /// Payload work is O(frames), metadata is capped at sixteen responses.
    pub(super) fn observe(&mut self, mut bytes: &[u8], now: Instant) {
        while !bytes.is_empty() {
            if self.header_len < 9 {
                self.started.get_or_insert(now);
                let count = bytes.len().min(9 - self.header_len);
                self.header[self.header_len..self.header_len + count]
                    .copy_from_slice(&bytes[..count]);
                self.header_len += count;
                bytes = &bytes[count..];
                if self.header_len == 9 {
                    self.remaining =
                        u32::from_be_bytes([0, self.header[0], self.header[1], self.header[2]])
                            as usize;
                    let id = self.stream();
                    if self.header[3] == 1
                        && !self
                            .responses
                            .iter()
                            .flatten()
                            .any(|(stream, _)| *stream == id)
                    {
                        if let Some(slot) = self.responses.iter_mut().find(|slot| slot.is_none()) {
                            *slot =
                                Some((id, self.started.expect("header started") + ASSEMBLY_BUDGET));
                        } else {
                            self.exhausted = true;
                        }
                    }
                    if self.remaining == 0 {
                        self.finish_frame();
                    }
                }
            } else {
                let count = bytes.len().min(self.remaining);
                bytes = &bytes[count..];
                self.remaining -= count;
                if self.remaining == 0 {
                    self.finish_frame();
                }
            }
        }
    }

    fn stream(&self) -> u32 {
        u32::from_be_bytes(self.header[5..].try_into().expect("fixed header")) & 0x7fff_ffff
    }

    fn finish_frame(&mut self) {
        let id = self.stream();
        match (self.header[3], self.header[4]) {
            (3, _) => self.cancel(id),
            (0, flags) if flags & 1 != 0 => self.cancel(id),
            (1, flags) if flags & 1 != 0 => {
                if flags & 4 != 0 {
                    self.cancel(id);
                } else {
                    self.ending_headers = Some(id);
                }
            }
            (9, flags) if flags & 4 != 0 && self.ending_headers == Some(id) => self.cancel(id),
            _ => {}
        }
        self.header_len = 0;
        self.started = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn head(id: u32, flags: u8) -> Vec<u8> {
        let mut bytes = vec![0, 0, 1, 1, flags];
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.push(0x88);
        bytes
    }

    #[test]
    fn end_stream_headers_wait_for_final_continuation_and_partial_header_has_a_budget() {
        let now = Instant::now();
        let mut delivery = Delivery::default();
        let headers = head(1, 1);
        delivery.observe(&headers[..1], now);
        assert_eq!(delivery.deadline(), Some(now + ASSEMBLY_BUDGET));
        delivery.observe(&headers[1..], now + Duration::from_secs(2));
        assert_eq!(delivery.deadline(), Some(now + ASSEMBLY_BUDGET));
        delivery.observe(&[0, 0, 1, 9, 4, 0, 0, 0, 1], now + Duration::from_secs(3));
        assert_eq!(delivery.deadline(), Some(now + ASSEMBLY_BUDGET));
        delivery.observe(&[42], now + Duration::from_secs(4));
        assert_eq!(delivery.deadline(), None);
    }

    #[test]
    fn response_deadline_survives_partial_data_and_other_streams() {
        let now = Instant::now();
        let mut delivery = Delivery::default();
        for byte in head(1, 4) {
            delivery.observe(&[byte], now);
        }
        let end = Some(now + ASSEMBLY_BUDGET);
        assert_eq!(delivery.deadline(), end);
        delivery.observe(
            &[0, 0, 1, 0, 0, 0, 0, 0, 1, 42],
            now + Duration::from_secs(2),
        );
        delivery.observe(&head(3, 5), now + Duration::from_secs(3));
        assert_eq!(delivery.deadline(), end);
        delivery.observe(&[0, 0, 1, 0, 1, 0, 0, 0, 1], now + Duration::from_secs(4));
        assert_eq!(delivery.deadline(), end, "END_STREAM payload must finish");
        delivery.observe(&[42], now + Duration::from_secs(4));
        assert_eq!(delivery.deadline(), None);
    }

    #[test]
    fn trailers_do_not_renew_deadline_and_reset_reclaims_capacity() {
        let now = Instant::now();
        let mut delivery = Delivery::default();
        for id in (1..32).step_by(2) {
            delivery.observe(&head(id, 4), now);
        }
        assert!(!delivery.exhausted());
        delivery.observe(&head(1, 4), now + Duration::from_secs(2));
        assert_eq!(delivery.deadline(), Some(now + ASSEMBLY_BUDGET));
        for id in (1..32).step_by(2) {
            delivery.cancel(id);
        }
        delivery.observe(&head(33, 4), now + Duration::from_secs(3));
        assert_eq!(delivery.deadline(), Some(now + Duration::from_secs(8)));
        assert!(!delivery.exhausted());
        delivery.observe(&[0, 0, 4, 3, 0, 0, 0, 0, 33, 0, 0, 0, 8], now);
        assert_eq!(delivery.deadline(), None);
    }

    #[test]
    fn seventeenth_pending_response_fails_closed_without_growing_metadata() {
        let now = Instant::now();
        let mut delivery = Delivery::default();
        for id in (1..=33).step_by(2) {
            delivery.observe(&head(id, 4), now);
        }
        assert!(delivery.exhausted());
        assert_eq!(delivery.responses.iter().flatten().count(), 16);
    }
}
