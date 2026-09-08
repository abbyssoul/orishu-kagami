//! Declared bounds on everything a workload document can ask a reader to do.
//!
//! A workload manifest arrives from a submitting client or a peer, so it is
//! hostile input even when it came from a trusted operator. Every bound lives
//! here as one named value rather than as a literal at the point of use, so a
//! network-facing caller can tighten them and a test can drive a limit failure
//! without generating a megabyte of YAML.
//!
//! Several of these are load-bearing for *ordering*, not only for size:
//!
//! - [`Limits::max_manifest_bytes`] is checked before the document is parsed,
//!   so an oversized manifest is never allocated in order to be rejected.
//! - Every **collection bound** is applied *during* authoring deserialization,
//!   by [`crate::authoring`]'s bounded reader: a collection stops as soon as
//!   accepting its next entry would exceed the bound, and the rejected entry is
//!   never deserialized. A limit is therefore a bound on what a document can
//!   make a reader allocate, not only on what it may declare.
//! - The same collection bounds, and every string bound, are applied again to
//!   the whole model before [`crate::validate_closure`] asks a provider for a
//!   single byte. That second application is not redundant: a manifest may also
//!   be built programmatically — by Kagami's compiler, or from canonical
//!   bytes — and never pass through the authoring reader at all.
//! - [`Limits::max_aggregate_declared_bytes`] is decided from the manifest
//!   alone for the same reason, so a document cannot make a reader fetch more
//!   bytes than it agreed to hold by declaring many large artifacts.
//!
//! Every bound below is reachable from an authored document. Where one bound
//! governs several collections that is said explicitly on the field, because a
//! bound nobody can name is a bound nobody can raise.

/// Bounds applied while reading, canonicalising, and validating a workload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest authored manifest accepted, in bytes, checked before parsing.
    pub max_manifest_bytes: u64,
    /// Deepest nesting the canonical encoder will follow.
    ///
    /// The typed model is not recursive, so this cannot be exceeded by a valid
    /// manifest; it exists so a future recursive field cannot turn encoding
    /// into unbounded stack use without someone raising this number first.
    pub max_nesting_depth: usize,
    /// Largest number of component instances in one graph.
    ///
    /// Also bounds [`crate::PlacementConstraint::instances`]: a constraint
    /// names instances of the graph it constrains, so it can never usefully
    /// name more than the graph may contain. A separate number would only be a
    /// second way to say the same thing, and the two could then disagree.
    pub max_components: usize,
    /// Largest number of state and contribution channels in one graph.
    pub max_channels: usize,
    /// Largest rank of one channel payload's declared shape.
    ///
    /// The length of [`crate::StateChannel::shape`], which is a
    /// component-count per axis rather than a spatial extent. Deliberately
    /// *not* [`Self::max_domain_dimensions`]: a rank-2 tensor field in a
    /// 3-dimensional domain has shape `[3, 3]`, so the two counts are
    /// independent and sharing one number would couple them by accident.
    pub max_channel_shape_rank: usize,
    /// Largest number of invocation nodes in one step plan.
    pub max_step_invocations: usize,
    /// Largest number of input channels one invocation may consume.
    pub max_inputs_per_invocation: usize,
    /// Largest number of output channels one invocation may produce.
    pub max_outputs_per_invocation: usize,
    /// Largest number of predecessors one invocation may declare.
    pub max_dependencies_per_invocation: usize,
    /// Largest number of channels one component instance may own.
    pub max_state_ownership_per_component: usize,
    /// Largest number of declared roles on one component instance.
    pub max_roles_per_component: usize,
    /// Largest number of configuration entries on one component instance.
    pub max_config_entries: usize,
    /// Largest number of resource-limit entries on one component instance.
    pub max_limit_entries: usize,
    /// Largest number of entries in one *declared parameter map*.
    ///
    /// One bound, four collections, because they are one kind of thing: a
    /// bounded map of already-resolved [`crate::ScalarValue`]s naming settings
    /// a reader passes through rather than interprets. Those are
    /// [`crate::PlacementConstraint::parameters`],
    /// [`crate::Integration::parameters`],
    /// [`crate::WorkloadRequirements::hardware`], and
    /// [`crate::WorkloadRequirements::execution_profile`].
    ///
    /// Component configuration is deliberately excluded and keeps its own,
    /// larger [`Self::max_config_entries`]: a guest's configuration is authored
    /// against a plugin's schema and is expected to be the biggest of these
    /// maps, while these four are read by the worker itself.
    ///
    /// The error a document gets names the collection, not this field, so the
    /// four remain distinguishable to an author despite sharing a number.
    pub max_parameter_entries: usize,
    /// Largest number of artifact descriptors reachable from one manifest.
    ///
    /// Aggregate across component artifacts, geometry, initial conditions, and
    /// [`crate::WorkloadInputs::additional`], counted as descriptors are read
    /// rather than once at the end. `additional` has no bound of its own for
    /// that reason: it is whatever the aggregate has left, which is the honest
    /// answer — a per-collection number there would either be unreachable or
    /// would let one manifest hold more descriptors than this permits.
    pub max_artifacts: usize,
    /// Largest declared size of any single artifact, in bytes.
    pub max_artifact_bytes: u64,
    /// Largest total declared size across the whole closure, in bytes.
    pub max_aggregate_declared_bytes: u64,
    /// Largest number of initial-condition artifacts.
    pub max_initial_conditions: usize,
    /// Largest number of placement constraints.
    pub max_placement_constraints: usize,
    /// Largest number of metadata labels on one manifest.
    pub max_labels: usize,
    /// Largest accepted length for a free-text value, in bytes.
    ///
    /// Applies to every [`crate::ScalarValue::Text`] a manifest carries —
    /// component configuration, resource requirements, placement-constraint
    /// parameters, and integration parameters. Names are bounded separately by
    /// their own types, which check length before allocating.
    pub max_text_bytes: usize,
    /// Largest number of spatial dimensions a domain may declare.
    ///
    /// Also bounds the side lengths of a [`crate::DomainBounds::Box`], which
    /// must agree with the declared dimensionality exactly — that equality is
    /// checked by [`crate::DomainSpec::validate`]; this bound is what stops a
    /// document declaring a million of them in order to be told so.
    pub max_domain_dimensions: u8,
    /// Largest number of problems one validation reports.
    ///
    /// A report is a diagnostic for a person, and a bounded manifest can still
    /// be wrong in proportion to its size — one error per invocation, say. This
    /// caps what a caller must hold and render; the report records that it was
    /// truncated so a reader is never misled into thinking the list is
    /// exhaustive.
    pub max_reported_errors: usize,
    /// Largest number of values one canonical document may contain.
    ///
    /// Depth alone does not bound decoding: a flat array header can claim
    /// billions of elements. This caps the total decoded value count so a
    /// hostile document cannot make a reader allocate without limit. Encoding
    /// is bounded by the typed model it starts from and does not need it.
    pub max_canonical_values: usize,
}

impl Limits {
    /// Bounds sized for the workloads this design is meant to carry.
    ///
    /// The component counts are generous against ADR 0024's worked example — a
    /// field model, a coupling phase, Dynamics, and an emitter is four
    /// instances — while keeping the worst case one manifest can cost small
    /// enough to validate eagerly.
    ///
    /// The byte budgets assume large scientific inputs are normal: a single
    /// 64 GiB artifact and a 1 TiB closure are permitted. That is affordable
    /// only because verification streams — a candidate is hashed in chunks and
    /// never materialised by the validator, and a [`crate::VerifiedClosure`]
    /// holds descriptors rather than bytes. A validator that held what it
    /// verified could not honestly offer these numbers.
    pub const DEFAULT: Self = Self {
        max_manifest_bytes: 4 * 1024 * 1024,
        max_nesting_depth: 32,
        max_components: 64,
        max_channels: 256,
        max_channel_shape_rank: 8,
        max_step_invocations: 256,
        max_inputs_per_invocation: 32,
        max_outputs_per_invocation: 32,
        max_dependencies_per_invocation: 32,
        max_state_ownership_per_component: 32,
        max_roles_per_component: 16,
        max_config_entries: 128,
        max_limit_entries: 32,
        max_parameter_entries: 64,
        max_artifacts: 512,
        max_artifact_bytes: 64 * 1024 * 1024 * 1024,
        max_aggregate_declared_bytes: 1024 * 1024 * 1024 * 1024,
        max_initial_conditions: 64,
        max_placement_constraints: 64,
        max_labels: 32,
        max_text_bytes: 4096,
        max_domain_dimensions: 4,
        max_reported_errors: 256,
        max_canonical_values: 1 << 20,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_the_declared_constant() {
        assert_eq!(Limits::default(), Limits::DEFAULT);
    }

    #[test]
    fn every_default_limit_is_non_zero() {
        // A zero bound would reject every workload, including a valid one, and
        // the failure would look like a malformed manifest rather than a
        // misconfigured reader.
        let limits = Limits::DEFAULT;
        assert!(limits.max_manifest_bytes > 0);
        assert!(limits.max_nesting_depth > 0);
        assert!(limits.max_components > 0);
        assert!(limits.max_channels > 0);
        assert!(limits.max_channel_shape_rank > 0);
        assert!(limits.max_step_invocations > 0);
        assert!(limits.max_inputs_per_invocation > 0);
        assert!(limits.max_outputs_per_invocation > 0);
        assert!(limits.max_dependencies_per_invocation > 0);
        assert!(limits.max_state_ownership_per_component > 0);
        assert!(limits.max_roles_per_component > 0);
        assert!(limits.max_config_entries > 0);
        assert!(limits.max_limit_entries > 0);
        assert!(limits.max_parameter_entries > 0);
        assert!(limits.max_artifacts > 0);
        assert!(limits.max_artifact_bytes > 0);
        assert!(limits.max_aggregate_declared_bytes > 0);
        assert!(limits.max_initial_conditions > 0);
        assert!(limits.max_placement_constraints > 0);
        assert!(limits.max_labels > 0);
        assert!(limits.max_text_bytes > 0);
        assert!(limits.max_domain_dimensions > 0);
        assert!(limits.max_reported_errors > 0);
        assert!(limits.max_canonical_values > 0);
    }

    #[test]
    fn one_artifact_cannot_exceed_the_whole_closure_budget() {
        // Otherwise the per-artifact bound would be unreachable and the
        // aggregate check would be the only one that ever fired. A `const`
        // block, so the defaults are checked when the crate compiles rather
        // than when the suite runs.
        const {
            assert!(
                Limits::DEFAULT.max_artifact_bytes <= Limits::DEFAULT.max_aggregate_declared_bytes
            );
        }
    }
}
