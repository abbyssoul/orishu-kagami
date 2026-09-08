//! Reading a workload a human wrote.
//!
//! JSON and YAML are authoring inputs, never identity. They parse into the
//! typed model, and [`crate::canonical`] decides what the resulting workload
//! *is*. Two consequences follow, and both are asserted by tests:
//!
//! - Whitespace, key order, comments, and the choice of JSON or YAML do not
//!   reach the digest. The same workload authored two ways is one workload.
//! - A key the schema does not recognise is an error rather than a dropped
//!   field, because a dropped field would leave the author believing it did
//!   something the digest says it did not.
//!
//! # Bounds come first
//!
//! [`Limits::max_manifest_bytes`] is checked against the input length *before*
//! parsing, so an oversized document is refused without being deserialized into
//! a tree of allocations. That ordering is the point of the check; moving it
//! after the parse would make it decorative.
//!
//! The same is true one level down. Every collection in the model is read
//! through a bounded seed that carries the caller's [`Limits`] into the
//! document, so a list or map stops at the point where accepting its next entry
//! *would* exceed its bound, and that entry is refused without being
//! deserialized. A hostile document therefore cannot make a reader build a
//! collection in order to be told it is too long. The machinery, and the two
//! things that remain bounded only by the input length, are described in
//! `authoring/seed.rs`.
//!
//! # This is the boundary hostile input crosses
//!
//! The model's types also have ordinary derived `Deserialize` implementations,
//! which is what makes a manifest usable with any serde format — and what a
//! caller building one itself, or reading one it produced, should use. They are
//! *not* bounded by [`Limits`], because a `Deserialize` impl cannot see a
//! runtime value. A document from a client, a peer, a plugin, or a file must
//! therefore enter through [`parse_str`], [`parse_bytes`], or [`from_reader`];
//! calling `serde_json::from_str::<WorkloadManifest>` on untrusted bytes gets
//! the schema without the bounds.

use crate::limits::Limits;
use crate::manifest::WorkloadManifest;

mod seed;

/// A collection in an authored document that exceeded its bound.
///
/// Structured rather than only rendered, because the caller most likely to act
/// on this is a submission client deciding what to tell an author, and reading
/// the limit back out of an English sentence is how that becomes brittle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionLimit {
    /// Which collection, named as the corresponding count.
    ///
    /// The collection rather than the [`Limits`] field, because several
    /// collections may share one bound and an author needs to know which part
    /// of their document to shorten.
    pub collection: &'static str,
    /// The bound that applies, from the caller's [`Limits`].
    pub limit: usize,
    /// How many entries the document asked for, when that is known.
    ///
    /// A format that declares a collection's length up front supplies the real
    /// figure. Otherwise reading stops one entry past the bound — which is all
    /// that was learned, so `limit + 1` is what is reported rather than a total
    /// nobody counted.
    pub found: Option<usize>,
}

impl std::fmt::Display for CollectionLimit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.found {
            Some(found) => write!(
                formatter,
                "{} is at least {found}, over the limit of {}",
                self.collection, self.limit
            ),
            None => write!(
                formatter,
                "{} is over the limit of {}",
                self.collection, self.limit
            ),
        }
    }
}

/// Why an authored workload could not be read.
#[derive(Debug, thiserror::Error)]
pub enum AuthoringError {
    /// The document is larger than the reader accepts.
    ///
    /// Reported before parsing, so the bytes were never deserialized.
    #[error("the manifest is {found} bytes, over the {limit}-byte limit")]
    TooLarge {
        /// Length supplied.
        found: u64,
        /// Length permitted.
        limit: u64,
    },
    /// A collection is longer than the reader accepts.
    ///
    /// Reported instead of [`Self::Malformed`] when reading stopped because a
    /// bound was reached, so the reason survives as a value rather than only as
    /// a sentence inside a codec error. The rejected entry was not
    /// deserialized.
    #[error("the manifest declares too much: {0}")]
    CollectionTooLarge(CollectionLimit),
    /// The bytes are not valid UTF-8.
    #[error("the manifest is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    /// The input could not be read.
    #[error("the manifest could not be read: {0}")]
    Io(#[from] std::io::Error),
    /// The document is not well-formed JSON or YAML, or does not match the
    /// schema.
    ///
    /// One variant for both codecs on purpose: every document is offered to
    /// YAML, which is a superset of JSON, so a failure is a failure to be a
    /// workload rather than a failure to be one syntax in particular.
    #[error("the manifest is not a well-formed workload document: {0}")]
    Malformed(#[from] serde_yaml::Error),
    /// The document parsed but is not a workload of the version this crate
    /// reads.
    #[error("{0}")]
    Discriminator(#[from] crate::manifest::UnexpectedDiscriminator),
}

/// Parses an authored workload from a string.
///
/// Checks the discriminator, so what is returned is a workload of this crate's
/// version. It does *not* validate the closure — see
/// [`crate::closure::validate_closure`] — because reading a manifest and having
/// its artifacts are separate things a caller does at different times.
///
/// Every collection is bounded as it is read; see the module documentation.
///
/// # Errors
///
/// Returns [`AuthoringError`] when the input is oversized, declares a
/// collection over its bound, is malformed, or is not a workload of the
/// supported version.
pub fn parse_str(input: &str, limits: &Limits) -> Result<WorkloadManifest, AuthoringError> {
    let found = input.len() as u64;
    if found > limits.max_manifest_bytes {
        return Err(AuthoringError::TooLarge {
            found,
            limit: limits.max_manifest_bytes,
        });
    }
    // YAML is a superset of JSON, so one pass reads both. Trying JSON first
    // would only change which error message a malformed document produces.
    //
    // `serde_yaml::from_str` is exactly `T::deserialize(Deserializer::from_str)`,
    // so driving the same deserializer with a seed changes what is built, not
    // how the document is read: multi-document input, anchors, and the codec's
    // own nesting bound behave identically.
    let bounded = seed::Bounded::new(limits);
    let manifest = match serde::de::DeserializeSeed::deserialize(
        seed::Manifest(&bounded),
        serde_yaml::Deserializer::from_str(input),
    ) {
        Ok(manifest) => manifest,
        // A bound the document exceeded is reported as itself. The codec error
        // carries the same sentence plus a position, but only as text.
        Err(error) => {
            return Err(match bounded.into_violation() {
                Some(violation) => AuthoringError::CollectionTooLarge(violation),
                None => AuthoringError::Malformed(error),
            });
        }
    };
    manifest.expect(&crate::manifest::api_version(), &crate::manifest::kind())?;
    Ok(manifest)
}

/// Parses an authored workload from bytes.
///
/// # Errors
///
/// Returns [`AuthoringError`] when the input is oversized, not UTF-8, declares
/// a collection over its bound, is malformed, or is not a workload of the
/// supported version.
pub fn parse_bytes(input: &[u8], limits: &Limits) -> Result<WorkloadManifest, AuthoringError> {
    let found = input.len() as u64;
    if found > limits.max_manifest_bytes {
        return Err(AuthoringError::TooLarge {
            found,
            limit: limits.max_manifest_bytes,
        });
    }
    parse_str(std::str::from_utf8(input)?, limits)
}

/// Parses an authored workload from a reader, reading at most
/// [`Limits::max_manifest_bytes`].
///
/// Unlike the prototype this model supersedes, the reader is bounded: it is
/// given one byte of headroom over the limit so an oversized stream is
/// *detected* rather than silently truncated into a manifest that parses.
///
/// # Errors
///
/// Returns [`AuthoringError`] when the stream is oversized, unreadable, not
/// UTF-8, declares a collection over its bound, is malformed, or is not a
/// workload of the supported version.
pub fn from_reader<R: std::io::Read>(
    reader: R,
    limits: &Limits,
) -> Result<WorkloadManifest, AuthoringError> {
    let mut buffer = Vec::new();
    let mut bounded = reader.take(limits.max_manifest_bytes.saturating_add(1));
    std::io::Read::read_to_end(&mut bounded, &mut buffer)?;
    parse_bytes(&buffer, limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::workload_digest;

    const MINIMAL: &str = r#"
apiVersion: orishu.dev/v2
kind: Workload
metadata:
  name: cavity
spec:
  compute:
    workloadGraphProfile: orishu.workload-graph/v1
    components:
      - instanceId: field
        artifact:
          role: component
          digest: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
          sizeBytes: 0
          mediaType: application/wasm
        pluginId: dev.orishu.em
        modelId: yee
        schemaId: em.field/v1
        engine: wasm-component
        lifecycle: orishu.component/v1
        stateOwnership: [e-field]
    channels:
      - channelId: e-field
        schema: {schemaId: em.field/v1, version: 1}
        owner: field
        reduction: single
    stepPlan:
      profile: orishu.workload-graph/v1
      invocations:
        - invocationId: advance-field
          instance: field
          phaseId: update
          outputs: [e-field]
  domain:
    dimensions: 3
    bounds: {shape: cube, sideMetres: 1.0}
    discretization: {spaceMetres: 0.001, timeSeconds: 1.5e-11}
"#;

    #[test]
    fn a_minimal_workload_parses() {
        let manifest = parse_str(MINIMAL, &Limits::DEFAULT).expect("it parses");
        assert_eq!(manifest.metadata.name.as_str(), "cavity");
        assert_eq!(manifest.spec.compute.components.len(), 1);
    }

    #[test]
    fn json_and_yaml_authorings_of_one_workload_have_one_identity() {
        let from_yaml = parse_str(MINIMAL, &Limits::DEFAULT).expect("YAML parses");
        // Re-emit as JSON and read it back: same workload, different syntax.
        let json = serde_json::to_string(&from_yaml).expect("it encodes");
        let from_json = parse_str(&json, &Limits::DEFAULT).expect("JSON parses");
        assert_eq!(
            workload_digest(&from_yaml, &Limits::DEFAULT).expect("digest"),
            workload_digest(&from_json, &Limits::DEFAULT).expect("digest"),
        );
    }

    #[test]
    fn whitespace_comments_and_key_order_do_not_reach_the_identity() {
        let reordered = r#"
# A comment, which is not part of any workload.
kind:       Workload
apiVersion: orishu.dev/v2

spec:
  domain:
    discretization: {timeSeconds: 1.5e-11, spaceMetres: 0.001}
    bounds: {shape: cube, sideMetres: 1.0}
    dimensions: 3
  compute:
    stepPlan:
      invocations:
        - phaseId: update
          instance: field
          outputs: [e-field]
          invocationId: advance-field
      profile: orishu.workload-graph/v1
    channels:
      - reduction: single
        owner: field
        schema: {version: 1, schemaId: em.field/v1}
        channelId: e-field
    components:
      - lifecycle: orishu.component/v1
        engine: wasm-component
        schemaId: em.field/v1
        modelId: yee
        pluginId: dev.orishu.em
        stateOwnership: [e-field]
        artifact:
          mediaType: application/wasm
          sizeBytes: 0
          digest: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
          role: component
        instanceId: field
    workloadGraphProfile: orishu.workload-graph/v1
metadata:
  name: cavity
"#;
        let original = parse_str(MINIMAL, &Limits::DEFAULT).expect("it parses");
        let shuffled = parse_str(reordered, &Limits::DEFAULT).expect("it parses");
        assert_eq!(
            workload_digest(&original, &Limits::DEFAULT).expect("digest"),
            workload_digest(&shuffled, &Limits::DEFAULT).expect("digest"),
            "authoring layout must not be part of workload identity"
        );
    }

    #[test]
    fn an_oversized_manifest_is_refused_before_it_is_parsed() {
        let tight = Limits {
            max_manifest_bytes: 16,
            ..Limits::DEFAULT
        };
        let error = parse_str(MINIMAL, &tight).unwrap_err();
        assert!(matches!(error, AuthoringError::TooLarge { limit: 16, .. }));
    }

    #[test]
    fn a_reader_is_bounded_rather_than_silently_truncated() {
        let tight = Limits {
            max_manifest_bytes: 32,
            ..Limits::DEFAULT
        };
        let error = from_reader(MINIMAL.as_bytes(), &tight).unwrap_err();
        // 33 bytes read from a 32-byte budget: detected as oversized, not
        // parsed as a truncated document.
        assert!(matches!(
            error,
            AuthoringError::TooLarge {
                found: 33,
                limit: 32
            }
        ));
    }

    #[test]
    fn the_superseded_prototype_version_is_not_read_as_this_format() {
        // There is no migration: the old unpinned schema is a different
        // discriminator and this parser refuses it by name rather than
        // silently accepting a manifest whose artifacts are not pinned.
        let legacy = MINIMAL.replace("orishu.dev/v2", "orishu.dev/v1");
        let error = parse_str(&legacy, &Limits::DEFAULT).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("orishu.dev/v2") && message.contains("orishu.dev/v1"),
            "the error should name both the expected and the found version, got: {message}"
        );
    }

    #[test]
    fn an_unrecognised_top_level_key_is_an_error_not_a_dropped_field() {
        let with_status = format!("{MINIMAL}\nstatus:\n  phase: Running\n");
        assert!(parse_str(&with_status, &Limits::DEFAULT).is_err());
    }

    #[test]
    fn a_misspelled_spec_key_is_reported_rather_than_ignored() {
        let typo = MINIMAL.replace("stateOwnership:", "stateOwnersihp:");
        let error = parse_str(&typo, &Limits::DEFAULT).unwrap_err();
        assert!(
            error.to_string().contains("stateOwnersihp"),
            "the error should name the misspelled key, got: {error}"
        );
    }

    #[test]
    fn a_non_utf8_document_is_refused() {
        let error = parse_bytes(&[0xff, 0xfe, 0xfd], &Limits::DEFAULT).unwrap_err();
        assert!(matches!(error, AuthoringError::InvalidUtf8(_)));
    }
}
