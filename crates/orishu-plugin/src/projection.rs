//! Explicit identity projections. No serde serialization occurs on this path.
use crate::*;
use orishu_workload::canonical::{CanonicalMap, CanonicalValue as V};

pub(crate) trait Project {
    fn project(&self) -> V;
}
pub(crate) fn object(fields: Vec<(&'static str, Option<V>)>) -> V {
    V::Map(CanonicalMap::fields(fields).expect("projection field names are static and unique"))
}
fn optional<T: Project>(value: &Option<T>) -> Option<V> {
    value.as_ref().map(Project::project)
}
fn sorted<T: Project, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> V {
    let mut ordered: Vec<_> = items.iter().collect();
    ordered.sort_by_key(|item| key(item));
    V::Array(ordered.into_iter().map(Project::project).collect())
}
impl<T: Project> Project for Vec<T> {
    fn project(&self) -> V {
        V::Array(self.iter().map(Project::project).collect())
    }
}
impl Project for String {
    fn project(&self) -> V {
        V::text(self)
    }
}
impl Project for bool {
    fn project(&self) -> V {
        V::Bool(*self)
    }
}
impl Project for FiniteF64 {
    fn project(&self) -> V {
        V::F64(*self)
    }
}
impl Project for Dimension {
    fn project(&self) -> V {
        V::Array(
            self.exponents()
                .iter()
                .map(|v| V::int(i64::from(*v)))
                .collect(),
        )
    }
}
macro_rules! text { ($($ty:ty),*) => { $(impl Project for $ty { fn project(&self) -> V { V::text(self.to_string()) } })* }; }
text!(
    PluginId,
    LocalContributionId,
    ExtensionPointId,
    ArtifactDigest,
    PluginReleaseId,
    ScientificDigest
);
macro_rules! uint { ($($ty:ty),*) => { $(impl Project for $ty { fn project(&self) -> V { V::UInt(u64::from(*self)) } })* }; }
uint!(u8, u32, u64);
impl Project for std::num::NonZeroU32 {
    fn project(&self) -> V {
        self.get().project()
    }
}

macro_rules! fields {
    ($ty:ty, {$($wire:literal => $field:ident),* $(,)?} $(, optional {$($owire:literal => $ofield:ident),* $(,)?})?) => {
        impl Project for $ty {
            fn project(&self) -> V {
                object(vec![$(($wire, Some(self.$field.project())),)* $($(($owire, optional(&self.$ofield)),)*)?])
            }
        }
    };
}
fields!(ScientificContractRef, {"name"=>name, "version"=>version, "digest"=>digest});
fields!(ContributionRef, {"release"=>release, "extensionPoint"=>extension_point, "localId"=>local_id});
fields!(Artifact, {"digest"=>digest,"sizeBytes"=>size_bytes,"mediaType"=>media_type});
fields!(ReleaseMetadata, {"pluginId"=>plugin_id,"versionLabel"=>version_label}, optional {"displayName"=>display_name,"description"=>description});
fields!(Annotations, {}, optional {"label"=>label,"description"=>description,"group"=>group,"preferredUnit"=>preferred_unit,"iconArtifact"=>icon_artifact});
fields!(ContractRequirement, {"slot"=>slot,"contract"=>contract});
fields!(RoleBindings, {}, optional {"inertialMass"=>inertial_mass,"source"=>source,"response"=>response});
fields!(Property, {"id"=>id,"required"=>required,"schema"=>schema});
fields!(Constant, {"id"=>id,"dimension"=>dimension,"valueSI"=>value_si,"meaning"=>meaning});
fields!(StateFormat, {"id"=>id,"version"=>version});
fields!(HistorySchema, {"format"=>format,"maxBytesPerEntity"=>max_bytes_per_entity,"samples"=>samples,"coldStart"=>cold_start});

impl Project for Release {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.0.api_version().as_str()))),
            ("kind", Some(V::text(self.0.kind().as_str()))),
            ("metadata", Some(self.0.metadata.project())),
            (
                "spec",
                Some(object(vec![
                    (
                        "contributions",
                        Some(sorted(&self.0.spec.contributions, |c| c.local_id.clone())),
                    ),
                    (
                        "artifacts",
                        Some(sorted(&self.0.spec.artifacts, |a| a.digest)),
                    ),
                ])),
            ),
        ])
    }
}
impl Project for Contribution {
    fn project(&self) -> V {
        object(vec![
            ("localId", Some(self.local_id.project())),
            ("extensionPoint", Some(self.extension_point.project())),
            ("payload", Some(self.payload.project())),
            (
                "requirements",
                Some(sorted(&self.requirements, |r| r.slot().clone())),
            ),
            ("annotations", optional(&self.annotations)),
        ])
    }
}
impl Project for Requirement {
    fn project(&self) -> V {
        match self {
            Self::Contract { slot, contract } => object(vec![
                ("slot", Some(slot.project())),
                ("contract", Some(contract.project())),
            ]),
            Self::Local {
                slot,
                local_contribution,
            } => object(vec![
                ("slot", Some(slot.project())),
                ("localContribution", Some(local_contribution.project())),
            ]),
        }
    }
}
impl<T: Project> Project for Declaration<T> {
    fn project(&self) -> V {
        object(vec![
            ("scientific", Some(self.scientific.project())),
            ("presentation", optional(&self.presentation)),
        ])
    }
}
impl Project for ComponentRole {
    fn project(&self) -> V {
        V::text(match self {
            Self::Data => "data",
            Self::Dynamics => "dynamics",
            Self::FieldCoupling => "field-coupling",
        })
    }
}
impl Project for ExecutionContractId {
    fn project(&self) -> V {
        V::text(match self {
            Self::Field => "orishu:simulation/field@1",
            Self::Dynamics => "orishu:simulation/dynamics@1",
        })
    }
}
impl Project for ExecutionProfile {
    fn project(&self) -> V {
        V::text("orishu.force-then-integrate/v1")
    }
}
impl Project for Shape {
    fn project(&self) -> V {
        match self {
            Self::Scalar => object(vec![("kind", Some(V::text("scalar")))]),
            Self::Vector { length } => object(vec![
                ("kind", Some(V::text("vector"))),
                ("length", Some(length.project())),
            ]),
            Self::Matrix { rows, columns } => object(vec![
                ("kind", Some(V::text("matrix"))),
                ("rows", Some(rows.project())),
                ("columns", Some(columns.project())),
            ]),
        }
    }
}
impl Project for PropertyType {
    fn project(&self) -> V {
        match self {
            Self::Quantity {
                dimension,
                default_expression,
                minimum_si,
                maximum_si,
            } => object(vec![
                ("kind", Some(V::text("quantity"))),
                ("dimension", Some(dimension.project())),
                ("defaultExpression", optional(default_expression)),
                ("minimumSI", optional(minimum_si)),
                ("maximumSI", optional(maximum_si)),
            ]),
            Self::Boolean { default } => object(vec![
                ("kind", Some(V::text("boolean"))),
                ("default", optional(default)),
            ]),
            Self::Text { max_bytes, default } => object(vec![
                ("kind", Some(V::text("text"))),
                ("maxBytes", Some(max_bytes.project())),
                ("default", optional(default)),
            ]),
        }
    }
}
macro_rules! scientific {
    ($ty:ty, {$($wire:literal => $field:ident),* $(,)?}) => {
        impl Project for $ty {
            fn project(&self) -> V { object(vec![
                ("name",Some(self.name.project())),("version",Some(self.version.project())),
                ("requirements",Some(sorted(&self.requirements, |r| r.slot.clone()))),
                $(($wire,Some(self.$field.project())),)*
            ]) }
        }
    };
}
scientific!(ComponentSchema,{"properties"=>properties,"role"=>role,"bindings"=>bindings});
scientific!(FieldSchema,{"domainDimension"=>domain_dimension,"domainRequirements"=>domain_requirements,"requiredObservables"=>required_observables});
scientific!(ObservableSchema,{"meaning"=>meaning,"shape"=>shape,"dimension"=>dimension,"frame"=>frame,"axes"=>axes,"conventions"=>conventions});
scientific!(FieldModelSchema,{"field"=>field,"couplings"=>couplings,"configuration"=>configuration,"stateFormat"=>state_format,"kernel"=>kernel,"executionContract"=>execution_contract,"observables"=>observables,"profile"=>profile,"fieldTimeConvention"=>field_time_convention,"maxStateBytes"=>max_state_bytes});
scientific!(IntegratorSchema,{"dynamics"=>dynamics,"configuration"=>configuration,"kernel"=>kernel,"executionContract"=>execution_contract,"profile"=>profile,"history"=>history});
impl Project for ConstantsSchema {
    fn project(&self) -> V {
        object(vec![
            ("name", Some(self.name.project())),
            ("version", Some(self.version.project())),
            (
                "requirements",
                Some(sorted(&self.requirements, |r| r.slot.clone())),
            ),
            ("constants", Some(sorted(&self.constants, |c| c.id.clone()))),
        ])
    }
}

/// One dispatch macro keeps variants visible, without erasing schema types.
macro_rules! dispatch {
    ($self:expr, $value:ident => $body:expr) => {
        match $self {
            Payload::Components($value) => $body,
            Payload::Fields($value) => $body,
            Payload::Observables($value) => $body,
            Payload::Constants($value) => $body,
            Payload::FieldModels($value) => $body,
            Payload::Integrators($value) => $body,
        }
    };
}
pub(crate) use dispatch;
impl Project for Payload {
    fn project(&self) -> V {
        dispatch!(self, v => v.project())
    }
}
impl Payload {
    pub(crate) fn scientific_projection(&self) -> V {
        dispatch!(self, v => v.scientific.project())
    }
    /// Exact scientific dependency declarations, not resolved providers.
    pub fn requirements(&self) -> &[ContractRequirement] {
        dispatch!(self, v => &v.scientific.requirements)
    }
    /// Presentation metadata excluded from semantic identity.
    pub fn presentation(&self) -> Option<&Annotations> {
        dispatch!(self, v => v.presentation.as_ref())
    }
    /// Understood extension point for this typed payload.
    pub fn point(&self) -> KnownPoint {
        match self {
            Self::Components(_) => KnownPoint::Components,
            Self::Fields(_) => KnownPoint::Fields,
            Self::Observables(_) => KnownPoint::Observables,
            Self::Constants(_) => KnownPoint::Constants,
            Self::FieldModels(_) => KnownPoint::FieldModels,
            Self::Integrators(_) => KnownPoint::Integrators,
        }
    }
    /// Independent executable artifact, if this is a computational contribution.
    pub fn kernel(&self) -> Option<ArtifactDigest> {
        match self {
            Self::FieldModels(v) => Some(v.scientific.kernel),
            Self::Integrators(v) => Some(v.scientific.kernel),
            _ => None,
        }
    }
}
