//! Artifact descriptors: identity-only references to the bytes a workload needs.
//!
//! # What is deliberately absent
//!
//! There is no `uri`, `url`, `path`, `registry`, `tag`, `peer`, `source`, or
//! `credential` field on [`ArtifactDescriptor`], and there is no variant that
//! carries inline bytes. That absence *is* the contract ADR 0010 decided:
//! retrieval location is distribution metadata, and a manifest that named one
//! would make two workers able to execute different bytes for the same
//! apparent workload.
//!
//! A descriptor answers "which bytes?" and nothing else. "Where do I get them?"
//! is a question for a submission request, a distribution envelope, or a live
//! availability index — none of which are part of workload identity, and all of
//! which may change without producing a different workload.
//!
//! `tests/canonical.rs` asserts the absence against the encoded bytes rather
//! than against this comment: it decodes a real workload's canonical form and
//! fails if any location-shaped key appears anywhere in it.

use serde::{Deserialize, Serialize};

use crate::digest::ArtifactDigest;
use crate::ids::{MediaType, NameError, SchemaId};

/// What an artifact is *for* within its workload.
///
/// An open validated vocabulary rather than a closed enum: ADR 0024 describes
/// field models, coupling phases, Dynamics and emitters as initial roles rather
/// than a closed list of physical phenomena, and a plugin may pin an artifact
/// this crate has never heard of. The constants below name the roles the rest
/// of the model reasons about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactRole(String);

// Hand-written for the same reason as the other validated names, and through
// the same helper so the two cannot drift: `#[serde(try_from = "String")]`
// would allocate every authored role, oversized ones included, before anything
// looked at its length.
impl Serialize for ArtifactRole {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ArtifactRole {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::ids::deserialize_name(deserializer, "an ArtifactRole")
    }
}

impl ArtifactRole {
    /// Executable component code. A component instance must name an artifact
    /// carrying this role.
    pub const COMPONENT: &'static str = "component";
    /// State at the initial simulation boundary.
    pub const INITIAL_CONDITIONS: &'static str = "initial-conditions";
    /// Static domain geometry: mesh, boundaries, sources and sinks.
    pub const GEOMETRY: &'static str = "geometry";
    /// A declarative state or observation schema.
    pub const SCHEMA: &'static str = "schema";
    /// A bounded emitter spawn blueprint, materialised at authoring time.
    pub const EMITTER_BLUEPRINT: &'static str = "emitter-blueprint";
    /// A material or constitutive property table.
    pub const MATERIAL_TABLE: &'static str = "material-table";

    /// Parses a role name.
    ///
    /// Checks the borrowed value before copying it, so an oversized role is
    /// refused without being allocated.
    ///
    /// # Errors
    ///
    /// Returns [`NameError`] when the value is empty, oversized, or contains a
    /// character outside printable non-whitespace ASCII.
    pub fn new<V: AsRef<str> + Into<String>>(value: V) -> Result<Self, NameError> {
        crate::ids::validate_role(value.as_ref())?;
        Ok(Self(value.into()))
    }

    /// The role a component instance's artifact must carry.
    #[must_use]
    pub fn component() -> Self {
        Self(Self::COMPONENT.to_owned())
    }

    /// Borrows the validated role name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this role may appear at most once in a workload.
    ///
    /// Geometry is singular because a domain has one static geometry. Component
    /// and initial-conditions artifacts are plural by design: ADR 0024's graph
    /// has several components, and a workload may pin one initial-condition
    /// artifact per owned state channel.
    #[must_use]
    pub fn is_singular(&self) -> bool {
        self.0 == Self::GEOMETRY
    }
}

impl std::fmt::Display for ArtifactRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::str::FromStr for ArtifactRole {
    type Err = NameError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for ArtifactRole {
    type Error = NameError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ArtifactRole> for String {
    fn from(value: ArtifactRole) -> Self {
        value.0
    }
}

/// The format contract an artifact's bytes satisfy.
///
/// Present so a worker can refuse an artifact whose schema its component does
/// not implement *before* handing bytes to a guest, rather than discovering the
/// mismatch as a decode failure inside the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaCompat {
    /// The stable schema identity.
    pub schema_id: SchemaId,
    /// The schema revision these bytes conform to.
    ///
    /// A plain integer rather than a semantic version: compatibility is decided
    /// by the component that declares the schema, and a version string would
    /// invite a reader here to guess at range semantics it does not own.
    pub version: u32,
}

/// An immutable, location-free reference to one artifact.
///
/// Every field is identity-bearing and participates in the workload digest.
/// Changing any of them produces a different workload.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDescriptor {
    /// What these bytes are for.
    pub role: ArtifactRole,
    /// Which bytes. The only thing that decides whether a candidate blob is
    /// this artifact.
    pub digest: ArtifactDigest,
    /// Exact byte length.
    ///
    /// Checked in addition to the digest, not instead of it: a declared size
    /// lets a reader refuse an oversized transfer before hashing it, and a
    /// mismatch between declared and actual size is a corrupt closure even when
    /// the manifest's own arithmetic is self-consistent.
    pub size_bytes: u64,
    /// The byte format.
    pub media_type: MediaType,
    /// Format/schema compatibility, where the role requires one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaCompat>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(role: &str) -> ArtifactDescriptor {
        ArtifactDescriptor {
            role: ArtifactRole::new(role).expect("a valid role"),
            digest: ArtifactDigest::sha256_of(b"bytes"),
            size_bytes: 5,
            media_type: MediaType::new("application/octet-stream").expect("a valid media type"),
            schema: None,
        }
    }

    #[test]
    fn a_descriptor_serializes_only_identity_bearing_fields() {
        let json = serde_json::to_value(descriptor(ArtifactRole::COMPONENT)).expect("it encodes");
        let object = json.as_object().expect("a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["digest", "mediaType", "role", "sizeBytes"]);
    }

    #[test]
    fn a_descriptor_refuses_a_location_field() {
        // The whole point of ADR 0010 is that a location cannot ride along in
        // the manifest. `deny_unknown_fields` is what makes an author's attempt
        // an error rather than a silently dropped key — and a silently dropped
        // key would be worse than an error, because the workload would keep its
        // digest while the author believed the URL was doing something.
        let with_uri = r#"{
            "role": "component",
            "digest": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "sizeBytes": 0,
            "mediaType": "application/wasm",
            "uri": "oci://registry.example.com/sim/em:v1"
        }"#;
        let error = serde_json::from_str::<ArtifactDescriptor>(with_uri).unwrap_err();
        assert!(
            error.to_string().contains("uri"),
            "the error should name the rejected field, got: {error}"
        );
    }

    #[test]
    fn geometry_is_singular_and_components_are_not() {
        assert!(
            ArtifactRole::new(ArtifactRole::GEOMETRY)
                .expect("valid")
                .is_singular()
        );
        assert!(!ArtifactRole::component().is_singular());
        assert!(
            !ArtifactRole::new(ArtifactRole::INITIAL_CONDITIONS)
                .expect("valid")
                .is_singular()
        );
    }

    #[test]
    fn a_role_outside_the_named_vocabulary_is_accepted() {
        // ADR 0024 calls its roles initial, not closed. A plugin pinning a
        // role this crate has never heard of is not an error.
        let role = ArtifactRole::new("spectral-basis").expect("an open vocabulary");
        assert_eq!(role.as_str(), "spectral-basis");
        assert!(!role.is_singular());
    }

    #[test]
    fn a_malformed_role_is_refused() {
        assert!(ArtifactRole::new("").is_err());
        assert!(ArtifactRole::new("two words").is_err());
    }

    #[test]
    fn a_role_is_validated_as_a_borrowed_value_on_both_paths() {
        // The constructor takes `AsRef<str>`, so the grammar runs on the
        // caller's borrow; the deserializer goes through the same shared
        // visitor as every other validated name, so an oversized role is
        // refused without being copied on the authoring path too.
        let oversized: String = "r".repeat(crate::ids::MAX_SYMBOL_LEN + 1);
        let borrowed: &str = &oversized;
        assert!(ArtifactRole::new(borrowed).is_err());
        assert!(serde_json::from_str::<ArtifactRole>(&format!("\"{oversized}\"")).is_err());
        assert!(serde_yaml::from_str::<ArtifactRole>(&oversized).is_err());
    }

    #[test]
    fn a_role_round_trips_through_serde() {
        let role = ArtifactRole::component();
        let json = serde_json::to_string(&role).expect("it encodes");
        assert_eq!(json, "\"component\"");
        assert_eq!(
            serde_json::from_str::<ArtifactRole>(&json).expect("it decodes"),
            role
        );
    }

    #[test]
    fn a_schema_compat_round_trips() {
        let compat = SchemaCompat {
            schema_id: SchemaId::new("dev.orishu.em.field/v1").expect("valid"),
            version: 3,
        };
        let json = serde_json::to_string(&compat).expect("it encodes");
        assert_eq!(json, r#"{"schemaId":"dev.orishu.em.field/v1","version":3}"#);
        assert_eq!(
            serde_json::from_str::<SchemaCompat>(&json).expect("it decodes"),
            compat
        );
    }
}
