//! IO-shell scheduling only. Reconciliation still uses the core's existing
//! correlated, bounded exchange over an authorized member route.
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
pub(crate) struct Reconciliation {
    last: Option<Instant>,
}

impl Reconciliation {
    /// Called by the owner's skipped-tick one-second maintenance wakeup.
    /// No catch-up bursts, overlapping rounds, or changed SWIM selection.
    pub(crate) fn due(&mut self, now: Instant, queued: bool, active: bool) -> bool {
        let interval = Duration::from_secs(if queued { 1 } else { 5 });
        if active
            || self
                .last
                .is_some_and(|last| now.duration_since(last) < interval)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_news_uses_bounded_repair_then_returns_to_idle_cadence() {
        let start = Instant::now();
        let mut schedule = Reconciliation::default();
        assert!(schedule.due(start, false, false));
        assert!(!schedule.due(start + Duration::from_millis(999), true, false));
        assert!(schedule.due(start + Duration::from_secs(1), true, false));
        assert!(!schedule.due(start + Duration::from_secs(1), true, false));
        assert!(!schedule.due(start + Duration::from_secs(5), false, false));
        assert!(schedule.due(start + Duration::from_secs(6), false, false));
    }

    #[test]
    fn active_round_is_not_overlapped_or_given_a_new_deadline() {
        let start = Instant::now();
        let mut schedule = Reconciliation::default();
        assert!(schedule.due(start, true, false));
        for second in 1..20 {
            assert!(!schedule.due(start + Duration::from_secs(second), true, true));
        }
        assert!(schedule.due(start + Duration::from_secs(20), false, false));
        assert!(!schedule.due(start + Duration::from_secs(20), true, false));
    }

    #[test]
    fn long_pause_emits_only_one_round_not_missed_tick_replay() {
        let start = Instant::now();
        let mut schedule = Reconciliation::default();
        assert!(schedule.due(start, false, false));
        assert!(schedule.due(start + Duration::from_secs(60), true, false));
        for _ in 0..10 {
            assert!(!schedule.due(start + Duration::from_secs(60), true, false));
        }
    }
}
