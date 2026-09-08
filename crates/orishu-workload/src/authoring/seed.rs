//! The bounded authoring reader.
//!
//! Every type in the workload model has a derived [`serde::Deserialize`], and
//! for a manifest a caller built itself that is the right tool. It is the wrong
//! tool for a document someone else wrote, because a derive builds the whole
//! `Vec` or `BTreeMap` and only then hands it to something that might refuse
//! it: the bound describes the result rather than the work.
//!
//! This module is the other half. It is a [`DeserializeSeed`] layer that
//! carries the caller's [`Limits`] down the document, so a collection stops at
//! the point where accepting the next entry *would* exceed its bound — and the
//! entry that would have exceeded it is refused without ever being
//! deserialized. That ordering is the whole point; see
//! [`Refuse`], which is a seed whose only behaviour is to fail without touching
//! the deserializer it is handed.
//!
//! # Why a seed and not a wrapper type
//!
//! `Limits` is supplied by the caller at run time — a network-facing admission
//! path will use tighter numbers than a local file import. A newtype with a
//! `Deserialize` impl cannot see a runtime value, and the usual ways to give it
//! one (a thread-local, a global, a `OnceCell`) make the bound depend on
//! ambient state that no signature mentions. A seed carries it explicitly, so
//! two parses with different limits can run concurrently and mean it.
//!
//! # What this layer does not change
//!
//! It reads exactly the schema the derives describe: the same field spellings,
//! the same required/optional split, the same refusal of unknown fields, and
//! the same duplicate-field and missing-field errors. It produces the same
//! typed model, so it cannot affect canonical bytes or workload identity. What
//! it adds is *when* a bound is applied, not what is accepted.
//!
//! # What is still bounded only by the input length
//!
//! Two things, deliberately:
//!
//! - **The codec's own buffers.** `serde_yaml` parses a document into an event
//!   buffer before deserialization begins; that buffer is proportional to the
//!   input, which [`Limits::max_manifest_bytes`] has already capped.
//! - **[`ScalarValue::Text`](crate::ScalarValue).** It is a value, not a
//!   collection, and it is bounded by [`Limits::max_text_bytes`] in the
//!   structural pass. Bounding it here too would mean hand-writing the untagged
//!   scalar dispatch, which is a change to what parses rather than to when a
//!   bound applies.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::artifact::ArtifactDescriptor;
use crate::domain::{Discretization, DomainBounds, DomainSpec, Integration};
use crate::graph::{
    ComponentInstance, ComputeSpec, PlacementConstraint, StateChannel, StepInvocation, StepPlan,
};
use crate::ids::{
    ComponentInstanceId, ComponentRole, LabelKey, LabelValue, LimitName, ParameterName,
    StateChannelId, StepInvocationId,
};
use crate::limits::Limits;
use crate::manifest::{
    ApiVersion, Kind, NoStatus, WorkloadInputs, WorkloadManifest, WorkloadMeta,
    WorkloadRequirements, WorkloadSpec,
};
use crate::value::{FiniteF64, ScalarValue};

use super::CollectionLimit;

// ── the context threaded through every seed ─────────────────────────────────

/// The caller's bounds, plus the counting a single collection cannot do alone.
///
/// Shared by reference through the whole document. The two [`Cell`]s are the
/// only mutable state, and both exist because a per-collection visitor is the
/// wrong place to know either fact:
///
/// - artifact descriptors are counted across four separate collections, so the
///   aggregate bound needs a total nothing local holds; and
/// - the structured reason a document was refused has to survive being turned
///   into a `serde` error, which can carry only a message.
pub(super) struct Bounded<'a> {
    limits: &'a Limits,
    artifacts: Cell<usize>,
    violation: Cell<Option<CollectionLimit>>,
}

impl<'a> Bounded<'a> {
    /// A fresh context for one document.
    pub(super) fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            artifacts: Cell::new(0),
            violation: Cell::new(None),
        }
    }

    /// The bound a document exceeded, when that is why the parse failed.
    ///
    /// Recorded rather than parsed back out of the message: a caller deciding
    /// what to tell an author should not have to read English.
    pub(super) fn into_violation(self) -> Option<CollectionLimit> {
        self.violation.into_inner()
    }

    /// Records `collection` as over its bound and renders the serde error.
    ///
    /// Only the first violation is kept. A document that is over two bounds
    /// stops at the first one, so a later one would be an artefact of field
    /// order rather than the reason it was refused.
    fn exceeded<E: de::Error>(
        &self,
        collection: &'static str,
        limit: usize,
        found: Option<usize>,
    ) -> E {
        let violation = CollectionLimit {
            collection,
            limit,
            found,
        };
        let message = violation.to_string();
        match self.violation.take() {
            Some(first) => self.violation.set(Some(first)),
            None => self.violation.set(Some(violation)),
        }
        E::custom(message)
    }

    /// Accounts for one more artifact descriptor, before it is read.
    ///
    /// The aggregate bound is the only one that spans collections: component
    /// artifacts, geometry, initial conditions, and additional inputs all draw
    /// on it, in whatever order the document happens to spell them.
    fn admit_artifact<E: de::Error>(&self) -> Result<(), E> {
        let seen = self.artifacts.get();
        if seen >= self.limits.max_artifacts {
            return Err(self.exceeded("the artifact count", self.limits.max_artifacts, None));
        }
        self.artifacts.set(seen + 1);
        Ok(())
    }
}

// ── the reusable machinery ──────────────────────────────────────────────────

/// A seed that refuses whatever it is offered, without reading it.
///
/// This is what makes "the rejected entry is not deserialized" true rather than
/// merely eventual: `_deserializer` is dropped unread, so no element value and
/// no map key is ever built. A bounded collection reaches for this exactly once
/// — when it already holds its limit and the document has another entry.
struct Refuse<'ctx> {
    context: &'ctx Bounded<'ctx>,
    collection: &'static str,
    limit: usize,
}

impl<'de> DeserializeSeed<'de> for Refuse<'_> {
    /// Uninhabited: this seed never returns a value, which is what lets a
    /// caller discharge the `Some` arm by pattern matching rather than by
    /// asserting something unreachable.
    type Value = Infallible;

    fn deserialize<D: Deserializer<'de>>(self, _deserializer: D) -> Result<Infallible, D::Error> {
        Err(self
            .context
            .exceeded(self.collection, self.limit, Some(self.limit + 1)))
    }
}

/// A seed that simply defers to a type's own `Deserialize`.
///
/// Used for every leaf that holds no collection of its own — validated names,
/// digests, scalars, and the descriptors' inner types. Those already check what
/// they can before allocating, so there is nothing for this layer to add.
struct Plain<T>(PhantomData<fn() -> T>);

impl<T> Plain<T> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for Plain<T> {
    type Value = T;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        T::deserialize(deserializer)
    }
}

/// Lifts a seed over an optional value, so an explicit null decodes as `None`.
struct Opt<S>(S);

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Opt<S> {
    type Value = Option<S::Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_option(self)
    }
}

impl<'de, S: DeserializeSeed<'de>> Visitor<'de> for Opt<S> {
    type Value = Option<S::Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an optional value")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(deserializer).map(Some)
    }
}

/// A sequence that stops at its bound rather than after it.
///
/// `make` builds a fresh element seed per element, because a seed is consumed
/// by the element it reads.
struct BoundedSeq<'ctx, F> {
    context: &'ctx Bounded<'ctx>,
    collection: &'static str,
    limit: usize,
    make: F,
}

impl<'ctx, F> BoundedSeq<'ctx, F> {
    fn new(context: &'ctx Bounded<'ctx>, collection: &'static str, limit: usize, make: F) -> Self {
        Self {
            context,
            collection,
            limit,
            make,
        }
    }
}

/// A bounded sequence of values that deserialize themselves.
fn plain_seq<'ctx, T>(
    context: &'ctx Bounded<'ctx>,
    collection: &'static str,
    limit: usize,
) -> BoundedSeq<'ctx, impl FnMut() -> Plain<T>> {
    BoundedSeq::new(context, collection, limit, Plain::new)
}

impl<'de, S, F> DeserializeSeed<'de> for BoundedSeq<'_, F>
where
    F: FnMut() -> S,
    S: DeserializeSeed<'de>,
{
    type Value = Vec<S::Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, S, F> Visitor<'de> for BoundedSeq<'_, F>
where
    F: FnMut() -> S,
    S: DeserializeSeed<'de>,
{
    type Value = Vec<S::Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "at most {} {}", self.limit, self.collection)
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Self::Value, A::Error> {
        // A hint is a claim by the document, so it is checked before it is
        // acted on and never used to reserve more than the bound permits.
        // A format that offers no hint is not treated as offering zero: the
        // count below is what actually decides.
        let hint = sequence.size_hint();
        if let Some(hint) = hint
            && hint > self.limit
        {
            return Err(self
                .context
                .exceeded(self.collection, self.limit, Some(hint)));
        }
        let mut items = Vec::with_capacity(hint.unwrap_or(0).min(self.limit));

        loop {
            if items.len() == self.limit {
                // Full. Ask whether there is another entry with a seed that
                // refuses before reading, so the entry that would have gone
                // over the bound is never built.
                if let Some(never) = sequence.next_element_seed(Refuse {
                    context: self.context,
                    collection: self.collection,
                    limit: self.limit,
                })? {
                    match never {}
                }
                break;
            }
            match sequence.next_element_seed((self.make)())? {
                Some(item) => items.push(item),
                None => break,
            }
        }
        Ok(items)
    }
}

/// A map that stops at its bound rather than after it.
///
/// Both halves of the rejected entry are refused: the bound is checked before
/// the *key* is read, so neither key nor value is deserialized.
struct BoundedMap<'ctx, K, V> {
    context: &'ctx Bounded<'ctx>,
    collection: &'static str,
    limit: usize,
    marker: PhantomData<fn() -> (K, V)>,
}

impl<'ctx, K, V> BoundedMap<'ctx, K, V> {
    fn new(context: &'ctx Bounded<'ctx>, collection: &'static str, limit: usize) -> Self {
        Self {
            context,
            collection,
            limit,
            marker: PhantomData,
        }
    }
}

impl<'de, K, V> DeserializeSeed<'de> for BoundedMap<'_, K, V>
where
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    type Value = BTreeMap<K, V>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de, K, V> Visitor<'de> for BoundedMap<'_, K, V>
where
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    type Value = BTreeMap<K, V>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "at most {} {}", self.limit, self.collection)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        if let Some(hint) = map.size_hint()
            && hint > self.limit
        {
            return Err(self
                .context
                .exceeded(self.collection, self.limit, Some(hint)));
        }
        let mut entries = BTreeMap::new();

        // Entries read, not entries kept. Two spellings of one key collapse in
        // the map, and counting the result would let a document buy itself
        // extra entries by repeating one.
        let mut read = 0usize;
        loop {
            if read == self.limit {
                if let Some(never) = map.next_key_seed(Refuse {
                    context: self.context,
                    collection: self.collection,
                    limit: self.limit,
                })? {
                    match never {}
                }
                break;
            }
            let Some(key) = map.next_key::<K>()? else {
                break;
            };
            read += 1;
            entries.insert(key, map.next_value::<V>()?);
        }
        Ok(entries)
    }
}

/// Counts one artifact descriptor against the aggregate bound, then reads it.
///
/// A descriptor holds no collection of its own, so the count is the only thing
/// this layer adds — and it is added *before* the descriptor is built.
struct Descriptor<'ctx>(&'ctx Bounded<'ctx>);

impl<'de> DeserializeSeed<'de> for Descriptor<'_> {
    type Value = ArtifactDescriptor;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<ArtifactDescriptor, D::Error> {
        self.0.admit_artifact()?;
        ArtifactDescriptor::deserialize(deserializer)
    }
}

// ── the struct reader ───────────────────────────────────────────────────────

/// Resolves one field after the map has been read.
macro_rules! bounded_field {
    (required, $value:ident, $key:literal) => {
        match $value {
            Some(value) => value,
            None => return Err(de::Error::missing_field($key)),
        }
    };
    (optional, $value:ident, $key:literal) => {
        $value.unwrap_or_default()
    };
}

/// Declares a [`DeserializeSeed`] that reads one struct of the model.
///
/// Written as a macro rather than fifteen hand-copied visitors because the
/// interesting part of each is one line per field — which seed reads it — and
/// the rest is the unknown-field, duplicate-field and missing-field handling
/// that must be identical everywhere. Copying that by hand fifteen times is how
/// one struct ends up quietly accepting a key the others refuse.
///
/// `$context` is named by the caller so the per-field seed expressions can use
/// it; a macro-introduced binding would not be visible to them.
macro_rules! bounded_struct {
    (
        $(#[$meta:meta])*
        $seed:ident($context:ident) -> $target:ident as $expecting:literal {
            $( $key:literal => $field:ident : $mode:ident = $make:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        struct $seed<'ctx>(&'ctx Bounded<'ctx>);

        impl<'de> DeserializeSeed<'de> for $seed<'_> {
            type Value = $target;

            fn deserialize<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<$target, D::Error> {
                deserializer.deserialize_struct(stringify!($target), &[$($key),*], self)
            }
        }

        impl<'de> Visitor<'de> for $seed<'_> {
            type Value = $target;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str($expecting)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<$target, A::Error> {
                const FIELDS: &[&str] = &[$($key),*];

                // Variants named for the Rust fields, so the match below cannot
                // pair a key with the wrong slot.
                #[allow(non_camel_case_types)]
                enum Field { $($field),* }

                /// Reads a key as a field, refusing an unrecognised one while
                /// its name is still borrowed.
                struct Key;

                impl<'de> DeserializeSeed<'de> for Key {
                    type Value = Field;

                    fn deserialize<D: Deserializer<'de>>(
                        self,
                        deserializer: D,
                    ) -> Result<Field, D::Error> {
                        deserializer.deserialize_identifier(self)
                    }
                }

                impl<'de> Visitor<'de> for Key {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str(concat!("a field of ", $expecting))
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
                        match value {
                            $($key => Ok(Field::$field),)*
                            other => Err(de::Error::unknown_field(other, FIELDS)),
                        }
                    }
                }

                let $context = self.0;
                $(let mut $field = None;)*

                while let Some(field) = map.next_key_seed(Key)? {
                    match field {
                        $(Field::$field => {
                            if $field.is_some() {
                                return Err(de::Error::duplicate_field($key));
                            }
                            $field = Some(map.next_value_seed($make)?);
                        })*
                    }
                }

                Ok($target {
                    $($field: bounded_field!($mode, $field, $key),)*
                })
            }
        }
    };
}

// ── the model, top down ─────────────────────────────────────────────────────

bounded_struct! {
    /// Identity-bearing metadata.
    Meta(context) -> WorkloadMeta as "workload metadata" {
        "name" => name: required = Plain::new(),
        "labels" => labels: optional = BoundedMap::<LabelKey, LabelValue>::new(
            context, "the label count", context.limits.max_labels),
    }
}

bounded_struct! {
    /// One configured, sandboxed use of a component artifact.
    Component(context) -> ComponentInstance as "a component instance" {
        "instanceId" => instance_id: required = Plain::new(),
        "artifact" => artifact: required = Descriptor(context),
        "pluginId" => plugin_id: required = Plain::new(),
        "modelId" => model_id: required = Plain::new(),
        "schemaId" => schema_id: required = Plain::new(),
        "engine" => engine: required = Plain::new(),
        "lifecycle" => lifecycle: required = Plain::new(),
        "roles" => roles: optional = plain_seq::<ComponentRole>(
            context, "a component's role count", context.limits.max_roles_per_component),
        "stateOwnership" => state_ownership: optional = plain_seq::<StateChannelId>(
            context,
            "a component's owned-channel count",
            context.limits.max_state_ownership_per_component,
        ),
        "config" => config: optional = BoundedMap::<ParameterName, ScalarValue>::new(
            context, "a component's configuration size", context.limits.max_config_entries),
        "limits" => limits: optional = BoundedMap::<LimitName, u64>::new(
            context, "a component's limit count", context.limits.max_limit_entries),
    }
}

bounded_struct! {
    /// One typed channel carrying state or contributions between instances.
    Channel(context) -> StateChannel as "a state channel" {
        "channelId" => channel_id: required = Plain::new(),
        "schema" => schema: required = Plain::new(),
        "shape" => shape: optional = plain_seq::<u32>(
            context, "a channel's shape rank", context.limits.max_channel_shape_rank),
        "owner" => owner: optional = Plain::<Option<ComponentInstanceId>>::new(),
        "reduction" => reduction: required = Plain::new(),
    }
}

bounded_struct! {
    /// One node of the step plan.
    Invocation(context) -> StepInvocation as "a step invocation" {
        "invocationId" => invocation_id: required = Plain::new(),
        "instance" => instance: required = Plain::new(),
        "phaseId" => phase_id: required = Plain::new(),
        "inputs" => inputs: optional = plain_seq::<StateChannelId>(
            context, "an invocation's input count", context.limits.max_inputs_per_invocation),
        "outputs" => outputs: optional = plain_seq::<StateChannelId>(
            context, "an invocation's output count", context.limits.max_outputs_per_invocation),
        "dependsOn" => depends_on: optional = plain_seq::<StepInvocationId>(
            context,
            "an invocation's dependency count",
            context.limits.max_dependencies_per_invocation,
        ),
    }
}

bounded_struct! {
    /// The deterministic schedule for advancing one committed boundary.
    Plan(context) -> StepPlan as "a step plan" {
        "profile" => profile: required = Plain::new(),
        "invocations" => invocations: required = BoundedSeq::new(
            context,
            "the step-invocation count",
            context.limits.max_step_invocations,
            || Invocation(context),
        ),
    }
}

bounded_struct! {
    /// A declared scientific placement constraint.
    Constraint(context) -> PlacementConstraint as "a placement constraint" {
        "constraint" => constraint: required = Plain::new(),
        // A constraint names instances of the graph it constrains, so the
        // graph's own bound is the one that applies.
        "instances" => instances: optional = plain_seq::<ComponentInstanceId>(
            context, "a placement constraint's instance count", context.limits.max_components),
        "parameters" => parameters: optional = BoundedMap::<ParameterName, ScalarValue>::new(
            context,
            "a placement constraint's parameter count",
            context.limits.max_parameter_entries,
        ),
    }
}

bounded_struct! {
    /// What runs, how it is wired, and in what order.
    Compute(context) -> ComputeSpec as "a compute definition" {
        "workloadGraphProfile" => workload_graph_profile: required = Plain::new(),
        "components" => components: required = BoundedSeq::new(
            context,
            "the component count",
            context.limits.max_components,
            || Component(context),
        ),
        "channels" => channels: optional = BoundedSeq::new(
            context,
            "the channel count",
            context.limits.max_channels,
            || Channel(context),
        ),
        "stepPlan" => step_plan: required = Plan(context),
        "placementConstraints" => placement_constraints: optional = BoundedSeq::new(
            context,
            "the placement-constraint count",
            context.limits.max_placement_constraints,
            || Constraint(context),
        ),
    }
}

bounded_struct! {
    /// The selected temporal integration scheme and its configuration.
    IntegrationSeed(context) -> Integration as "an integration scheme" {
        "scheme" => scheme: required = Plain::new(),
        "parameters" => parameters: optional = BoundedMap::<ParameterName, ScalarValue>::new(
            context, "the integration parameter count", context.limits.max_parameter_entries),
    }
}

bounded_struct! {
    /// How space and time are sampled.
    DiscretizationSeed(context) -> Discretization as "a discretization" {
        "spaceMetres" => space_metres: required = Plain::new(),
        "timeSeconds" => time_seconds: required = Plain::new(),
        "integration" => integration: optional = Opt(IntegrationSeed(context)),
    }
}

bounded_struct! {
    /// The physical region a workload evolves.
    Domain(context) -> DomainSpec as "a domain" {
        "dimensions" => dimensions: required = Plain::new(),
        "bounds" => bounds: required = Bounds(context),
        "discretization" => discretization: required = DiscretizationSeed(context),
    }
}

bounded_struct! {
    /// The artifacts a workload consumes that are not executable code.
    Inputs(context) -> WorkloadInputs as "workload inputs" {
        "geometry" => geometry: optional = Opt(Descriptor(context)),
        "initialConditions" => initial_conditions: optional = BoundedSeq::new(
            context,
            "the initial-condition count",
            context.limits.max_initial_conditions,
            || Descriptor(context),
        ),
        // No bound of its own: `Descriptor` charges every descriptor in the
        // document to the aggregate artifact count, and that is what is left.
        "additional" => additional: optional = BoundedSeq::new(
            context,
            "the additional-artifact count",
            context.limits.max_artifacts,
            || Descriptor(context),
        ),
    }
}

bounded_struct! {
    /// What a worker must satisfy to run this workload.
    Requirements(context) -> WorkloadRequirements as "workload requirements" {
        "hardware" => hardware: optional = BoundedMap::<ParameterName, ScalarValue>::new(
            context, "the hardware-requirement count", context.limits.max_parameter_entries),
        "executionProfile" => execution_profile: optional
            = BoundedMap::<ParameterName, ScalarValue>::new(
                context, "the execution-profile count", context.limits.max_parameter_entries),
    }
}

bounded_struct! {
    /// Everything a workload declares.
    Spec(context) -> WorkloadSpec as "a workload specification" {
        "compute" => compute: required = Compute(context),
        "domain" => domain: required = Domain(context),
        "inputs" => inputs: optional = Inputs(context),
        "requirements" => requirements: optional = Requirements(context),
    }
}

// ── the two hand-written readers ────────────────────────────────────────────

/// Reads the extent of the simulated region.
///
/// Hand-written because [`DomainBounds`] is tagged by a `shape` field, and the
/// derive resolves an out-of-order document — `sideMetres` before `shape` — by
/// buffering the whole map until the tag turns up. Reading `sideMetres` as
/// "either one length or a bounded list of them" and reconciling it with the
/// shape afterwards accepts exactly the same documents while buffering nothing
/// and bounding the list as it is read.
struct Bounds<'ctx>(&'ctx Bounded<'ctx>);

/// One side length, or one per dimension, before the shape is known.
enum SideMetres {
    One(FiniteF64),
    Each(Vec<FiniteF64>),
}

/// Reads a `sideMetres` value without yet knowing which shape wants it.
struct SideMetresSeed<'ctx>(&'ctx Bounded<'ctx>);

impl<'de> DeserializeSeed<'de> for SideMetresSeed<'_> {
    type Value = SideMetres;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<SideMetres, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SideMetresSeed<'_> {
    type Value = SideMetres;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a side length in metres, or one per dimension")
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<SideMetres, E> {
        FiniteF64::new(value)
            .map(SideMetres::One)
            .map_err(de::Error::custom)
    }

    // An authored `1` is an integer to both codecs; the model has one numeric
    // type, so it becomes the same length either way.
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<SideMetres, E> {
        self.visit_f64(value as f64)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<SideMetres, E> {
        self.visit_f64(value as f64)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<SideMetres, A::Error> {
        let limit = usize::from(self.0.limits.max_domain_dimensions);
        plain_seq::<FiniteF64>(self.0, "the domain side-length count", limit)
            .visit_seq(sequence)
            .map(SideMetres::Each)
    }
}

impl<'de> DeserializeSeed<'de> for Bounds<'_> {
    type Value = DomainBounds;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<DomainBounds, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Bounds<'_> {
    type Value = DomainBounds;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("domain bounds")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<DomainBounds, A::Error> {
        let mut shape: Option<Shape> = None;
        let mut side_metres: Option<SideMetres> = None;

        while let Some(key) = map.next_key::<BoundsField>()? {
            match key {
                BoundsField::Shape => {
                    if shape.is_some() {
                        return Err(de::Error::duplicate_field("shape"));
                    }
                    shape = Some(map.next_value()?);
                }
                BoundsField::SideMetres => {
                    if side_metres.is_some() {
                        return Err(de::Error::duplicate_field("sideMetres"));
                    }
                    side_metres = Some(map.next_value_seed(SideMetresSeed(self.0))?);
                }
            }
        }

        let shape = shape.ok_or_else(|| de::Error::missing_field("shape"))?;
        let side_metres = side_metres.ok_or_else(|| de::Error::missing_field("sideMetres"))?;

        match (shape, side_metres) {
            (Shape::Cube, SideMetres::One(side_metres)) => Ok(DomainBounds::Cube { side_metres }),
            (Shape::Box, SideMetres::Each(side_metres)) => Ok(DomainBounds::Box { side_metres }),
            (Shape::Cube, SideMetres::Each(_)) => {
                Err(de::Error::custom("a cube has one `sideMetres`, not a list"))
            }
            (Shape::Box, SideMetres::One(_)) => Err(de::Error::custom(
                "a box has one `sideMetres` per dimension, as a list",
            )),
        }
    }
}

/// Which shape a domain's bounds declare.
enum Shape {
    Cube,
    Box,
}

impl<'de> Deserialize<'de> for Shape {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Tag;

        impl<'de> Visitor<'de> for Tag {
            type Value = Shape;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a domain shape")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Shape, E> {
                match value {
                    "cube" => Ok(Shape::Cube),
                    "box" => Ok(Shape::Box),
                    other => Err(de::Error::unknown_variant(other, &["cube", "box"])),
                }
            }
        }

        deserializer.deserialize_str(Tag)
    }
}

/// One recognised key of [`DomainBounds`].
enum BoundsField {
    Shape,
    SideMetres,
}

impl<'de> Deserialize<'de> for BoundsField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Key;

        impl<'de> Visitor<'de> for Key {
            type Value = BoundsField;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a field of domain bounds")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<BoundsField, E> {
                match value {
                    "shape" => Ok(BoundsField::Shape),
                    "sideMetres" => Ok(BoundsField::SideMetres),
                    other => Err(de::Error::unknown_field(other, &["shape", "sideMetres"])),
                }
            }
        }

        deserializer.deserialize_identifier(Key)
    }
}

/// Reads the resource envelope around a workload.
///
/// Hand-written rather than macro-generated because the envelope belongs to
/// `orishu-resource`, whose discriminator fields are private: the manifest is
/// rebuilt through the constructor, which is also what keeps a workload from
/// existing with a discriminator nothing checked. The `status` handling
/// reproduces `DenyUnknown` exactly — a value is refused because [`NoStatus`]
/// has no inhabitant, and an explicit null is refused because it is a
/// top-level field carrying nothing.
pub(super) struct Manifest<'ctx>(pub(super) &'ctx Bounded<'ctx>);

impl<'de> DeserializeSeed<'de> for Manifest<'_> {
    type Value = WorkloadManifest;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<WorkloadManifest, D::Error> {
        deserializer.deserialize_struct("Resource", ENVELOPE_FIELDS, self)
    }
}

const ENVELOPE_FIELDS: &[&str] = &["apiVersion", "kind", "metadata", "spec", "status"];

/// One recognised key of the envelope.
enum EnvelopeField {
    ApiVersion,
    Kind,
    Metadata,
    Spec,
    Status,
}

impl<'de> Deserialize<'de> for EnvelopeField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Key;

        impl<'de> Visitor<'de> for Key {
            type Value = EnvelopeField;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a resource envelope field")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<EnvelopeField, E> {
                match value {
                    "apiVersion" => Ok(EnvelopeField::ApiVersion),
                    "kind" => Ok(EnvelopeField::Kind),
                    "metadata" => Ok(EnvelopeField::Metadata),
                    "spec" => Ok(EnvelopeField::Spec),
                    "status" => Ok(EnvelopeField::Status),
                    other => Err(de::Error::unknown_field(other, ENVELOPE_FIELDS)),
                }
            }
        }

        deserializer.deserialize_identifier(Key)
    }
}

impl<'de> Visitor<'de> for Manifest<'_> {
    type Value = WorkloadManifest;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a workload with apiVersion, kind, metadata and spec")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<WorkloadManifest, A::Error> {
        let context = self.0;
        let mut api_version: Option<ApiVersion> = None;
        let mut kind: Option<Kind> = None;
        let mut metadata: Option<WorkloadMeta> = None;
        let mut spec: Option<WorkloadSpec> = None;

        while let Some(field) = map.next_key::<EnvelopeField>()? {
            match field {
                EnvelopeField::ApiVersion => {
                    if api_version.is_some() {
                        return Err(de::Error::duplicate_field("apiVersion"));
                    }
                    api_version = Some(map.next_value()?);
                }
                EnvelopeField::Kind => {
                    if kind.is_some() {
                        return Err(de::Error::duplicate_field("kind"));
                    }
                    kind = Some(map.next_value()?);
                }
                EnvelopeField::Metadata => {
                    if metadata.is_some() {
                        return Err(de::Error::duplicate_field("metadata"));
                    }
                    metadata = Some(map.next_value_seed(Meta(context))?);
                }
                EnvelopeField::Spec => {
                    if spec.is_some() {
                        return Err(de::Error::duplicate_field("spec"));
                    }
                    spec = Some(map.next_value_seed(Spec(context))?);
                }
                EnvelopeField::Status => {
                    // A value reaches `NoStatus`, which has no inhabitant to
                    // decode into; a null never reaches it, so it is refused
                    // here as the valueless top-level field it is. Both
                    // spellings fail, so there is no second `status` to detect
                    // as a duplicate.
                    match map.next_value::<Option<NoStatus>>()? {
                        Some(never) => match never {},
                        None => return Err(de::Error::custom("`status` must not be null")),
                    }
                }
            }
        }

        Ok(crate::manifest::WorkloadManifest::new(
            api_version.ok_or_else(|| de::Error::missing_field("apiVersion"))?,
            kind.ok_or_else(|| de::Error::missing_field("kind"))?,
            metadata.ok_or_else(|| de::Error::missing_field("metadata"))?,
            spec.ok_or_else(|| de::Error::missing_field("spec"))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    //! The reusable machinery, driven directly.
    //!
    //! These do not parse a document. They hand the bounded sequence and map
    //! visitors an access whose entries *cannot* be deserialized — the
    //! deserializer panics if anything reads it — so a passing test is evidence
    //! about ordering rather than about the eventual error. An implementation
    //! that read the entry and then measured the result would abort here.

    use super::*;
    use serde::de::value::{
        Error as ValueError, StrDeserializer, U32Deserializer, U64Deserializer,
    };
    use serde::forward_to_deserialize_any;

    /// A deserializer that must never be read.
    struct Exploding;

    impl<'de> Deserializer<'de> for Exploding {
        type Error = ValueError;

        fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, ValueError> {
            panic!("the entry past the bound was deserialized");
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    /// A sequence of `values`, followed by `beyond` entries that exist but
    /// cannot be read.
    struct Seq {
        hint: Option<usize>,
        values: std::vec::IntoIter<u32>,
        beyond: usize,
    }

    impl Seq {
        fn new(hint: Option<usize>, values: Vec<u32>, beyond: usize) -> Self {
            Self {
                hint,
                values: values.into_iter(),
                beyond,
            }
        }
    }

    impl<'de> SeqAccess<'de> for Seq {
        type Error = ValueError;

        fn next_element_seed<T: DeserializeSeed<'de>>(
            &mut self,
            seed: T,
        ) -> Result<Option<T::Value>, ValueError> {
            if let Some(value) = self.values.next() {
                return seed.deserialize(U32Deserializer::new(value)).map(Some);
            }
            if self.beyond == 0 {
                return Ok(None);
            }
            self.beyond -= 1;
            seed.deserialize(Exploding).map(Some)
        }

        fn size_hint(&self) -> Option<usize> {
            self.hint
        }
    }

    /// A map of `entries`, followed by `beyond` entries that cannot be read.
    struct Map {
        hint: Option<usize>,
        entries: std::vec::IntoIter<(&'static str, u64)>,
        pending: Option<u64>,
        beyond: usize,
    }

    impl Map {
        fn new(hint: Option<usize>, entries: Vec<(&'static str, u64)>, beyond: usize) -> Self {
            Self {
                hint,
                entries: entries.into_iter(),
                pending: None,
                beyond,
            }
        }
    }

    impl<'de> MapAccess<'de> for Map {
        type Error = ValueError;

        fn next_key_seed<K: DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, ValueError> {
            if let Some((key, value)) = self.entries.next() {
                self.pending = Some(value);
                let key: StrDeserializer<'de, ValueError> = key.into_deserializer();
                return seed.deserialize(key).map(Some);
            }
            if self.beyond == 0 {
                return Ok(None);
            }
            self.beyond -= 1;
            seed.deserialize(Exploding).map(Some)
        }

        fn next_value_seed<V: DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, ValueError> {
            let value = self.pending.take().expect("a value follows its key");
            seed.deserialize(U64Deserializer::new(value))
        }

        fn size_hint(&self) -> Option<usize> {
            self.hint
        }
    }

    use serde::de::IntoDeserializer;

    fn context(limits: &Limits) -> Bounded<'_> {
        Bounded::new(limits)
    }

    #[test]
    fn a_sequence_accepts_exactly_its_limit() {
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let accepted = plain_seq::<u32>(&context, "the test count", 3)
            .visit_seq(Seq::new(None, vec![7, 8, 9], 0))
            .expect("three entries fit a limit of three");
        assert_eq!(accepted, [7, 8, 9]);
    }

    #[test]
    fn a_sequence_refuses_the_entry_past_its_limit_without_reading_it() {
        // `Exploding` panics if the fourth entry is deserialized, so this fails
        // loudly rather than quietly if the bound ever moves after the read.
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let error = plain_seq::<u32>(&context, "the test count", 3)
            .visit_seq(Seq::new(None, vec![7, 8, 9], 1))
            .expect_err("a fourth entry is over the limit");
        assert!(error.to_string().contains("the test count"), "{error}");
        assert_eq!(
            context.into_violation(),
            Some(CollectionLimit {
                collection: "the test count",
                limit: 3,
                found: Some(4),
            })
        );
    }

    #[test]
    fn a_sequence_refuses_an_oversized_hint_before_reserving_for_it() {
        // A reader that trusted this would try to reserve the whole address
        // space; the test passing at all is the assertion.
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let error = plain_seq::<u32>(&context, "the test count", 4)
            .visit_seq(Seq::new(Some(usize::MAX), Vec::new(), 0))
            .expect_err("a hint over the limit is refused");
        assert!(error.to_string().contains("the test count"), "{error}");
        assert_eq!(
            context
                .into_violation()
                .expect("a recorded violation")
                .found,
            Some(usize::MAX),
            "the hint the document supplied is what is reported"
        );
    }

    #[test]
    fn a_sequence_never_reserves_more_than_its_limit() {
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let accepted = plain_seq::<u32>(&context, "the test count", 4)
            .visit_seq(Seq::new(Some(4), vec![1, 2], 0))
            .expect("two entries fit");
        assert!(
            accepted.capacity() <= 4,
            "reserved {} for a bound of 4",
            accepted.capacity()
        );
    }

    #[test]
    fn a_map_accepts_exactly_its_limit() {
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let accepted = BoundedMap::<ParameterName, u64>::new(&context, "the test count", 2)
            .visit_map(Map::new(None, vec![("a", 1), ("b", 2)], 0))
            .expect("two entries fit a limit of two");
        assert_eq!(accepted.len(), 2);
    }

    #[test]
    fn a_map_refuses_the_entry_past_its_limit_without_reading_its_key_or_value() {
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let error = BoundedMap::<ParameterName, u64>::new(&context, "the test count", 2)
            .visit_map(Map::new(None, vec![("a", 1), ("b", 2)], 1))
            .expect_err("a third entry is over the limit");
        assert!(error.to_string().contains("the test count"), "{error}");
        assert_eq!(
            context.into_violation(),
            Some(CollectionLimit {
                collection: "the test count",
                limit: 2,
                found: Some(3),
            })
        );
    }

    #[test]
    fn a_map_refuses_an_oversized_hint_before_reserving_for_it() {
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        BoundedMap::<ParameterName, u64>::new(&context, "the test count", 2)
            .visit_map(Map::new(Some(usize::MAX), Vec::new(), 0))
            .expect_err("a hint over the limit is refused");
        assert_eq!(
            context
                .into_violation()
                .expect("a recorded violation")
                .found,
            Some(usize::MAX)
        );
    }

    #[test]
    fn a_map_counts_entries_read_rather_than_entries_kept() {
        // Otherwise repeating one key would buy a document extra entries: the
        // duplicate collapses in the map, so the length would stop growing.
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let error = BoundedMap::<ParameterName, u64>::new(&context, "the test count", 2)
            .visit_map(Map::new(None, vec![("a", 1), ("a", 2)], 1))
            .expect_err("two reads exhaust a limit of two, however they collapse");
        assert!(error.to_string().contains("the test count"), "{error}");
    }

    #[test]
    fn only_the_first_violation_is_recorded() {
        // A document over two bounds stops at the first; a later one would be
        // an artefact of field order rather than the reason it was refused.
        let limits = Limits::DEFAULT;
        let context = context(&limits);
        let _: ValueError = context.exceeded("the first count", 1, None);
        let _: ValueError = context.exceeded("the second count", 2, None);
        assert_eq!(
            context.into_violation().expect("a violation").collection,
            "the first count"
        );
    }
}
