//! Immutable entity composition and authoring evidence, independent of Kagami.
//! Resolved values are scientific inputs. Source expressions are provenance only:
//! admission never evaluates them, applies defaults or consults a catalog.
use super::{ConfigurationValue, Kinematics};
use crate::{
    codec,
    projection::{Project, object},
    *,
};
use orishu_resource::ApiVersion;
use orishu_workload::{ComponentInstanceId, canonical::CanonicalValue as V};
use serde::{Deserialize, Serialize};

/// Portable initial scene/composition schema.
pub const SCENE_SCHEMA: &str = "orishu.simulation.scene/v1";

/// Resource policy for a cold scene, independent of per-step numerical budgets.
#[derive(Clone, Copy, Debug)]
pub struct SceneLimits {
    /// Maximum encoded bytes, checked before parsing.
    pub bytes: usize,
    /// Maximum objects, including objects without components.
    pub objects: usize,
    /// Maximum attached components across all objects.
    pub components: usize,
    /// Maximum values/sources across all components and kernels.
    pub properties: usize,
    /// Maximum variable provenance records.
    pub variables: usize,
    /// Maximum configured kernels with retained sources.
    pub kernels: usize,
    /// Maximum bytes in any text, expression or label.
    pub text_bytes: usize,
    /// Canonical tree work bound, including map keys.
    pub values: usize,
    /// Maximum object/field-slot comparisons when proving projection completeness.
    pub projection_work: usize,
}
impl Default for SceneLimits {
    fn default() -> Self {
        Self {
            bytes: 16 * 1024 * 1024,
            objects: 100_000,
            components: 400_000,
            properties: 1_000_000,
            variables: 16_384,
            kernels: 65,
            text_bytes: 16_384,
            values: 2_000_000,
            projection_work: 16_000_000,
        }
    }
}
impl SceneLimits {
    fn codec(self) -> Limits {
        Limits {
            max_payload_bytes: self.bytes,
            max_values: self.values,
            max_depth: 24,
            max_schema_items: self
                .objects
                .max(self.components)
                .max(self.properties)
                .max(self.variables)
                .max(self.kernels)
                .max(7),
            max_text_bytes: self.text_bytes,
            ..Limits::default()
        }
    }
}

/// Original quantity expression plus optional separately authored unit symbol.
/// These bytes are evidence, never runtime evaluation instructions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuantitySource {
    /// Exact original expression text.
    pub expression: String,
    /// Original separate unit annotation, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}
/// One resolved component property and optional quantity source evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneProperty {
    /// Exact local schema ID, not an expression alias.
    pub id: LocalContributionId,
    /// Complete resolved SI/literal value, independently checked at admission.
    pub value: ConfigurationValue,
    /// Present only for quantities. Absence is valid for external literal producers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<QuantitySource>,
}
/// One component attached to an object. Membership fixes its scientific schema;
/// a separate schema-version flag cannot override that exact provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneComponent {
    /// Exact selected contribution, never a plugin default or file path.
    pub contribution: ContributionRef,
    /// Complete explicit property set in strictly ascending ID order.
    pub properties: Vec<SceneProperty>,
}
/// Retained object geometry in SI. Kernels consume geometry only through their
/// declared execution contract; the current field profile uses point projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SceneShape {
    /// Sphere with positive radius in metres.
    Sphere {
        /// Positive radius in metres.
        #[serde(rename = "radiusMetres")]
        radius_metres: FiniteF64,
    },
    /// Box with nonnegative local half extents in metres.
    Box {
        /// Nonnegative local half extents in metres.
        #[serde(rename = "halfExtentMetres")]
        half_extent_metres: [FiniteF64; 3],
    },
}
/// Historical template content fingerprint; no lookup location/name is exported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateEvidence {
    /// Fingerprinted template's original schema, identifying the hash convention.
    pub api_version: String,
    /// SHA-256 of canonical source template content, NOT a required artifact edge.
    pub fingerprint: ArtifactDigest,
}
/// Complete initial entity composition. Entity IDs remain stable across projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneObject {
    /// Run-local stable identity.
    pub id: super::EntityId,
    /// Human label, never identity or executable selection.
    pub name: String,
    /// Initial position/velocity shared with the numerical packet.
    pub kinematics: Kinematics,
    /// Unit quaternion `[x,y,z,w]`, with no automatic normalization at admission.
    pub orientation: [FiniteF64; 4],
    /// Initial angular velocity in radians/second; unsupported profiles must refuse it.
    pub angular_velocity: [FiniteF64; 3],
    /// `None` is a point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<SceneShape>,
    /// Exact attached components in strictly ascending contribution order.
    pub components: Vec<SceneComponent>,
    /// Historical source only; losing that source cannot prevent admission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplateEvidence>,
}
/// Historical variable definition. Resolved scientific values reside at their
/// property/configuration uses; workers never evaluate this source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VariableEvidence {
    /// Original document-local identity, not a runtime variable handle.
    pub id: u64,
    /// Original fully qualified authoring name.
    pub name: String,
    /// Original expression text.
    pub expression: String,
    /// Optional explanation from the author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
/// Authored quantity sources for one captured kernel configuration. Literal
/// boolean/text values already reside in its resolved configuration artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KernelSource {
    /// Exact configured kernel use.
    pub instance: ComponentInstanceId,
    /// Explicitly authored property IDs, including booleans/text. Absence means
    /// the captured configuration used the selected declaration's default.
    pub authored: Vec<LocalContributionId>,
    /// Only explicitly authored quantity sources; omitted values came from defaults.
    pub quantities: Vec<PropertySource>,
}
/// Named source expression, separate from its already captured resolved value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySource {
    /// Property in the selected configuration schema.
    pub id: LocalContributionId,
    /// Retained original source.
    pub source: QuantitySource,
}
/// Initial scene data referenced by an execution-v2 definition. No blueprints or
/// emission semantics are implicit: those require an explicit schema extension.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDefinition {
    /// Exactly [`SCENE_SCHEMA`].
    pub api_version: ApiVersion,
    /// Complete object set, strictly ascending stable identity.
    pub objects: Vec<SceneObject>,
    /// Retained definitions, strictly ascending document identity.
    pub variables: Vec<VariableEvidence>,
    /// Complete kernel-source set, strictly ascending instance identity.
    pub kernels: Vec<KernelSource>,
}
fn invalid() -> Error {
    Error::new(
        ErrorCode::InvalidSelection,
        "scene",
        "invalid scene structure or ordering",
    )
}
fn limited() -> Error {
    Error::new(ErrorCode::LimitExceeded, "scene", "scene budget exceeded")
}
impl SceneDefinition {
    /// Structural validation only; selected membership, dimensions, required
    /// properties and agreement with numerical projections require admission.
    pub fn validate(&self, limits: SceneLimits) -> Result<(), Error> {
        if self.api_version.as_str() != SCENE_SCHEMA {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "scene",
                "unsupported scene schema",
            ));
        }
        if self.objects.len() > limits.objects
            || self.variables.len() > limits.variables
            || self.kernels.len() > limits.kernels
        {
            return Err(limited());
        }
        if self.objects.windows(2).any(|v| v[0].id >= v[1].id)
            || self.variables.windows(2).any(|v| v[0].id >= v[1].id)
            || self
                .kernels
                .windows(2)
                .any(|v| v[0].instance >= v[1].instance)
        {
            return Err(invalid());
        }
        let reserved_bytes = std::cell::Cell::new(0usize);
        let reserve = |n: usize| -> Result<(), Error> {
            reserved_bytes.set(
                reserved_bytes
                    .get()
                    .checked_add(n)
                    .filter(|n| *n <= limits.bytes)
                    .ok_or_else(limited)?,
            );
            Ok(())
        };
        let text = |s: &str| {
            if s.len() <= limits.text_bytes {
                reserve(s.len())
            } else {
                Err(limited())
            }
        };
        let source = |s: &QuantitySource| -> Result<(), Error> {
            text(&s.expression)?;
            if let Some(u) = &s.unit {
                text(u)?;
            }
            Ok(())
        };
        let mut components = 0usize;
        let mut properties = 0usize;
        // Conservative weight BEFORE constructing canonical tree copies. Each
        // property reserves room for its dimension/source and each exact identity.
        let mut weight = 16usize;
        let mut charge = |n: usize| -> Result<(), Error> {
            reserve(n.checked_mul(8).ok_or_else(limited)?)?;
            weight = weight
                .checked_add(n)
                .filter(|n| *n <= limits.values)
                .ok_or_else(limited)?;
            Ok(())
        };
        let mut names = std::collections::BTreeSet::new();
        for v in &self.variables {
            charge(16)?;
            text(&v.name)?;
            text(&v.expression)?;
            if !names.insert(&v.name) {
                return Err(invalid());
            }
            if let Some(d) = &v.description {
                text(d)?;
            }
        }
        for o in &self.objects {
            charge(100)?;
            text(&o.name)?;
            let norm: f64 = o.orientation.iter().map(|v| v.get() * v.get()).sum();
            if !norm.is_finite()
                || (norm - 1.0).abs() > 1e-12
                || o.components
                    .windows(2)
                    .any(|v| v[0].contribution >= v[1].contribution)
            {
                return Err(invalid());
            }
            match &o.shape {
                Some(SceneShape::Sphere { radius_metres }) if !radius_metres.is_positive() => {
                    return Err(invalid());
                }
                Some(SceneShape::Box { half_extent_metres })
                    if half_extent_metres.iter().any(|v| v.get() < 0.0) =>
                {
                    return Err(invalid());
                }
                _ => {}
            }
            if let Some(t) = &o.template {
                text(&t.api_version)?;
            }
            components = components
                .checked_add(o.components.len())
                .filter(|n| *n <= limits.components)
                .ok_or_else(limited)?;
            for c in &o.components {
                charge(16)?;
                properties = properties
                    .checked_add(c.properties.len())
                    .filter(|n| *n <= limits.properties)
                    .ok_or_else(limited)?;
                if c.properties.windows(2).any(|v| v[0].id >= v[1].id) {
                    return Err(invalid());
                }
                for p in &c.properties {
                    charge(36)?;
                    if let ConfigurationValue::Text { value } = &p.value {
                        text(value)?;
                    }
                    if let Some(s) = &p.source {
                        if !matches!(p.value, ConfigurationValue::Quantity { .. }) {
                            return Err(invalid());
                        }
                        source(s)?;
                    }
                }
            }
        }
        for k in &self.kernels {
            charge(10)?;
            properties = properties
                .checked_add(k.authored.len())
                .and_then(|n| n.checked_add(k.quantities.len()))
                .filter(|n| *n <= limits.properties)
                .ok_or_else(limited)?;
            if k.authored.windows(2).any(|v| v[0] >= v[1])
                || k.quantities.windows(2).any(|v| v[0].id >= v[1].id)
            {
                return Err(invalid());
            }
            for p in &k.quantities {
                if k.authored.binary_search(&p.id).is_err() {
                    return Err(invalid());
                }
                charge(12)?;
                source(&p.source)?;
            }
            for _ in &k.authored {
                charge(2)?;
            }
        }
        Ok(())
    }
    /// Explicit deterministic encoding, never serde's incidental map order.
    pub fn to_cbor(&self, limits: SceneLimits) -> Result<Vec<u8>, Error> {
        self.validate(limits)?;
        codec::encode(&self.project(), &limits.codec(), limits.bytes)
    }
    /// Byte/tree-bounded canonical decoding before typed collection construction.
    pub fn from_cbor(bytes: &[u8], limits: SceneLimits) -> Result<Self, Error> {
        let scene: Self = codec::structured_from_cbor(bytes, &limits.codec(), limits.bytes)?;
        if scene.to_cbor(limits)? != bytes {
            return Err(invalid());
        }
        Ok(scene)
    }
}

fn vector(v: &[FiniteF64]) -> V {
    V::Array(v.iter().map(Project::project).collect())
}
impl Project for Kinematics {
    fn project(&self) -> V {
        object(vec![
            ("position_metres", Some(vector(&self.position_metres))),
            (
                "velocity_metres_per_second",
                Some(vector(&self.velocity_metres_per_second)),
            ),
        ])
    }
}
impl Project for QuantitySource {
    fn project(&self) -> V {
        object(vec![
            ("expression", Some(V::text(&self.expression))),
            ("unit", self.unit.as_ref().map(V::text)),
        ])
    }
}
impl Project for SceneProperty {
    fn project(&self) -> V {
        object(vec![
            ("id", Some(self.id.project())),
            ("value", Some(self.value.project())),
            ("source", self.source.as_ref().map(Project::project)),
        ])
    }
}
impl Project for SceneComponent {
    fn project(&self) -> V {
        object(vec![
            ("contribution", Some(self.contribution.project())),
            ("properties", Some(self.properties.project())),
        ])
    }
}
impl Project for SceneShape {
    fn project(&self) -> V {
        match self {
            Self::Sphere { radius_metres } => object(vec![
                ("kind", Some(V::text("sphere"))),
                ("radiusMetres", Some(radius_metres.project())),
            ]),
            Self::Box { half_extent_metres } => object(vec![
                ("kind", Some(V::text("box"))),
                ("halfExtentMetres", Some(vector(half_extent_metres))),
            ]),
        }
    }
}
impl Project for SceneObject {
    fn project(&self) -> V {
        object(vec![
            ("id", Some(V::UInt(self.id.0))),
            ("name", Some(V::text(&self.name))),
            ("kinematics", Some(self.kinematics.project())),
            ("orientation", Some(vector(&self.orientation))),
            ("angularVelocity", Some(vector(&self.angular_velocity))),
            ("shape", self.shape.as_ref().map(Project::project)),
            ("components", Some(self.components.project())),
            (
                "template",
                self.template.as_ref().map(|t| {
                    object(vec![
                        ("apiVersion", Some(V::text(&t.api_version))),
                        ("fingerprint", Some(t.fingerprint.project())),
                    ])
                }),
            ),
        ])
    }
}
impl Project for VariableEvidence {
    fn project(&self) -> V {
        object(vec![
            ("id", Some(V::UInt(self.id))),
            ("name", Some(V::text(&self.name))),
            ("expression", Some(V::text(&self.expression))),
            ("description", self.description.as_ref().map(V::text)),
        ])
    }
}
impl Project for KernelSource {
    fn project(&self) -> V {
        object(vec![
            ("instance", Some(V::text(self.instance.as_str()))),
            ("authored", Some(self.authored.project())),
            ("quantities", Some(self.quantities.project())),
        ])
    }
}
impl Project for PropertySource {
    fn project(&self) -> V {
        object(vec![
            ("id", Some(self.id.project())),
            ("source", Some(self.source.project())),
        ])
    }
}
impl Project for SceneDefinition {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("objects", Some(self.objects.project())),
            ("variables", Some(self.variables.project())),
            ("kernels", Some(self.kernels.project())),
        ])
    }
}
