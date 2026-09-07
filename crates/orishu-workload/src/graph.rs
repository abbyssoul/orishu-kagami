//! The bounded component-instance graph and its deterministic step plan.
//!
//! ADR 0024 decided that a workload has one immutable root *manifest*, not
//! necessarily one root *executable*. What this module describes is the shape
//! of that composition: which sandboxed instances exist, what typed channels
//! carry state and contributions between them, and in what order Orishu invokes
//! them.
//!
//! These types are structural. They say a channel exists, who owns it, and
//! which invocations read and write it; they do not say whether the physics is
//! correct. Whether a reduction is scientifically meaningful, whether two
//! channels agree on physical dimensions, and whether a placement is feasible
//! on today's cluster are decided elsewhere — see [`crate::closure`] for what
//! is checked now and what is deferred.
//!
//! Field spellings follow `docs/protocol-client.md`, which committed to
//! `instanceId`, `pluginId`, `modelId`, `schemaId`, `engine`, `lifecycle`,
//! `roles`, `stateOwnership` and `limits` before this model existed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::artifact::{ArtifactDescriptor, SchemaCompat};
use crate::ids::{
    ComponentInstanceId, ComponentRole, ConstraintName, Engine, GraphProfile, LifecycleId,
    LimitName, ModelId, ParameterName, PhaseId, PluginId, StateChannelId, StepInvocationId,
};
use crate::value::ScalarValue;

/// One configured, sandboxed use of a component artifact.
///
/// The artifact is what runs; everything else here is what it runs *as*. Two
/// instances may name the same artifact digest with different configuration and
/// different owned state, and they are two instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentInstance {
    /// Names this instance within the graph.
    pub instance_id: ComponentInstanceId,
    /// The digest-pinned executable. Must carry
    /// [`ArtifactRole::COMPONENT`](crate::ArtifactRole::COMPONENT).
    pub artifact: ArtifactDescriptor,
    /// Which plugin this component was authored from.
    ///
    /// Provenance, not resolution: a worker never looks this up. See
    /// [`PluginId`].
    pub plugin_id: PluginId,
    /// Which computational model it implements.
    pub model_id: ModelId,
    /// Which state schema it speaks.
    pub schema_id: crate::ids::SchemaId,
    /// The execution engine its artifact targets.
    pub engine: Engine,
    /// The component lifecycle contract it implements.
    pub lifecycle: LifecycleId,
    /// What it does in the graph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<ComponentRole>,
    /// The channels whose authoritative state this instance owns.
    ///
    /// Ownership is scientific even though the runtime owns storage and
    /// transfer: a field-model instance owns its field, and Dynamics alone
    /// writes candidate particle kinematics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_ownership: Vec<StateChannelId>,
    /// Frozen configuration for this instance.
    ///
    /// A resolved map, not an expression graph. Retaining authored expression
    /// source in the manifest is specified by `docs/protocol-client.md` and
    /// arrives with the variables integration; until then a manifest carries
    /// only already-resolved scalars, which is what a guest consumes in either
    /// design.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<ParameterName, ScalarValue>,
    /// Declared per-instance resource limits.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: BTreeMap<LimitName, u64>,
}

/// How several producers' contributions to one channel are combined.
///
/// Named in the manifest rather than inferred, because ADR 0024 requires that
/// plugin installation order, manifest map order, worker timing, and guest
/// completion order never decide the scientific reduction order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reduction {
    /// Exactly one producer writes this channel; combining is not defined.
    Single,
    /// Contributions are summed. Order-independent by declaration; the runtime
    /// is responsible for making the summation deterministic.
    Sum,
    /// The smallest contribution wins.
    Min,
    /// The largest contribution wins.
    Max,
}

/// One typed channel carrying state or contributions between instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateChannel {
    /// Names this channel within the graph.
    pub channel_id: StateChannelId,
    /// The format its payload conforms to.
    pub schema: SchemaCompat,
    /// The payload's structural shape, as component-count per axis.
    ///
    /// Structural only. Physical dimension compatibility — that a force channel
    /// carries forces — is a later slice; this exists so a shape mismatch
    /// between a producer and a consumer is representable now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<u32>,
    /// The instance holding authoritative ownership, for a state channel.
    ///
    /// `None` for a contribution channel, which no instance owns and several
    /// may write under [`StateChannel::reduction`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<ComponentInstanceId>,
    /// How multiple contributions combine.
    pub reduction: Reduction,
}

/// One node of the step plan: a single component invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepInvocation {
    /// Names this node within the plan.
    pub invocation_id: StepInvocationId,
    /// The instance to invoke.
    pub instance: ComponentInstanceId,
    /// The admitted phase export to call. Never an arbitrary guest function.
    ///
    /// Spelled `phaseId` rather than `phase` because `phase` is the runtime
    /// workload status in `docs/protocol-client.md`, and the two must not be
    /// confused: this one is identity-bearing, that one cannot reach identity
    /// at all.
    pub phase_id: PhaseId,
    /// Channels this invocation reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<StateChannelId>,
    /// Channels this invocation writes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<StateChannelId>,
    /// Nodes that must complete before this one may run.
    ///
    /// The plan's edges. Declared rather than derived from channel use, because
    /// a method may need an ordering its data flow does not imply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<StepInvocationId>,
}

/// The deterministic schedule for advancing one committed boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepPlan {
    /// Which graph/plan semantics this plan was written against.
    pub profile: GraphProfile,
    /// The plan's nodes.
    ///
    /// Order in this list is *not* execution order — the dependency edges are.
    /// Keeping it a list rather than a map preserves the authored sequence for
    /// human reading while the canonical encoding commits to it, so a reordered
    /// document is a different workload and cannot silently be the same one.
    pub invocations: Vec<StepInvocation>,
}

/// A declared scientific placement constraint.
///
/// Says what is legal, never where anything currently runs. ADR 0024 makes
/// placement runtime state: co-locating every instance on one worker and
/// distributing them across eligible nodes are the same workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlacementConstraint {
    /// What the constraint is about, for example `co-locate` or
    /// `requires-accelerator`.
    pub constraint: ConstraintName,
    /// The instances it applies to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<ComponentInstanceId>,
    /// Constraint-specific values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<ParameterName, ScalarValue>,
}

/// The identity-bearing compute definition: what runs, how it is wired, and in
/// what order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputeSpec {
    /// Which component-graph profile this definition was written against.
    pub workload_graph_profile: GraphProfile,
    /// The component instances.
    pub components: Vec<ComponentInstance>,
    /// The typed channels between them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<StateChannel>,
    /// The deterministic schedule.
    pub step_plan: StepPlan,
    /// Scientific placement constraints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placement_constraints: Vec<PlacementConstraint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::ArtifactRole;
    use crate::digest::ArtifactDigest;
    use crate::ids::{MediaType, SchemaId};

    #[test]
    fn a_component_instance_refuses_an_unrecognised_field() {
        // An identity-bearing document must not silently drop a key: the
        // author would believe it did something while the digest says
        // otherwise.
        let json = r#"{
            "instanceId": "field",
            "artifact": {
                "role": "component",
                "digest": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "sizeBytes": 0,
                "mediaType": "application/wasm"
            },
            "pluginId": "p", "modelId": "m", "schemaId": "s",
            "engine": "wasm-component", "lifecycle": "orishu.component/v1",
            "installPath": "/usr/lib/orishu"
        }"#;
        let error = serde_json::from_str::<ComponentInstance>(json).unwrap_err();
        assert!(
            error.to_string().contains("installPath"),
            "the error should name the rejected field, got: {error}"
        );
    }

    #[test]
    fn a_minimal_component_instance_round_trips() {
        let instance = ComponentInstance {
            instance_id: ComponentInstanceId::new("field").expect("valid"),
            artifact: ArtifactDescriptor {
                role: ArtifactRole::component(),
                digest: ArtifactDigest::sha256_of(b"wasm"),
                size_bytes: 4,
                media_type: MediaType::new("application/wasm").expect("valid"),
                schema: None,
            },
            plugin_id: PluginId::new("dev.orishu.em").expect("valid"),
            model_id: ModelId::new("yee").expect("valid"),
            schema_id: SchemaId::new("em.field/v1").expect("valid"),
            engine: Engine::new("wasm-component").expect("valid"),
            lifecycle: LifecycleId::new("orishu.component/v1").expect("valid"),
            roles: Vec::new(),
            state_ownership: Vec::new(),
            config: BTreeMap::new(),
            limits: BTreeMap::new(),
        };
        let json = serde_json::to_string(&instance).expect("it encodes");
        assert_eq!(
            serde_json::from_str::<ComponentInstance>(&json).expect("it decodes"),
            instance
        );
        // Empty collections are absent rather than present-and-empty, so an
        // authored document and a round-tripped one agree.
        assert!(!json.contains("roles"));
        assert!(!json.contains("config"));
    }

    #[test]
    fn reduction_spells_itself_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Reduction::Single).expect("it encodes"),
            "\"single\""
        );
        assert_eq!(
            serde_json::to_string(&Reduction::Sum).expect("it encodes"),
            "\"sum\""
        );
    }
}
