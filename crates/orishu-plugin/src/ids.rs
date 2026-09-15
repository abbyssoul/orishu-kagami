//! Identity domains; names never resolve a provider implicitly.
use crate::{ArtifactDigest, Error};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, str::FromStr};

fn segment(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

macro_rules! name {
    ($name:ident, $doc:literal, $check:expr) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            /// Validate before copying the supplied spelling; no normalization.
            pub fn new(value: &str) -> Result<Self, Error> {
                if !($check)(value) {
                    return Err(Error::malformed(stringify!($name), "invalid identifier"));
                }
                Ok(Self(value.to_owned()))
            }
            /// Exact spelling.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                Self::new(s)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str(stringify!($name))
                    }
                    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                        $name::new(s).map_err(E::custom)
                    }
                }
                d.deserialize_str(Visitor)
            }
        }
    };
}

name!(
    PluginId,
    "Logical plugin/scientific name, not authenticated ownership.",
    |s: &str| { s.len() <= 128 && s.split('.').count() <= 8 && s.split('.').all(segment) }
);
name!(
    LocalContributionId,
    "Release-local contribution identity (also schema/slot names).",
    |s: &str| s.len() <= 64 && segment(s)
);
name!(
    ExtensionPointId,
    "Platform extension slot. Unknown well-formed slots remain opaque.",
    |s: &str| {
        s.len() <= 160
            && s.split_once('/').is_some_and(|(name, version)| {
                PluginId::new(name).is_ok()
                    && version.strip_prefix('v').is_some_and(|v| {
                        !v.is_empty()
                            && !v.starts_with('0')
                            && v.bytes().all(|b| b.is_ascii_digit())
                            && v.parse::<u32>().is_ok()
                    })
            })
    }
);

macro_rules! digest {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(ArtifactDigest);
        impl $name {
            pub(crate) fn of_canonical(bytes: &[u8]) -> Self {
                Self(ArtifactDigest::sha256_of(bytes))
            }
            /// Exact SHA-256 bytes.
            pub fn as_bytes(&self) -> &[u8] {
                self.0.as_bytes()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                if s.len() != 71 {
                    return Err(Error::malformed(
                        stringify!($name),
                        "expected sha256 digest",
                    ));
                }
                s.parse()
                    .map(Self)
                    .map_err(|_| Error::malformed(stringify!($name), "invalid sha256 digest"))
            }
        }
    };
}
digest!(
    PluginReleaseId,
    "Digest of canonical root bytes, not an artifact or mutable label."
);
digest!(
    ScientificDigest,
    "Digest of a scientific projection, excluding presentation and provider release."
);

/// Exact scientific identity: all three fields participate in matching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScientificContractRef {
    /// Scientific name, independent of provider identity.
    pub name: PluginId,
    /// Nonzero scientific version.
    pub version: std::num::NonZeroU32,
    /// Scientific projection digest.
    pub digest: ScientificDigest,
}

/// Exact provider-qualified contribution pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContributionRef {
    /// Immutable provider release.
    pub release: PluginReleaseId,
    /// Platform slot.
    pub extension_point: ExtensionPointId,
    /// Local identity within the release.
    pub local_id: LocalContributionId,
}

/// One independently compiled scientific execution contract per kernel artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionContractId {
    /// Field initialization, evolution/force production and sampling.
    #[serde(rename = "orishu:simulation/field@1")]
    Field,
    /// Dynamics integration and numerical history lifecycle.
    #[serde(rename = "orishu:simulation/dynamics@1")]
    Dynamics,
}

/// The six understood slots. Unknown slots are not rejected as corrupt releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownPoint {
    /// Entity component vocabulary.
    Components,
    /// Field family vocabulary.
    Fields,
    /// Observable channel vocabulary.
    Observables,
    /// Exported dimensioned constants.
    Constants,
    /// Field update kernels.
    FieldModels,
    /// Dynamics integrators.
    Integrators,
}
impl KnownPoint {
    /// Stable extension point spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Components => "orishu.model.components/v1",
            Self::Fields => "orishu.model.fields/v1",
            Self::Observables => "orishu.model.observables/v1",
            Self::Constants => "orishu.model.constants/v1",
            Self::FieldModels => "orishu.compute.field-models/v1",
            Self::Integrators => "orishu.compute.integrators/v1",
        }
    }
    /// Recognize an exact version; other versions remain unsupported/opaque.
    pub fn from_id(id: &ExtensionPointId) -> Option<Self> {
        [
            Self::Components,
            Self::Fields,
            Self::Observables,
            Self::Constants,
            Self::FieldModels,
            Self::Integrators,
        ]
        .into_iter()
        .find(|point| point.as_str() == id.as_str())
    }
}
