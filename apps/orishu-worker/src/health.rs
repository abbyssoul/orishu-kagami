//! Bounded membership-owner supervision, independent of diagnostic exporters.
//! This is one input to process probes, not a complete process readiness gate.

use orishu::model::cluster::Participation;
use std::time::Duration;
use tokio::time::Instant;

/// Five missed one-second owner supervision opportunities exhaust this budget.
/// This is a local control-loop threshold, not a peer or simulation deadline.
pub const OWNER_PROGRESS_DEADLINE: Duration = Duration::from_secs(5);

/// Process readiness reason; independent of the process-lifetime startup latch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// All locally required initialization and formation gates are satisfied.
    Ready,
    /// Configured listeners/roles have not all completed initialization.
    Initializing,
    /// A configured required role has exited; restart is required in this PoC.
    RequiredRoleUnavailable,
    /// The membership-owner readiness gate has not passed.
    Owner(OwnerHealth),
    /// Shutdown has been requested, even if the owner has not consumed it yet.
    Stopping,
}

/// Bounded local probe inputs. No network checks or membership transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHealth {
    /// Latched completion of local initialization, not continuing readiness.
    pub startup_complete: bool,
    /// Supervision-derived owner liveness, never exporter responsiveness.
    pub owner: OwnerHealth,
    /// Combined required-role, shutdown and formation readiness.
    pub readiness: Readiness,
}

#[derive(Default)]
pub(crate) struct ProcessHealthState(std::sync::atomic::AtomicU8);

impl ProcessHealthState {
    pub(crate) fn initialized(&self) {
        self.0.fetch_or(1, std::sync::atomic::Ordering::AcqRel);
    }

    pub(crate) fn role_failed(&self) {
        self.0.fetch_or(2, std::sync::atomic::Ordering::AcqRel);
    }

    pub(crate) fn snapshot(&self, owner: OwnerHealth, stopping: bool) -> ProcessHealth {
        let state = self.0.load(std::sync::atomic::Ordering::Acquire);
        ProcessHealth {
            startup_complete: state & 1 != 0,
            owner,
            readiness: if stopping {
                Readiness::Stopping
            } else if state & 2 != 0 {
                Readiness::RequiredRoleUnavailable
            } else if state & 1 == 0 {
                Readiness::Initializing
            } else if !owner.formation_ready() {
                Readiness::Owner(owner)
            } else {
                Readiness::Ready
            },
        }
    }
}

/// Finite, secret-free local owner health reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerHealth {
    /// The owner has not yet published its first supervision tick.
    Starting,
    /// The owner is responsive and its formation state can be served safely.
    Healthy,
    /// Joining, uncertain admission or catch-up is not formation-ready.
    Transitioning,
    /// The owner remains responsive but has been excluded from its formation.
    Ejected,
    /// Shutdown is progressing; no new formation service should be admitted.
    Stopping,
    /// Progress is absent beyond its budget (or the supplied time regressed).
    Stalled,
    /// The owner task no longer publishes state.
    Closed,
}

impl OwnerHealth {
    /// Whether owner supervision is current, independently of readiness.
    pub fn is_responsive(self) -> bool {
        matches!(
            self,
            Self::Healthy | Self::Transitioning | Self::Ejected | Self::Stopping
        )
    }

    /// Only the formation portion of readiness. Process initialization and
    /// required-role/listener health must also pass before `/readyz` succeeds.
    pub fn formation_ready(self) -> bool {
        self == Self::Healthy
    }

    pub(crate) fn evaluate(
        participation: Participation,
        progress: Option<Instant>,
        now: Instant,
        closed: bool,
    ) -> Self {
        if closed {
            return Self::Closed;
        }
        let Some(progress) = progress else {
            return Self::Starting;
        };
        if !now
            .checked_duration_since(progress)
            .is_some_and(|age| age < OWNER_PROGRESS_DEADLINE)
        {
            return Self::Stalled;
        }
        match participation {
            Participation::Standalone | Participation::Joined => Self::Healthy,
            Participation::Joining | Participation::JoinUnresolved | Participation::CatchingUp => {
                Self::Transitioning
            }
            Participation::Ejected => Self::Ejected,
            Participation::Stopping => Self::Stopping,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_latches_but_role_failure_and_shutdown_withhold_readiness() {
        let state = ProcessHealthState::default();
        assert!(!state.snapshot(OwnerHealth::Healthy, false).startup_complete);
        assert_eq!(
            state.snapshot(OwnerHealth::Healthy, false).readiness,
            Readiness::Initializing
        );
        state.initialized();
        assert_eq!(
            state.snapshot(OwnerHealth::Healthy, false).readiness,
            Readiness::Ready
        );
        assert_eq!(
            state.snapshot(OwnerHealth::Transitioning, false).readiness,
            Readiness::Owner(OwnerHealth::Transitioning)
        );
        state.role_failed();
        state.initialized(); // A late/repeated initializer cannot clear failure.
        let failed = state.snapshot(OwnerHealth::Healthy, false);
        assert!(failed.startup_complete);
        assert!(failed.owner.is_responsive());
        assert_eq!(failed.readiness, Readiness::RequiredRoleUnavailable);
        let stopping = state.snapshot(OwnerHealth::Closed, true);
        assert!(stopping.startup_complete);
        assert_eq!(stopping.readiness, Readiness::Stopping);
        assert!(!stopping.owner.is_responsive());
    }

    #[test]
    fn every_formation_phase_has_independent_liveness_and_readiness() {
        let now = Instant::now();
        for (phase, expected) in [
            (Participation::Standalone, OwnerHealth::Healthy),
            (Participation::Joined, OwnerHealth::Healthy),
            (Participation::Joining, OwnerHealth::Transitioning),
            (Participation::JoinUnresolved, OwnerHealth::Transitioning),
            (Participation::CatchingUp, OwnerHealth::Transitioning),
            (Participation::Ejected, OwnerHealth::Ejected),
            (Participation::Stopping, OwnerHealth::Stopping),
        ] {
            let health = OwnerHealth::evaluate(phase, Some(now), now, false);
            assert_eq!(health, expected);
            assert!(health.is_responsive());
            assert_eq!(health.formation_ready(), expected == OwnerHealth::Healthy);
        }
    }

    #[test]
    fn initialization_deadline_clock_and_owner_closure_fail_closed() {
        let now = Instant::now();
        let phase = Participation::Standalone;
        assert_eq!(
            OwnerHealth::evaluate(phase, None, now, false),
            OwnerHealth::Starting
        );
        assert_eq!(
            OwnerHealth::evaluate(
                phase,
                Some(now),
                now + OWNER_PROGRESS_DEADLINE - Duration::from_nanos(1),
                false
            ),
            OwnerHealth::Healthy
        );
        assert_eq!(
            OwnerHealth::evaluate(phase, Some(now), now + OWNER_PROGRESS_DEADLINE, false),
            OwnerHealth::Stalled
        );
        assert_eq!(
            OwnerHealth::evaluate(phase, Some(now + Duration::from_nanos(1)), now, false),
            OwnerHealth::Stalled
        );
        assert_eq!(
            OwnerHealth::evaluate(phase, Some(now), now, true),
            OwnerHealth::Closed
        );
        for health in [
            OwnerHealth::Starting,
            OwnerHealth::Stalled,
            OwnerHealth::Closed,
        ] {
            assert!(!health.is_responsive());
            assert!(!health.formation_ready());
        }
    }
}
