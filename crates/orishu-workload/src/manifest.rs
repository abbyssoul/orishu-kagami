//! The immutable workload manifest: the root of a workload's identity.
//!
//! # What is structurally excluded
//!
//! Three exclusions are enforced by the type rather than by a serializer
//! attribute, because "the digest must not include this" is too important to
//! rest on a `skip_serializing_if`:
//!
//! - **Runtime status.** [`WorkloadManifest`] is a
//!   [`Resource`] with [`NoStatus`], an uninhabited type. A phase, simulation
//!   time, epoch or partition map cannot be put into this value at all — not
//!   skipped when encoding, but unrepresentable.
//! - **The cluster-assigned resource identity.** [`WorkloadMeta`] is its own
//!   type, deliberately not `orishu::model::manifest::ObjectMeta`. It has no
//!   `uid` and no `namespace`, so the identifier a worker assigns on admission
//!   cannot reach the digest and make one logical workload have two identities
//!   on two clusters.
//! - **Retrieval locations.** See [`crate::artifact`].
//!
//! # Unknown fields are an error
//!
//! Both the envelope ([`DenyUnknown`]) and every spec type
//! (`deny_unknown_fields`) refuse a key they do not recognise. For an
//! identity-bearing document this is the safer failure: a silently dropped key
//! leaves the author believing it did something while the digest says it did
//! not.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use orishu_resource::{
    ApiVersion, DenyUnknown, Kind, NoStatus, Resource, ResourceError, ResourceHeader,
    UnexpectedDiscriminator,
};

use crate::artifact::ArtifactDescriptor;
use crate::domain::DomainSpec;
use crate::graph::ComputeSpec;
use crate::ids::{LabelKey, LabelValue, ParameterName, WorkloadName};
use crate::value::ScalarValue;

/// The resource format version this crate reads and writes.
///
/// `v2` while the superseded unpinned prototype in `crates/orishu` still
/// answers to `orishu.dev/v1`, so exactly one parser claims each discriminator.
/// When worker admission moves to this model and the prototype is deleted, this
/// becomes the group's `v1` workload version.
pub const WORKLOAD_API_VERSION: &str = "orishu.dev/v2";

/// The resource kind for a workload.
pub const WORKLOAD_KIND: &str = "Workload";

/// [`WORKLOAD_API_VERSION`] as the validated type the envelope carries.
///
/// # Panics
///
/// Never: the constant is checked by a unit test in this module.
#[must_use]
pub fn api_version() -> ApiVersion {
    ApiVersion::from_static(WORKLOAD_API_VERSION)
}

/// [`WORKLOAD_KIND`] as the validated type the envelope carries.
///
/// # Panics
///
/// Never: the constant is checked by a unit test in this module.
#[must_use]
pub fn kind() -> Kind {
    Kind::from_static(WORKLOAD_KIND)
}

/// Identity-bearing metadata.
///
/// Both fields participate in the workload digest, which is why neither may be
/// a server annotation: changing a label produces a different workload, and
/// that is the intended contract, not an accident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadMeta {
    /// A human-readable name, useful for discovery and never an integrity
    /// identity.
    pub name: WorkloadName,
    /// Descriptive labels.
    ///
    /// A `BTreeMap` rather than a `HashMap`: the canonical encoder needs a
    /// stable order, and getting it from the container means an unordered map
    /// never reaches the encoder in the first place.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<LabelKey, LabelValue>,
}

impl WorkloadMeta {
    /// Metadata carrying only a name.
    #[must_use]
    pub fn new(name: WorkloadName) -> Self {
        Self {
            name,
            labels: BTreeMap::new(),
        }
    }
}

/// The artifacts a workload consumes that are not executable code.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadInputs {
    /// Static domain geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<ArtifactDescriptor>,
    /// State at the initial simulation boundary, one artifact per owned state
    /// channel that needs one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_conditions: Vec<ArtifactDescriptor>,
    /// Any other artifacts this workload's profile requires.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional: Vec<ArtifactDescriptor>,
}

/// What a worker must satisfy to run this workload.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadRequirements {
    /// Minimum hardware capability, as declared key/value pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hardware: BTreeMap<ParameterName, ScalarValue>,
    /// The determinism and numerical contract workers must honour.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub execution_profile: BTreeMap<ParameterName, ScalarValue>,
}

/// Everything a workload declares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadSpec {
    /// What runs, how it is wired, and in what order.
    pub compute: ComputeSpec,
    /// What physical region exists, and how it is sampled.
    pub domain: DomainSpec,
    /// The non-code artifacts it consumes.
    #[serde(default)]
    pub inputs: WorkloadInputs,
    /// What a worker must satisfy to run it.
    #[serde(default)]
    pub requirements: WorkloadRequirements,
}

/// An immutable workload: the shared envelope over [`WorkloadMeta`] and
/// [`WorkloadSpec`], with no status slot.
pub type WorkloadManifest = Resource<WorkloadMeta, WorkloadSpec, NoStatus, DenyUnknown>;

/// Builds a workload manifest with the correct discriminator.
///
/// The only constructor, so a workload cannot exist carrying the wrong
/// `apiVersion` or a placeholder kind.
#[must_use]
pub fn manifest(metadata: WorkloadMeta, spec: WorkloadSpec) -> WorkloadManifest {
    WorkloadManifest::new(api_version(), kind(), metadata, spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_discriminator_constants_are_valid() {
        // `api_version` and `kind` panic on an invalid constant, so this is the
        // test that keeps those panics unreachable.
        assert_eq!(api_version().as_str(), WORKLOAD_API_VERSION);
        assert_eq!(kind().as_str(), WORKLOAD_KIND);
    }

    #[test]
    fn the_workload_version_is_distinct_from_the_superseded_prototype() {
        // Two Rust models exist while worker admission still parses the old
        // one. They must not both answer to the same discriminator.
        assert_ne!(WORKLOAD_API_VERSION, "orishu.dev/v1");
    }

    #[test]
    fn metadata_has_no_slot_for_a_server_assigned_identifier() {
        let json = r#"{"name":"cavity","uid":"wl-7f3a"}"#;
        let error = serde_json::from_str::<WorkloadMeta>(json).unwrap_err();
        assert!(
            error.to_string().contains("uid"),
            "the error should name the rejected field, got: {error}"
        );
    }

    #[test]
    fn metadata_has_no_slot_for_a_namespace() {
        let json = r#"{"name":"cavity","namespace":"cluster_xyz"}"#;
        assert!(serde_json::from_str::<WorkloadMeta>(json).is_err());
    }

    #[test]
    fn a_manifest_has_no_status_slot_at_all() {
        // `NoStatus` is uninhabited, so this is not "status is skipped when
        // encoding" -- there is no value that could occupy the slot. A
        // `status` carrying anything is refused, and under `DenyUnknown` so is
        // an explicit null.
        let with_status = r#"{
            "apiVersion": "orishu.dev/v2",
            "kind": "Workload",
            "metadata": {"name": "cavity"},
            "spec": {},
            "status": {"phase": "Running"}
        }"#;
        assert!(serde_json::from_str::<WorkloadManifest>(with_status).is_err());
    }

    #[test]
    fn labels_are_ordered_by_the_container_that_holds_them() {
        // The canonical encoder needs a stable order. Taking it from the
        // container means an unordered map never reaches the encoder.
        let json = r#"{"name":"cavity","labels":{"zone":"b","domain":"em"}}"#;
        let meta: WorkloadMeta = serde_json::from_str(json).expect("it decodes");
        let keys: Vec<&str> = meta.labels.keys().map(LabelKey::as_str).collect();
        assert_eq!(keys, ["domain", "zone"]);
    }
}
