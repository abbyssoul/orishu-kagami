//! Typed bounds on everything a hostile peer could try to make unbounded.
//!
//! Every limit is validated once, at construction, and then carried in the
//! model. Downstream code reads an accessor and never re-derives a bound from
//! a magic number, so there is exactly one place to audit when asking "what
//! stops one message from allocating a gigabyte?".
//!
//! # Division of responsibility with the wire decoder
//!
//! These limits are defence in depth, not the first line of defence. By the
//! time a [`crate::Message`] reaches the core, its `Vec`s are already
//! allocated: re-checking a length here catches a decoder bug and prevents the
//! oversized value from being *adopted into state*, but it cannot un-allocate
//! memory the decoder already committed. The decoder obligations that remain
//! mandatory are listed in the crate documentation.

use serde::{Deserialize, Serialize};

/// A timer delay in milliseconds.
///
/// Deliberately a plain scalar rather than a `std::time::Duration`: the core
/// never reads a clock, it only tells the shell *how long* a timer should be.
/// Keeping the unit in the type stops a caller passing seconds by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurationMillis(pub u64);

impl std::fmt::Display for DurationMillis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

/// Declares the limit fields once, generating the plain-data specification,
/// its protocol defaults, and the read-only accessors on the validated type.
macro_rules! limits {
    (
        $(
            $(#[$meta:meta])*
            $name:ident : $ty:ty = $default:expr
        ),* $(,)?
    ) => {
        /// Plain, unvalidated limit values.
        ///
        /// This is the input side of "parse, don't validate": construct a
        /// `LimitsSpec` (usually from configuration), then convert it into
        /// [`Limits`], which cannot exist in an inconsistent state.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct LimitsSpec {
            $(
                $(#[$meta])*
                pub $name: $ty,
            )*
        }

        impl Default for LimitsSpec {
            /// The defaults documented in `docs/protocol-p2p.md`, with
            /// memory-facing caps chosen for a single worker process.
            fn default() -> Self {
                Self { $( $name: $default, )* }
            }
        }

        /// Validated limits.
        ///
        /// Fields are private and reachable only through accessors, so no
        /// caller can mutate one bound out of agreement with another after the
        /// cross-field checks in [`Limits::try_from`] have run.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Limits {
            spec: LimitsSpec,
        }

        impl Limits {
            $(
                $(#[$meta])*
                #[must_use]
                pub fn $name(&self) -> $ty {
                    self.spec.$name
                }
            )*

            /// The unvalidated values behind this instance, for diagnostics
            /// and for round-tripping configuration.
            #[must_use]
            pub fn spec(&self) -> &LimitsSpec {
                &self.spec
            }
        }
    };
}

limits! {
    /// Maximum members, including the local node, that this formation admits.
    /// Doubles as the admission capacity gate.
    max_members: usize = 4096,

    /// Maximum member records accepted from a bootstrap snapshot in a
    /// `JoinReply`. An introducer that offers more is not trusted to be
    /// telling the truth about the formation's size.
    max_snapshot_members: usize = 4096,

    /// Maximum encoded size of a bootstrap snapshot, in bytes, as reported by
    /// the decoder. Bounds the work of validating a snapshot independently of
    /// its item count.
    max_snapshot_bytes: usize = 1_048_576,

    /// Maximum gossip deltas attached to one outgoing message.
    max_gossip_per_message: usize = 10,

    /// Maximum gossip deltas accepted from one incoming message. Kept equal to
    /// the send limit so a peer cannot use us as an amplifier.
    max_gossip_per_inbound_message: usize = 10,

    /// Maximum deltas retained in the retirement queue. When full, the delta
    /// closest to retirement (highest hop count) is evicted first.
    max_gossip_queue: usize = 512,

    /// Hops after which a delta is retired. `0` selects the protocol's
    /// automatic value, `ceil(3 * log2(N))` over alive members.
    max_gossip_hops: u32 = 0,

    /// Maximum deltas in one `PullReply` before it is truncated and a
    /// continuation cursor is returned.
    max_anti_entropy_entries: usize = 1000,

    /// Depth of the canonical anti-entropy hash tree, giving `2^depth`
    /// buckets. Must be in `1..=8`.
    anti_entropy_depth: u8 = 4,

    /// Maximum `PullReq`/`PullReply` exchanges in one anti-entropy round
    /// before it is abandoned. Stops a peer keeping us in an endless
    /// continuation loop by always replying `complete: false`.
    max_anti_entropy_rounds: u32 = 16,

    /// Maximum advertised addresses accepted per node, per address family
    /// (peer and client are counted separately).
    max_addresses_per_node: usize = 8,

    /// Maximum length of one advertised address, in bytes.
    max_address_len: usize = 255,

    /// Maximum length of any other free-form text the core stores, in bytes:
    /// architecture strings, accelerator names, engine identifiers, rejection
    /// reasons.
    max_text_len: usize = 253,

    /// Maximum accelerator entries accepted in a node's advertised
    /// capabilities.
    max_accelerators: usize = 32,

    /// Maximum engine entries accepted in a node's advertised capabilities.
    max_engines: usize = 16,

    /// Maximum redirect hints accepted from a `JoinReply`. Redirects are
    /// untrusted candidates, so the bound is small.
    max_redirects: usize = 8,

    /// Maximum probes in flight, across direct, indirect, and relayed roles.
    max_pending_probes: usize = 64,

    /// Maximum admission decisions parked awaiting a correlated effect
    /// outcome.
    max_pending_joins: usize = 64,

    /// Maximum blocklist entries retained.
    max_blocklist_entries: usize = 1024,

    /// Maximum tombstones retained.
    max_tombstones: usize = 4096,

    /// Cap on effects emitted by one call to `update`. One message must not
    /// produce unbounded fan-out.
    max_effects_per_update: usize = 64,

    /// Cap on diagnostics emitted by one call to `update`.
    max_diagnostics_per_update: usize = 32,

    /// Number of intermediaries asked to probe a target indirectly.
    indirect_probe_count: usize = 3,

    /// Time to wait for a direct `Ack`.
    probe_timeout: DurationMillis = DurationMillis(500),

    /// Time to wait for any `PingReply` from the chosen intermediaries.
    indirect_probe_timeout: DurationMillis = DurationMillis(1_000),

    /// Base suspicion timeout, before cluster-size scaling.
    suspicion_timeout: DurationMillis = DurationMillis(5_000),

    /// Multiplier applied to the size-scaled suspicion timeout.
    suspicion_multiplier: u32 = 3,

    /// Time to wait for a `PullReply` before abandoning an anti-entropy round.
    anti_entropy_timeout: DurationMillis = DurationMillis(10_000),

    /// Base backoff before retrying a join. Doubles per attempt, capped by
    /// [`LimitsSpec::max_join_backoff`].
    join_backoff: DurationMillis = DurationMillis(1_000),

    /// Ceiling on the exponential join backoff.
    max_join_backoff: DurationMillis = DurationMillis(60_000),

    /// Maximum join attempts before the local node gives up and reports
    /// failure to its operator.
    max_join_attempts: u32 = 8,
}

/// Why a [`LimitsSpec`] does not describe a workable configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitsError {
    /// A limit that must admit at least one item was zero.
    #[error("limit `{limit}` must be greater than zero")]
    Zero {
        /// Name of the offending limit.
        limit: &'static str,
    },

    /// A limit fell outside its permitted range.
    #[error("limit `{limit}` is {value}, outside the permitted range {min}..={max}")]
    OutOfRange {
        /// Name of the offending limit.
        limit: &'static str,
        /// Value supplied.
        value: u64,
        /// Lowest permitted value.
        min: u64,
        /// Highest permitted value.
        max: u64,
    },

    /// Two limits are individually valid but mutually inconsistent.
    #[error("limit `{limit}` ({value}) must not exceed `{bound}` ({bound_value})")]
    Inconsistent {
        /// Name of the offending limit.
        limit: &'static str,
        /// Value supplied for it.
        value: u64,
        /// Name of the limit it must not exceed.
        bound: &'static str,
        /// Value supplied for the bound.
        bound_value: u64,
    },
}

/// Smallest effect budget that can still express the widest single transition:
/// an indirect probe fans out to `indirect_probe_count` sends plus one timer,
/// and admission emits a reply, a publish, and a gossip broadcast.
const MIN_EFFECT_HEADROOM: usize = 4;

impl TryFrom<LimitsSpec> for Limits {
    type Error = LimitsError;

    /// Validates a specification into usable limits.
    ///
    /// # Errors
    ///
    /// Returns [`LimitsError`] when a limit is zero where at least one item is
    /// required, when the anti-entropy depth is outside `1..=8`, or when two
    /// limits contradict each other.
    fn try_from(spec: LimitsSpec) -> Result<Self, Self::Error> {
        fn non_zero(limit: &'static str, value: usize) -> Result<(), LimitsError> {
            if value == 0 {
                Err(LimitsError::Zero { limit })
            } else {
                Ok(())
            }
        }

        fn at_most(
            limit: &'static str,
            value: usize,
            bound: &'static str,
            bound_value: usize,
        ) -> Result<(), LimitsError> {
            if value > bound_value {
                Err(LimitsError::Inconsistent {
                    limit,
                    value: value as u64,
                    bound,
                    bound_value: bound_value as u64,
                })
            } else {
                Ok(())
            }
        }

        non_zero("maxMembers", spec.max_members)?;
        non_zero("maxSnapshotMembers", spec.max_snapshot_members)?;
        non_zero("maxSnapshotBytes", spec.max_snapshot_bytes)?;
        non_zero("maxGossipPerMessage", spec.max_gossip_per_message)?;
        non_zero(
            "maxGossipPerInboundMessage",
            spec.max_gossip_per_inbound_message,
        )?;
        non_zero("maxGossipQueue", spec.max_gossip_queue)?;
        non_zero("maxAntiEntropyEntries", spec.max_anti_entropy_entries)?;
        non_zero("maxAddressesPerNode", spec.max_addresses_per_node)?;
        non_zero("maxAddressLen", spec.max_address_len)?;
        non_zero("maxTextLen", spec.max_text_len)?;
        non_zero("maxPendingProbes", spec.max_pending_probes)?;
        non_zero("maxPendingJoins", spec.max_pending_joins)?;
        non_zero("maxTombstones", spec.max_tombstones)?;
        non_zero("maxBlocklistEntries", spec.max_blocklist_entries)?;
        non_zero("maxEffectsPerUpdate", spec.max_effects_per_update)?;
        non_zero("maxDiagnosticsPerUpdate", spec.max_diagnostics_per_update)?;
        non_zero("indirectProbeCount", spec.indirect_probe_count)?;

        if spec.max_anti_entropy_rounds == 0 {
            return Err(LimitsError::Zero {
                limit: "maxAntiEntropyRounds",
            });
        }
        if spec.max_join_attempts == 0 {
            return Err(LimitsError::Zero {
                limit: "maxJoinAttempts",
            });
        }
        if spec.suspicion_multiplier == 0 {
            return Err(LimitsError::Zero {
                limit: "suspicionMultiplier",
            });
        }

        // The tree must have at least two buckets to be a tree at all, and
        // 2^8 = 256 buckets is already more than one datagram of hashes.
        if !(1..=8).contains(&spec.anti_entropy_depth) {
            return Err(LimitsError::OutOfRange {
                limit: "antiEntropyDepth",
                value: u64::from(spec.anti_entropy_depth),
                min: 1,
                max: 8,
            });
        }

        at_most(
            "maxGossipPerMessage",
            spec.max_gossip_per_message,
            "maxGossipQueue",
            spec.max_gossip_queue,
        )?;
        at_most(
            "maxSnapshotMembers",
            spec.max_snapshot_members,
            "maxMembers",
            spec.max_members,
        )?;
        // An indirect probe fans out to one send per intermediary plus timers;
        // an effect budget below that would truncate a correctness-relevant
        // transition rather than merely a diagnostic one.
        at_most(
            "indirectProbeCount",
            spec.indirect_probe_count + MIN_EFFECT_HEADROOM,
            "maxEffectsPerUpdate",
            spec.max_effects_per_update,
        )?;

        Ok(Self { spec })
    }
}

impl Default for Limits {
    fn default() -> Self {
        // The default specification is validated by `default_spec_is_valid`.
        Self::try_from(LimitsSpec::default()).expect("default limits are valid")
    }
}

impl Limits {
    /// Suspicion timeout scaled for a formation with `alive` alive members.
    ///
    /// `base * ceil(log2(alive + 1)) * multiplier`, as `docs/protocol-p2p.md`
    /// specifies. Scaling matters because gossip needs more rounds to reach
    /// every member of a larger formation, and a fixed timeout would turn that
    /// into false-positive failure detection. Arithmetic saturates rather than
    /// overflowing, so an absurd configuration produces a very long timeout
    /// instead of a panic or a wrapped, near-zero one.
    #[must_use]
    pub fn effective_suspicion_timeout(&self, alive: usize) -> DurationMillis {
        let scale = u64::from(ceil_log2(alive.saturating_add(1)));
        DurationMillis(
            self.spec
                .suspicion_timeout
                .0
                .saturating_mul(scale.max(1))
                .saturating_mul(u64::from(self.spec.suspicion_multiplier)),
        )
    }

    /// Hop count after which a gossip delta is retired.
    ///
    /// A configured `0` selects the protocol's automatic value,
    /// `ceil(3 * log2(N))`, which grows the dissemination window just fast
    /// enough to keep infection-style spread reaching every member.
    #[must_use]
    pub fn effective_gossip_hops(&self, alive: usize) -> u32 {
        if self.spec.max_gossip_hops > 0 {
            return self.spec.max_gossip_hops;
        }
        ceil_log2(alive.max(1)).saturating_mul(3).max(1)
    }

    /// Backoff before join attempt number `attempt` (one-based), doubling per
    /// attempt and capped by [`Limits::max_join_backoff`].
    #[must_use]
    pub fn join_backoff_for(&self, attempt: u32) -> DurationMillis {
        let doublings = attempt.saturating_sub(1).min(u32::BITS - 1);
        let scaled = self
            .spec
            .join_backoff
            .0
            .saturating_mul(1u64 << doublings.min(63));
        DurationMillis(scaled.min(self.spec.max_join_backoff.0))
    }

    /// Number of buckets in the anti-entropy hash tree, `2^depth`.
    #[must_use]
    pub fn anti_entropy_buckets(&self) -> usize {
        1usize << self.spec.anti_entropy_depth
    }
}

/// `ceil(log2(value))` for `value >= 1`, without floating point.
fn ceil_log2(value: usize) -> u32 {
    match value {
        0 | 1 => 0,
        _ => usize::BITS - (value - 1).leading_zeros(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_is_valid() {
        assert!(Limits::try_from(LimitsSpec::default()).is_ok());
    }

    #[test]
    fn zero_capacity_is_rejected() {
        let spec = LimitsSpec {
            max_members: 0,
            ..LimitsSpec::default()
        };
        assert_eq!(
            Limits::try_from(spec),
            Err(LimitsError::Zero {
                limit: "maxMembers"
            })
        );
    }

    #[test]
    fn anti_entropy_depth_is_bounded() {
        for depth in [0u8, 9, 64] {
            let spec = LimitsSpec {
                anti_entropy_depth: depth,
                ..LimitsSpec::default()
            };
            assert!(matches!(
                Limits::try_from(spec),
                Err(LimitsError::OutOfRange {
                    limit: "antiEntropyDepth",
                    ..
                })
            ));
        }
        assert_eq!(
            Limits::try_from(LimitsSpec {
                anti_entropy_depth: 4,
                ..LimitsSpec::default()
            })
            .unwrap()
            .anti_entropy_buckets(),
            16
        );
    }

    #[test]
    fn a_queue_smaller_than_one_message_is_inconsistent() {
        let spec = LimitsSpec {
            max_gossip_per_message: 32,
            max_gossip_queue: 8,
            ..LimitsSpec::default()
        };
        assert!(matches!(
            Limits::try_from(spec),
            Err(LimitsError::Inconsistent {
                limit: "maxGossipPerMessage",
                ..
            })
        ));
    }

    #[test]
    fn an_effect_budget_below_probe_fan_out_is_inconsistent() {
        let spec = LimitsSpec {
            indirect_probe_count: 8,
            max_effects_per_update: 4,
            ..LimitsSpec::default()
        };
        assert!(matches!(
            Limits::try_from(spec),
            Err(LimitsError::Inconsistent {
                limit: "indirectProbeCount",
                ..
            })
        ));
    }

    #[test]
    fn ceil_log2_matches_its_definition() {
        assert_eq!(ceil_log2(0), 0);
        assert_eq!(ceil_log2(1), 0);
        assert_eq!(ceil_log2(2), 1);
        assert_eq!(ceil_log2(3), 2);
        assert_eq!(ceil_log2(4), 2);
        assert_eq!(ceil_log2(5), 3);
        assert_eq!(ceil_log2(1024), 10);
    }

    #[test]
    fn suspicion_timeout_grows_with_cluster_size() {
        let limits = Limits::default();
        let small = limits.effective_suspicion_timeout(1);
        let large = limits.effective_suspicion_timeout(1000);
        assert!(large > small, "{large} should exceed {small}");
        // base 5000ms * ceil(log2(2)) = 1 * multiplier 3
        assert_eq!(small, DurationMillis(15_000));
    }

    #[test]
    fn suspicion_timeout_saturates_instead_of_overflowing() {
        let limits = Limits::try_from(LimitsSpec {
            suspicion_timeout: DurationMillis(u64::MAX),
            suspicion_multiplier: u32::MAX,
            ..LimitsSpec::default()
        })
        .unwrap();
        assert_eq!(
            limits.effective_suspicion_timeout(usize::MAX),
            DurationMillis(u64::MAX)
        );
    }

    #[test]
    fn gossip_hops_default_to_the_automatic_value() {
        let limits = Limits::default();
        assert_eq!(limits.effective_gossip_hops(1), 1);
        assert_eq!(limits.effective_gossip_hops(16), 12);

        let fixed = Limits::try_from(LimitsSpec {
            max_gossip_hops: 5,
            ..LimitsSpec::default()
        })
        .unwrap();
        assert_eq!(fixed.effective_gossip_hops(1024), 5);
    }

    #[test]
    fn join_backoff_doubles_then_caps() {
        let limits = Limits::default();
        assert_eq!(limits.join_backoff_for(1), DurationMillis(1_000));
        assert_eq!(limits.join_backoff_for(2), DurationMillis(2_000));
        assert_eq!(limits.join_backoff_for(3), DurationMillis(4_000));
        assert_eq!(limits.join_backoff_for(64), DurationMillis(60_000));
    }
}
