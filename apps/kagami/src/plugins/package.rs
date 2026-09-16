use super::{Code, Error, files::Directory};
use orishu_plugin::{
    bundle::{self, BundleLimits},
    resolution::VerifiedRelease,
    *,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Verified owned local package. Verification is declarative, never executable.
#[derive(Debug)]
pub struct Package {
    pub(super) release: VerifiedRelease,
    pub(super) blobs: BTreeMap<ArtifactDigest, Vec<u8>>,
}
impl Package {
    /// Validate a bounded stored ZIP and own its declared blobs.
    pub fn from_bundle(bytes: &[u8]) -> Result<Self, Error> {
        let bundle = bundle::read(bytes, &Limits::default(), BundleLimits::default())?;
        let (release, blobs) = bundle.into_parts();
        Ok(Self {
            release,
            blobs: blobs.into_iter().map(|(id, b)| (id, b.to_vec())).collect(),
        })
    }
    /// Load an explicit source directory or bundle. Source files are never
    /// interpreted as shell/build commands; plugins are built outside Kagami.
    pub fn load(path: &Path) -> Result<Self, Error> {
        if path.is_dir() {
            Self::source(path)
        } else {
            Self::from_bundle(&read_file(path, BundleLimits::default().max_bytes)?)
        }
    }
    /// Verified immutable declarations.
    pub fn release(&self) -> &VerifiedRelease {
        &self.release
    }
    /// Deterministic stored ZIP, excluding the source's build files.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let blobs = self.blobs.iter().map(|(k, v)| (*k, v.as_slice())).collect();
        Ok(bundle::pack(
            self.release.root(),
            &blobs,
            &Limits::default(),
            BundleLimits::default(),
        )?)
    }
    /// Package to an explicit output path. This refuses an existing target;
    /// publishing a new bundle never silently overwrites another user's file.
    pub fn write_bundle(&self, path: &Path) -> Result<(), Error> {
        super::files::create_file(path, &self.pack()?)
    }
    fn source(path: &Path) -> Result<Self, Error> {
        let dir = Directory::open(path)?;
        let limits = Limits::default();
        let bytes = dir.read(Path::new("plugin.json"), limits.max_manifest_bytes)?;
        let source: Source = serde_json::from_slice(&bytes)
            .map_err(|_| Error::new(Code::Malformed, "invalid plugin source manifest"))?;
        Release::new(
            source.metadata.clone(),
            ReleaseSpec {
                contributions: vec![],
                artifacts: vec![],
            },
        )
        .validate(&limits)?;
        let mut blobs = BTreeMap::new();
        let mut descriptors = BTreeMap::new();
        let mut seen_paths = BTreeSet::new();
        let mut contributions = Vec::new();
        let mut total = 0u64;
        for artifact in source.artifacts.0 {
            check_path(&artifact.path, &mut seen_paths)?;
            if artifact.media_type.len() > limits.max_text_bytes {
                return Err(source_limit());
            }
            let remaining = limits
                .max_declared_bytes
                .checked_sub(total)
                .ok_or_else(source_limit)?;
            let bytes = dir.read(
                Path::new(&artifact.path),
                remaining.min(limits.max_artifact_bytes) as usize,
            )?;
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or_else(source_limit)?;
            let digest = ArtifactDigest::sha256_of(&bytes);
            let descriptor = Artifact {
                digest,
                size_bytes: bytes.len() as u64,
                media_type: artifact.media_type,
            };
            if let Some(previous) = descriptors.insert(digest, descriptor.clone())
                && previous != descriptor
            {
                return Err(Error::new(
                    Code::Malformed,
                    "one source artifact digest has conflicting descriptors",
                ));
            }
            blobs.insert(digest, bytes);
        }
        for contribution in source.contributions.0 {
            check_path(&contribution.path, &mut seen_paths)?;
            let remaining = limits
                .max_declared_bytes
                .checked_sub(total)
                .ok_or_else(source_limit)?;
            let known = KnownPoint::from_id(&contribution.extension_point);
            let input_limit = if known.is_some() {
                limits.max_payload_bytes as u64
            } else {
                limits.max_artifact_bytes
            };
            let source_bytes = dir.read(
                Path::new(&contribution.path),
                remaining.min(input_limit) as usize,
            )?;
            let (bytes, requirements) = if let Some(point) = known {
                let payload = payload_from_json(point, &source_bytes, &limits)?;
                let requirements = payload
                    .requirements()
                    .iter()
                    .map(|r| Requirement::Contract {
                        slot: r.slot.clone(),
                        contract: r.contract.clone(),
                    })
                    .collect();
                (payload.canonical_bytes(&limits)?, requirements)
            } else {
                (source_bytes, Vec::new())
            };
            total = total
                .checked_add(bytes.len() as u64)
                .filter(|n| *n <= limits.max_declared_bytes)
                .ok_or_else(source_limit)?;
            let digest = ArtifactDigest::sha256_of(&bytes);
            descriptors.entry(digest).or_insert(Artifact {
                digest,
                size_bytes: bytes.len() as u64,
                media_type: "application/cbor".into(),
            });
            blobs.insert(digest, bytes);
            contributions.push(Contribution {
                local_id: contribution.local_id,
                extension_point: contribution.extension_point,
                payload: digest,
                requirements,
                annotations: None,
            });
        }
        let root = Release::new(
            source.metadata,
            ReleaseSpec {
                contributions,
                artifacts: descriptors.into_values().collect(),
            },
        );
        let borrowed = blobs.iter().map(|(k, v)| (*k, v.as_slice())).collect();
        let release = VerifiedRelease::verify(root, &borrowed, &limits)?;
        Ok(Self { release, blobs })
    }
}
fn source_limit() -> Error {
    Error::new(
        Code::LimitExceeded,
        "plugin source aggregate byte budget exceeded",
    )
}
fn check_path(path: &str, seen: &mut BTreeSet<String>) -> Result<(), Error> {
    if path.len() > 4096
        || path.contains('\\')
        || path == "plugin.json"
        || !seen.insert(path.into())
    {
        return Err(Error::new(
            Code::Malformed,
            "invalid or duplicate source path",
        ));
    }
    Ok(())
}
pub(super) fn read_file(path: &Path, max: usize) -> Result<Vec<u8>, Error> {
    super::files::read_file(path, max)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Source {
    #[serde(rename = "apiVersion")]
    _version: SourceVersion,
    metadata: ReleaseMetadata,
    contributions: BoundedList<SourceContribution, 256>,
    artifacts: BoundedList<SourceArtifact, 4096>,
}
#[derive(Deserialize)]
enum SourceVersion {
    #[serde(rename = "orishu.plugin-source/v1")]
    V1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceContribution {
    local_id: LocalContributionId,
    extension_point: ExtensionPointId,
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceArtifact {
    path: String,
    media_type: String,
}

/// Bounded before reading the rejected element, not a post-allocation Vec check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(super) struct BoundedList<T, const N: usize>(pub Vec<T>);
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for BoundedList<T, N> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
        struct Refuse;
        impl<'de> serde::de::DeserializeSeed<'de> for Refuse {
            type Value = ();
            fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
                Err(serde::de::Error::custom("collection budget exceeded"))
            }
        }
        impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
            type Value = BoundedList<T, N>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded collection")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while values.len() < N {
                    match seq.next_element()? {
                        Some(v) => values.push(v),
                        None => return Ok(BoundedList(values)),
                    }
                }
                seq.next_element_seed(Refuse)?;
                Ok(BoundedList(values))
            }
        }
        d.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedList;

    #[test]
    fn limit_refuses_the_next_value_before_deserialization() {
        // The malformed suffix would produce a syntax error if deserialized.
        let error = serde_json::from_str::<BoundedList<u8, 1>>("[1,{broken]").unwrap_err();
        assert!(error.to_string().contains("collection budget exceeded"));
        assert_eq!(
            serde_json::from_str::<BoundedList<u8, 1>>("[1]").unwrap().0,
            vec![1]
        );
    }
}
