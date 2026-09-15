//! Raw versioned declarations. Acceptance is explicit, never a serde side effect.
use crate::*;
use orishu_resource::{ApiVersion, Kind, Resource};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Raw root envelope; status is uninhabited and rejected by validation.
pub type ReleaseEnvelope =
    Resource<ReleaseMetadata, ReleaseSpec, orishu_resource::NoStatus, orishu_resource::DenyUnknown>;

/// Raw release data, suitable for authoring but not yet validated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Release(pub ReleaseEnvelope);

impl Release {
    /// Build a raw v1 root. Call `validate` before accepting it.
    pub fn new(metadata: ReleaseMetadata, spec: ReleaseSpec) -> Self {
        Self(Resource::new(
            ApiVersion::from_static("orishu.plugin/v1"),
            Kind::from_static("PluginRelease"),
            metadata,
            spec,
        ))
    }
}

/// Human labels do not establish publisher authenticity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseMetadata {
    /// Logical plugin identity.
    pub plugin_id: PluginId,
    /// Bounded human version label, not ordered semver.
    pub version_label: String,
    /// Optional display label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional descriptive text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Root descriptor closure; every package file is declared here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseSpec {
    /// Set keyed by release-local identity.
    pub contributions: Vec<Contribution>,
    /// Set keyed by exact artifact digest.
    pub artifacts: Vec<Artifact>,
}

/// A descriptor intentionally has no location, executable authority or credentials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    /// Raw-content identity, shared with workloads.
    pub digest: ArtifactDigest,
    /// Exact size, checked before verification.
    pub size_bytes: u64,
    /// Declared media type; does not prove code is safe.
    pub media_type: String,
}

/// One contribution of a release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contribution {
    /// Unique throughout this release.
    pub local_id: LocalContributionId,
    /// Exact platform extension point/version.
    pub extension_point: ExtensionPointId,
    /// Digest of separately stored payload bytes.
    pub payload: ArtifactDigest,
    /// Unique named dependencies; no install-order selection.
    pub requirements: Vec<Requirement>,
    /// Host-rendered metadata, not code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// A dependency is either an exact scientific contract or a local contribution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Requirement {
    /// Independently supplied scientific vocabulary.
    Contract {
        /// Unique requirement slot.
        slot: LocalContributionId,
        /// Exact scientific identity.
        contract: ScientificContractRef,
    },
    /// Dependency on another declaration in the same release.
    Local {
        /// Unique requirement slot.
        slot: LocalContributionId,
        /// Release-local target.
        #[serde(rename = "localContribution")]
        local_contribution: LocalContributionId,
    },
}
impl Requirement {
    /// Slot independent of dependency form.
    pub fn slot(&self) -> &LocalContributionId {
        match self {
            Self::Contract { slot, .. } | Self::Local { slot, .. } => slot,
        }
    }
}

/// Exact scientific dependencies included in the semantic projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractRequirement {
    /// Named slot referenced by the scientific schema.
    pub slot: LocalContributionId,
    /// Identity, never a provider release or self-reference.
    pub contract: ScientificContractRef,
}

/// Optional host UI metadata excluded from scientific hashing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Annotations {
    /// Display label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Display description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Host grouping hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Display unit hint, never the numerical interchange unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_unit: Option<String>,
    /// Declared icon artifact, never an executable view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_artifact: Option<ArtifactDigest>,
}

/// A payload's scientific and presentation projections are deliberately separate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Declaration<T> {
    /// Scientific meaning, including defaults and exact dependency declarations.
    pub scientific: T,
    /// Optional non-scientific presentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<Annotations>,
}

macro_rules! scientific {
    ($name:ident, $doc:literal, {$($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)?}) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            /// Scientific name, not a provider identity.
            pub name: PluginId,
            /// Nonzero scientific schema version.
            pub version: NonZeroU32,
            /// Set of exact scientific dependencies keyed by slot.
            pub requirements: Vec<ContractRequirement>,
            $($(#[$attr])* pub $field: $ty,)*
        }
    };
}

scientific!(ComponentSchema, "Entity component vocabulary; no memory layout or native system.", {
    /// Ordered property declarations.
    properties: Vec<Property>,
    /// Participation role; Dynamics opts into integration.
    role: ComponentRole,
    /// Property bindings for standard bulk projections.
    bindings: RoleBindings,
});

/// Component semantics; without Dynamics entities remain kinematic/static.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentRole {
    /// Additive data only.
    Data,
    /// Inertial response to accumulated forces.
    Dynamics,
    /// Source and/or response coupling to a field.
    FieldCoupling,
}

/// Named scalar properties used by the common runtime input projection.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleBindings {
    /// Required only for Dynamics; must have mass dimension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inertial_mass: Option<LocalContributionId>,
    /// Optional field-source strength property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<LocalContributionId>,
    /// Optional response strength; may differ from source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<LocalContributionId>,
}

/// An editable component/configuration property.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Property {
    /// Unique name within this property list.
    pub id: LocalContributionId,
    /// Whether an instance must supply this property.
    pub required: bool,
    /// Typed default and scalar constraints; expressions are retained source.
    pub schema: PropertyType,
}

/// Initial property schema: scalar quantity expressions, booleans or bounded text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PropertyType {
    /// SI dimension is explicit; default source is evaluated by the variables owner.
    Quantity {
        /// Physical dimension in shared SI exponent order.
        dimension: Dimension,
        /// Retained authored default expression; absence means no default.
        #[serde(rename = "defaultExpression", skip_serializing_if = "Option::is_none")]
        default_expression: Option<String>,
        /// Inclusive resolved SI minimum.
        #[serde(rename = "minimumSI", skip_serializing_if = "Option::is_none")]
        minimum_si: Option<FiniteF64>,
        /// Inclusive resolved SI maximum.
        #[serde(rename = "maximumSI", skip_serializing_if = "Option::is_none")]
        maximum_si: Option<FiniteF64>,
    },
    /// Boolean value with optional literal default.
    Boolean {
        /// Default, when supplied.
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    /// UTF-8 text, constrained by encoded bytes.
    Text {
        /// Maximum bytes of the instance value.
        #[serde(rename = "maxBytes")]
        max_bytes: u32,
        /// Default, when supplied.
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
}

scientific!(FieldSchema, "A field family is defined over each point of its declared domain.", {
    /// Spatial dimension, exactly three in v1.
    domain_dimension: u8,
    /// Ordered model-domain/discretization requirements, declarative identifiers.
    domain_requirements: Vec<LocalContributionId>,
    /// Required minimum channels, as scientific requirement slots.
    required_observables: Vec<LocalContributionId>,
});

scientific!(ObservableSchema, "Typed scientific channel independent of opaque field layout.", {
    /// Meaning of values, not merely their shape.
    meaning: String,
    /// Scalar/fixed vector/fixed matrix.
    shape: Shape,
    /// One SI dimension for every component of this channel.
    dimension: Dimension,
    /// Coordinate/reference frame.
    frame: String,
    /// Ordered axis meanings; one for a vector, two for a matrix, none for scalar.
    axes: Vec<String>,
    /// Gauge/index/interpolation conventions, including Jacobian interpretation.
    conventions: String,
});

/// Matrices are row-major; shape does not imply spatial interpretation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Shape {
    /// A scalar.
    Scalar,
    /// One to sixteen components.
    Vector {
        /// Fixed component count.
        length: u8,
    },
    /// One to sixteen rows/columns, at most 256 elements.
    Matrix {
        /// Fixed row count.
        rows: u8,
        /// Fixed column count.
        columns: u8,
    },
}

scientific!(ConstantsSchema, "Read-only dimensioned defaults; imports capture values and provenance.", {
    /// Named constants; set keyed by constant ID.
    constants: Vec<Constant>,
});

/// One literal scientific constant, never a live mutable registry variable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Constant {
    /// Unique exported name.
    pub id: LocalContributionId,
    /// Shared SI dimension.
    pub dimension: Dimension,
    /// Finite literal in SI units.
    #[serde(rename = "valueSI")]
    pub value_si: FiniteF64,
    /// Scientific meaning included in semantic hash.
    pub meaning: String,
}

/// Versioned opaque state/history format identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateFormat {
    /// Plugin-owned format name.
    pub id: PluginId,
    /// Nonzero format version; same name is not sufficient for compatibility.
    pub version: NonZeroU32,
}

/// The only force/evaluation convention supported by this first profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionProfile {
    /// All fields evaluate committed kinematics, then one integration stage.
    #[serde(rename = "orishu.force-then-integrate/v1")]
    ForceThenIntegrate,
}

scientific!(FieldModelSchema, "One field update kernel with declared typed sampling support.", {
    /// Requirement slot naming its field family.
    field: LocalContributionId,
    /// Requirement slots naming coupling components.
    couplings: Vec<LocalContributionId>,
    /// Ordered editable configuration parameters.
    configuration: Vec<Property>,
    /// Portable opaque scientific format.
    state_format: StateFormat,
    /// One independent executable artifact.
    kernel: ArtifactDigest,
    /// Must be the field contract.
    execution_contract: ExecutionContractId,
    /// Supplied observable requirement slots, possibly beyond the family minimum.
    observables: Vec<LocalContributionId>,
    /// Initial fixed scientific execution profile.
    profile: ExecutionProfile,
    /// Scientific convention relating produced field state to step time.
    field_time_convention: String,
    /// Declared maximum state bytes, not an instruction to allocate them.
    max_state_bytes: u64,
});

scientific!(IntegratorSchema, "Swappable integration formula; runtime owns reduction and commit.", {
    /// Requirement slot naming Dynamics vocabulary.
    dynamics: LocalContributionId,
    /// Ordered editable configuration parameters.
    configuration: Vec<Property>,
    /// One independent executable artifact.
    kernel: ArtifactDigest,
    /// Must be the dynamics contract.
    execution_contract: ExecutionContractId,
    /// Initial fixed scientific execution profile.
    profile: ExecutionProfile,
    /// Required history format, bounds and newborn cold-start policy.
    history: HistorySchema,
});

/// Explicit numerical history independent of visible trails.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistorySchema {
    /// Versioned portable history encoding.
    pub format: StateFormat,
    /// Zero permits a memoryless integrator.
    pub max_bytes_per_entity: u32,
    /// Zero for a memoryless method, otherwise declared bounded sample count.
    pub samples: u32,
    /// Kernel-defined newborn initialization rule, included in scientific identity.
    pub cold_start: String,
}

/// Understood payload after decoding its external extension-point discriminator.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// Component declaration.
    Components(Declaration<ComponentSchema>),
    /// Field declaration.
    Fields(Declaration<FieldSchema>),
    /// Observable declaration.
    Observables(Declaration<ObservableSchema>),
    /// Constant declarations.
    Constants(Declaration<ConstantsSchema>),
    /// Field model/kernel declaration.
    FieldModels(Declaration<FieldModelSchema>),
    /// Integration kernel declaration.
    Integrators(Declaration<IntegratorSchema>),
}
