//! Deciding whether a manifest plus some candidate bytes is a complete,
//! self-consistent workload.
//!
//! # Two phases, in that order
//!
//! Validation is deliberately split, and the split is load-bearing rather than
//! organisational:
//!
//! 1. **A structural pass over the manifest alone.** Bounds, the domain, the
//!    component graph, and every claim the manifest makes about its own
//!    artifacts — role cardinality, contradictory descriptors, per-artifact and
//!    aggregate declared size. None of this needs a byte from anywhere.
//! 2. **Verification**, reached only if the first pass found nothing.
//!
//! A manifest that is over its artifact budget, names a channel that does not
//! exist, or declares one digest two contradictory ways therefore never reaches
//! the provider at all. Retrieval is work, and possibly network work; a
//! document has to earn it by being internally consistent first.
//!
//! # What "transport-neutral" means here
//!
//! The validator asks a [`BlobSource`] for bytes **by digest and nothing
//! else**, and re-hashes everything it is handed. It never reads a role, a
//! name, a length, a media type, or a location from the provider. A source that
//! lies cannot make a workload validate; the worst it can do is fail to supply
//! something.
//!
//! That is why this module has no notion of a cache, a peer, a file, or an
//! archive. Thin submission, a local directory import, a portable bundle, and a
//! peer fetch are different implementations of one trait facing identical
//! verification.
//!
//! # Nothing is held
//!
//! Verification streams: a candidate arrives in chunks through
//! [`BlobVerifier`], is hashed as it goes, and is never materialised here. A
//! [`VerifiedClosure`] records *that* each artifact was verified and what the
//! manifest said about it — not its bytes. This is what makes
//! [`Limits::max_artifact_bytes`]'s 64 GiB default honest; a validator that
//! kept what it verified could not offer that number.
//!
//! Callers that need the bytes already hold the source they supplied.
//!
//! # Every error, not the first
//!
//! [`validate_closure`] returns a [`ClosureReport`] carrying every problem it
//! found, following `kagami-catalog`'s diagnostics: a submitter fixing a
//! workload wants the list, not one item per run. The list is itself bounded by
//! [`Limits::max_reported_errors`], and a truncated report says so rather than
//! letting a reader believe it is exhaustive. No partial [`VerifiedClosure`] is
//! produced on failure.
//!
//! # What this checks, and what it does not
//!
//! Checked: artifact presence, digest, exact size, declared-byte budgets, role
//! cardinality, descriptor consistency, and the *structure* of the component
//! graph — that names resolve and agree, that ownership is declared
//! consistently from both sides, that authoritative state has exactly one
//! writer and that the writer owns it, that consumed channels have producers,
//! and that the step plan is a DAG within its bounds.
//!
//! Deliberately not checked, and owned by later work: expression resolution and
//! the variables language, physical dimension compatibility between channels,
//! numerical rules beyond [`crate::value::FiniteF64`], model-family
//! exclusivity, per-model policy, hardware-requirement matching, and placement
//! feasibility against a real cluster. A successful validation is a structural
//! statement, never a claim that the physics is admissible.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::artifact::{ArtifactDescriptor, ArtifactRole, SchemaCompat};
use crate::digest::ArtifactDigest;
use crate::domain::DomainError;
use crate::graph::{ComputeSpec, Reduction, StateChannel};
use crate::ids::{ComponentInstanceId, MediaType, StateChannelId, StepInvocationId};
use crate::limits::Limits;
use crate::manifest::WorkloadManifest;
use crate::value::ScalarValue;

// ── obtaining and verifying bytes ───────────────────────────────────────────

/// Accumulates a candidate artifact's bytes without holding them.
///
/// A [`BlobSource`] writes into this rather than returning a slice, so a blob
/// far larger than memory can still be verified. The hasher and the length
/// counter belong to the validator, so nothing a source does can make a
/// candidate verify except supplying the right bytes.
pub struct BlobVerifier<'a> {
    expected: &'a ArtifactDescriptor,
    hasher: Sha256,
    seen: u64,
    overrun: bool,
}

impl BlobVerifier<'_> {
    /// Feeds the next chunk of the candidate.
    ///
    /// Returns `false` once the candidate has already exceeded its declared
    /// size, at which point the source should stop reading. Continuing is not
    /// unsafe — the verifier has stopped accumulating and the artifact is
    /// already refused — it only wastes the source's own effort.
    pub fn write(&mut self, chunk: &[u8]) -> bool {
        if self.overrun {
            return false;
        }
        self.seen = self.seen.saturating_add(chunk.len() as u64);
        if self.seen > self.expected.size_bytes {
            self.overrun = true;
            return false;
        }
        self.hasher.update(chunk);
        true
    }

    /// How many bytes have been accepted so far.
    #[must_use]
    pub fn accepted(&self) -> u64 {
        self.seen
    }

    fn finish(self) -> Result<(), ClosureError> {
        let declared = self.expected.size_bytes;
        let digest = self.expected.digest;
        if self.overrun {
            return Err(ClosureError::SizeOverrun { digest, declared });
        }
        if self.seen != declared {
            return Err(ClosureError::SizeMismatch {
                digest,
                declared,
                actual: self.seen,
            });
        }
        let actual = ArtifactDigest::from_sha256(self.hasher.finalize().into());
        if actual != digest {
            return Err(ClosureError::DigestMismatch {
                declared: digest,
                actual,
            });
        }
        Ok(())
    }
}

/// Somewhere candidate bytes can be obtained, addressed only by digest.
///
/// Implement this over whatever holds bytes — an in-memory map, a verified
/// cache, an unpacked archive, a network fetch. The implementation is trusted
/// to produce *some* bytes and trusted for nothing else.
pub trait BlobSource {
    /// Streams the candidate bytes for `digest` into `verifier`.
    ///
    /// Returns `false` when this source holds nothing for that digest, which is
    /// reported as a missing artifact rather than as a failure. Returning
    /// `true` after writing the wrong bytes is not an error the implementation
    /// needs to detect: the verifier hashes what it receives.
    fn read_blob(&self, digest: &ArtifactDigest, verifier: &mut BlobVerifier<'_>) -> bool;
}

/// A blob source backed by an ordinary map.
///
/// Useful for tests, for a submitter assembling a closure in memory, and as the
/// reference implementation of "the source is trusted for nothing".
#[derive(Debug, Clone, Default)]
pub struct InMemoryBlobs {
    blobs: BTreeMap<ArtifactDigest, Vec<u8>>,
}

impl InMemoryBlobs {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `content`, keyed by its own digest.
    ///
    /// The caller cannot choose the key: it is computed from the bytes, so this
    /// source cannot be populated with a mislabelled blob even deliberately.
    pub fn insert(&mut self, content: impl Into<Vec<u8>>) -> ArtifactDigest {
        let content = content.into();
        let digest = ArtifactDigest::sha256_of(&content);
        self.blobs.insert(digest, content);
        digest
    }

    /// How many blobs are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    /// Whether the source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }
}

impl FromIterator<Vec<u8>> for InMemoryBlobs {
    fn from_iter<I: IntoIterator<Item = Vec<u8>>>(iter: I) -> Self {
        let mut blobs = Self::new();
        for content in iter {
            blobs.insert(content);
        }
        blobs
    }
}

impl BlobSource for InMemoryBlobs {
    fn read_blob(&self, digest: &ArtifactDigest, verifier: &mut BlobVerifier<'_>) -> bool {
        match self.blobs.get(digest) {
            Some(content) => {
                verifier.write(content);
                true
            }
            None => false,
        }
    }
}

// ── the result ──────────────────────────────────────────────────────────────

/// One artifact whose bytes were obtained and verified.
///
/// Holds what the manifest claimed about the blob, never the blob. `roles` is a
/// set because one blob may legitimately serve several purposes in one
/// workload; the byte-facts beside it are singular because two contradicting
/// claims about one digest cannot both be true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    /// Which bytes were verified.
    pub digest: ArtifactDigest,
    /// Their verified length.
    pub size_bytes: u64,
    /// Their declared format.
    pub media_type: MediaType,
    /// Their declared schema compatibility, where the role requires one.
    pub schema: Option<SchemaCompat>,
    /// Every role this blob serves in this workload.
    pub roles: BTreeSet<ArtifactRole>,
}

/// A manifest and its complete, verified closure.
///
/// Only constructible by [`validate_closure`] returning `Ok`, so holding one is
/// evidence that the manifest is structurally sound and that every artifact it
/// declares was supplied and hashed to what it claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClosure {
    root: crate::digest::WorkloadDigest,
    artifacts: BTreeMap<ArtifactDigest, VerifiedArtifact>,
}

impl VerifiedClosure {
    /// The root workload identity.
    #[must_use]
    pub fn root(&self) -> crate::digest::WorkloadDigest {
        self.root
    }

    /// The verified artifacts, keyed by digest.
    #[must_use]
    pub fn artifacts(&self) -> &BTreeMap<ArtifactDigest, VerifiedArtifact> {
        &self.artifacts
    }

    /// One verified artifact by digest.
    #[must_use]
    pub fn artifact(&self, digest: &ArtifactDigest) -> Option<&VerifiedArtifact> {
        self.artifacts.get(digest)
    }

    /// How many distinct blobs the closure contains.
    ///
    /// Identical content declared under two roles is one blob: the closure is
    /// keyed by digest, so content is counted once.
    #[must_use]
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// Whether the closure is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}

// ── what can be wrong ───────────────────────────────────────────────────────

/// One reason a workload is not admissible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClosureError {
    /// The manifest's discriminator is not the one this crate reads.
    #[error("{0}")]
    Discriminator(#[from] crate::manifest::UnexpectedDiscriminator),
    /// The manifest could not be canonically encoded, so it has no identity.
    #[error("the manifest has no canonical form: {0}")]
    NotCanonical(#[from] crate::canonical::CanonicalError),
    /// The domain is not internally consistent.
    #[error("the domain is invalid: {0}")]
    Domain(#[from] DomainError),
    /// A declared artifact was not supplied.
    #[error("no candidate bytes were supplied for the {role} artifact {digest}")]
    MissingArtifact {
        /// One role the manifest declared for it.
        role: ArtifactRole,
        /// The digest that was requested.
        digest: ArtifactDigest,
    },
    /// Supplied bytes do not hash to the declared digest.
    #[error("the bytes supplied for {declared} hash to {actual}")]
    DigestMismatch {
        /// What the manifest declared.
        declared: ArtifactDigest,
        /// What the supplied bytes actually hash to.
        actual: ArtifactDigest,
    },
    /// The source supplied fewer bytes than declared.
    #[error("artifact {digest} declares {declared} bytes but {actual} were supplied")]
    SizeMismatch {
        /// The artifact.
        digest: ArtifactDigest,
        /// Length declared.
        declared: u64,
        /// Length supplied.
        actual: u64,
    },
    /// The source supplied more bytes than declared.
    ///
    /// Separate from [`Self::SizeMismatch`] because verification stops at the
    /// declared length rather than reading on to find out how much more there
    /// was — so the actual size is genuinely unknown, and reporting a number
    /// would be inventing one.
    #[error("artifact {digest} declares {declared} bytes but the source supplied more")]
    SizeOverrun {
        /// The artifact.
        digest: ArtifactDigest,
        /// Length declared.
        declared: u64,
    },
    /// One artifact declares more bytes than the reader accepts.
    #[error("artifact {digest} declares {declared} bytes, over the {limit}-byte limit")]
    ArtifactTooLarge {
        /// The artifact.
        digest: ArtifactDigest,
        /// Length declared.
        declared: u64,
        /// Length permitted.
        limit: u64,
    },
    /// The closure declares more bytes in total than the reader accepts.
    #[error("the closure declares {declared} bytes in total, over the {limit}-byte limit")]
    ClosureTooLarge {
        /// Total length declared.
        declared: u64,
        /// Total length permitted.
        limit: u64,
    },
    /// The same digest is declared with two incompatible descriptions of its
    /// bytes.
    ///
    /// The *role* is deliberately not part of this: one blob may legitimately
    /// serve two purposes, and content-addressed storage holds it once either
    /// way. Size, media type, and schema are claims about the bytes themselves,
    /// and two contradicting claims cannot both be true.
    #[error("artifact {digest} is declared with two different descriptions of its bytes")]
    ConflictingDescriptor {
        /// The artifact declared twice.
        digest: ArtifactDigest,
    },
    /// A role that may appear once appears more than once.
    #[error("the {role} role may appear at most once, but appears {count} times")]
    DuplicateRole {
        /// The repeated role.
        role: ArtifactRole,
        /// How many times it appeared.
        count: usize,
    },
    /// A component instance names an artifact that is not component code.
    #[error(
        "component instance `{instance}` names an artifact with role `{role}`, not `component`"
    )]
    NotAComponentArtifact {
        /// The offending instance.
        instance: ComponentInstanceId,
        /// The role its artifact declared.
        role: ArtifactRole,
    },
    /// Two component instances share an id.
    #[error("component instance id `{instance}` is declared more than once")]
    DuplicateInstance {
        /// The repeated id.
        instance: ComponentInstanceId,
    },
    /// Two channels share an id.
    #[error("channel id `{channel}` is declared more than once")]
    DuplicateChannel {
        /// The repeated id.
        channel: StateChannelId,
    },
    /// Two step-plan nodes share an id.
    #[error("step invocation id `{invocation}` is declared more than once")]
    DuplicateInvocation {
        /// The repeated id.
        invocation: StepInvocationId,
    },
    /// Something names a component instance that does not exist.
    #[error("`{referrer}` names component instance `{instance}`, which is not declared")]
    UnknownInstance {
        /// What made the reference.
        referrer: String,
        /// The name that did not resolve.
        instance: ComponentInstanceId,
    },
    /// Something names a channel that does not exist.
    #[error("`{referrer}` names channel `{channel}`, which is not declared")]
    UnknownChannel {
        /// What made the reference.
        referrer: String,
        /// The name that did not resolve.
        channel: StateChannelId,
    },
    /// A step-plan node depends on a node that does not exist.
    #[error("invocation `{invocation}` depends on `{dependency}`, which is not declared")]
    UnknownDependency {
        /// The dependent node.
        invocation: StepInvocationId,
        /// The name that did not resolve.
        dependency: StepInvocationId,
    },
    /// The compute definition and its step plan name different graph profiles.
    ///
    /// The profile decides how the graph and the plan are read. Two answers
    /// would leave a worker free to interpret one half under each.
    #[error(
        "the compute definition declares graph profile `{compute}` but its step plan declares `{plan}`"
    )]
    GraphProfileMismatch {
        /// The profile on the compute definition.
        compute: crate::ids::GraphProfile,
        /// The profile on the step plan.
        plan: crate::ids::GraphProfile,
    },
    /// The two declarations of channel ownership disagree.
    ///
    /// A channel names its owner and an instance lists what it owns. Both are
    /// in the schema, so both must say the same thing; a workload where they
    /// differ has no single answer to "who owns this state".
    #[error(
        "ownership of channel `{channel}` disagrees: {claimants} claim it, and the channel names {owner}"
    )]
    OwnershipDisagreement {
        /// The disputed channel.
        channel: StateChannelId,
        /// The instances listing it under `stateOwnership`.
        claimants: String,
        /// What the channel's own `owner` says.
        owner: String,
    },
    /// An owned channel declares a combining reduction.
    ///
    /// Authoritative state has one writer, so there is nothing to combine.
    /// `sum` on owned state would describe contributions accumulating into a
    /// value that by definition only one instance may write.
    #[error(
        "channel `{channel}` is owned by `{owner}` but declares reduction `{reduction}`; owned state must be `single`"
    )]
    OwnedChannelNotSingle {
        /// The channel.
        channel: StateChannelId,
        /// Its owner.
        owner: ComponentInstanceId,
        /// The reduction it declared.
        reduction: &'static str,
    },
    /// Owned state is never written by the plan.
    ///
    /// An owner that never writes leaves its state at the initial boundary
    /// forever, which is a plan that does not advance rather than one that
    /// advances slowly.
    #[error("channel `{channel}` is owned by `{owner}` but no invocation produces it")]
    OwnedStateWithoutWriter {
        /// The unwritten channel.
        channel: StateChannelId,
        /// Its owner.
        owner: ComponentInstanceId,
    },
    /// A channel is read but never written.
    #[error("channel `{channel}` is consumed by `{consumer}` but no invocation produces it")]
    ChannelWithoutProducer {
        /// The unproduced channel.
        channel: StateChannelId,
        /// One invocation that consumes it.
        consumer: StepInvocationId,
    },
    /// A channel admitting one writer has several.
    #[error("channel `{channel}` admits a single writer but is produced by {count} invocations")]
    MultipleWriters {
        /// The over-written channel.
        channel: StateChannelId,
        /// How many invocations produce it.
        count: usize,
    },
    /// An invocation writes a state channel its instance does not own.
    #[error(
        "invocation `{invocation}` on instance `{instance}` writes channel `{channel}`, which is owned by `{owner}`"
    )]
    WriterIsNotOwner {
        /// The offending node.
        invocation: StepInvocationId,
        /// The instance it runs.
        instance: ComponentInstanceId,
        /// The channel it writes.
        channel: StateChannelId,
        /// The instance that actually owns the channel.
        owner: ComponentInstanceId,
    },
    /// The step plan contains a cycle.
    #[error("the step plan is cyclic; {count} invocations are in or behind a cycle")]
    CyclicStepPlan {
        /// How many nodes could never become ready.
        count: usize,
    },
    /// A declared count exceeds its bound.
    #[error("{what} is {found}, over the limit of {limit}")]
    LimitExceeded {
        /// Which count.
        what: &'static str,
        /// The count found.
        found: usize,
        /// The count permitted.
        limit: usize,
    },
    /// A text value is longer than the reader accepts.
    #[error("the text at {at} is {found} bytes, over the {limit}-byte limit")]
    TextTooLong {
        /// Where the value sits, as a readable path.
        at: String,
        /// Length found.
        found: usize,
        /// Length permitted.
        limit: usize,
    },
}

/// Everything wrong with a workload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureReport {
    errors: Vec<ClosureError>,
    withheld: usize,
}

impl ClosureReport {
    /// The problems found, in the order they were detected.
    #[must_use]
    pub fn errors(&self) -> &[ClosureError] {
        &self.errors
    }

    /// How many problems are listed.
    ///
    /// Not necessarily how many exist — see [`Self::withheld`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Whether the report is empty. Never true for a report a caller receives.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// How many further problems were found but not listed.
    ///
    /// Non-zero when the workload had more problems than
    /// [`Limits::max_reported_errors`]. A caller rendering this report should
    /// say so rather than implying the list is exhaustive.
    #[must_use]
    pub fn withheld(&self) -> usize {
        self.withheld
    }
}

impl std::fmt::Display for ClosureReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the workload is not admissible:")?;
        for error in &self.errors {
            write!(formatter, "\n  - {error}")?;
        }
        if self.withheld > 0 {
            write!(
                formatter,
                "\n  ... and {} further problems, not listed",
                self.withheld
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ClosureReport {}

/// Collects problems up to a bound.
///
/// A bounded manifest can still be wrong in proportion to its size — one error
/// per invocation, say — so the report needs its own bound, separate from the
/// bounds on the document.
struct Errors {
    found: Vec<ClosureError>,
    limit: usize,
    withheld: usize,
}

impl Errors {
    fn new(limits: &Limits) -> Self {
        Self {
            found: Vec::new(),
            limit: limits.max_reported_errors,
            withheld: 0,
        }
    }

    fn push(&mut self, error: ClosureError) {
        if self.found.len() < self.limit {
            self.found.push(error);
        } else {
            self.withheld += 1;
        }
    }

    fn is_empty(&self) -> bool {
        // `withheld` can only be non-zero once `found` is full, so this is
        // complete despite reading only one field.
        self.found.is_empty()
    }

    fn into_report(self) -> ClosureReport {
        ClosureReport {
            errors: self.found,
            withheld: self.withheld,
        }
    }
}

// ── validation ──────────────────────────────────────────────────────────────

/// Validates a manifest and its candidate bytes.
///
/// Returns a [`VerifiedClosure`] only when the manifest is structurally sound
/// *and* every artifact it declares was supplied and verified. The structural
/// pass runs first and completely: a manifest that fails it never reaches
/// `blobs`.
///
/// # Errors
///
/// Returns a [`ClosureReport`] listing every problem found, bounded by
/// [`Limits::max_reported_errors`]. The report is never empty and no partial
/// closure accompanies it.
pub fn validate_closure(
    manifest: &WorkloadManifest,
    blobs: &impl BlobSource,
    limits: &Limits,
) -> Result<VerifiedClosure, ClosureReport> {
    let mut errors = Errors::new(limits);

    if let Err(error) = manifest.expect(&crate::manifest::api_version(), &crate::manifest::kind()) {
        // Nothing below is meaningful for a document that is not a workload of
        // this version, so this is the one check that stops immediately.
        errors.push(error.into());
        return Err(errors.into_report());
    }

    if let Err(error) = manifest.spec.domain.validate(limits) {
        errors.push(error.into());
    }
    check_scalar_bounds(manifest, limits, &mut errors);
    validate_graph(&manifest.spec.compute, limits, &mut errors);
    let declared = declared_artifacts(manifest, limits, &mut errors);

    let root = match crate::canonical::workload_digest(manifest, limits) {
        Ok(root) => Some(root),
        Err(error) => {
            errors.push(error.into());
            None
        }
    };

    // The gate. Everything above reads the manifest and nothing else; only a
    // manifest that is consistent with itself is worth fetching bytes for.
    if !errors.is_empty() {
        return Err(errors.into_report());
    }
    let root = root.expect("a manifest with no structural errors has a canonical form");

    let artifacts = verify_artifacts(&declared, blobs, &mut errors);
    if errors.is_empty() {
        Ok(VerifiedClosure { root, artifacts })
    } else {
        Err(errors.into_report())
    }
}

// ── phase one: the manifest alone ───────────────────────────────────────────

/// One distinct blob the manifest declares, with everything it claims about it.
struct DeclaredArtifact {
    digest: ArtifactDigest,
    size_bytes: u64,
    media_type: MediaType,
    schema: Option<SchemaCompat>,
    roles: BTreeSet<ArtifactRole>,
    /// Kept so verification can hand `BlobVerifier` the descriptor a source is
    /// being measured against, rather than reassembling one.
    descriptor: ArtifactDescriptor,
}

/// Every descriptor a manifest declares, in a stable order.
fn every_descriptor(manifest: &WorkloadManifest) -> impl Iterator<Item = &ArtifactDescriptor> {
    manifest
        .spec
        .compute
        .components
        .iter()
        .map(|component| &component.artifact)
        .chain(manifest.spec.inputs.geometry.iter())
        .chain(manifest.spec.inputs.initial_conditions.iter())
        .chain(manifest.spec.inputs.additional.iter())
}

/// Reduces the manifest's descriptors to the distinct blobs it needs, reporting
/// every way the declarations can be wrong on their own.
///
/// Pure: it reads the manifest and nothing else. Conflicting descriptors,
/// cardinality, and both byte budgets are decided here rather than during
/// verification, so a contradiction is reported even when the blob it concerns
/// is missing.
fn declared_artifacts(
    manifest: &WorkloadManifest,
    limits: &Limits,
    errors: &mut Errors,
) -> Vec<DeclaredArtifact> {
    let descriptors: Vec<&ArtifactDescriptor> = every_descriptor(manifest).collect();

    check_limit(
        "the artifact count",
        descriptors.len(),
        limits.max_artifacts,
        errors,
    );
    check_limit(
        "the initial-condition count",
        manifest.spec.inputs.initial_conditions.len(),
        limits.max_initial_conditions,
        errors,
    );

    // Role cardinality is counted over declarations, not over distinct blobs:
    // declaring one geometry twice is a manifest mistake even when both
    // declarations name identical bytes.
    let mut role_counts: BTreeMap<&ArtifactRole, usize> = BTreeMap::new();
    for descriptor in &descriptors {
        *role_counts.entry(&descriptor.role).or_default() += 1;
    }
    for (role, count) in role_counts {
        if role.is_singular() && count > 1 {
            errors.push(ClosureError::DuplicateRole {
                role: role.clone(),
                count,
            });
        }
    }

    // Collapse to distinct blobs, checking that repeated declarations of one
    // digest agree about the bytes.
    let mut by_digest: BTreeMap<ArtifactDigest, DeclaredArtifact> = BTreeMap::new();
    let mut conflicted: BTreeSet<ArtifactDigest> = BTreeSet::new();
    for descriptor in &descriptors {
        match by_digest.get_mut(&descriptor.digest) {
            Some(existing) => {
                if existing.size_bytes != descriptor.size_bytes
                    || existing.media_type != descriptor.media_type
                    || existing.schema != descriptor.schema
                {
                    // Once per digest, however many times it is repeated.
                    if conflicted.insert(descriptor.digest) {
                        errors.push(ClosureError::ConflictingDescriptor {
                            digest: descriptor.digest,
                        });
                    }
                } else {
                    existing.roles.insert(descriptor.role.clone());
                }
            }
            None => {
                by_digest.insert(
                    descriptor.digest,
                    DeclaredArtifact {
                        digest: descriptor.digest,
                        size_bytes: descriptor.size_bytes,
                        media_type: descriptor.media_type.clone(),
                        schema: descriptor.schema.clone(),
                        roles: BTreeSet::from([descriptor.role.clone()]),
                        descriptor: (*descriptor).clone(),
                    },
                );
            }
        }
    }

    // Byte budgets, over distinct blobs: identical content is transferred and
    // stored once, so it counts once.
    let mut aggregate: u64 = 0;
    for artifact in by_digest.values() {
        if artifact.size_bytes > limits.max_artifact_bytes {
            errors.push(ClosureError::ArtifactTooLarge {
                digest: artifact.digest,
                declared: artifact.size_bytes,
                limit: limits.max_artifact_bytes,
            });
        }
        aggregate = aggregate.saturating_add(artifact.size_bytes);
    }
    if aggregate > limits.max_aggregate_declared_bytes {
        errors.push(ClosureError::ClosureTooLarge {
            declared: aggregate,
            limit: limits.max_aggregate_declared_bytes,
        });
    }

    by_digest.into_values().collect()
}

/// Bounds the declared parameter maps a manifest carries, and the text in them.
///
/// The sizes are checked here rather than in [`validate_graph`] because these
/// maps are what they are wherever they sit: a bounded map of already-resolved
/// scalars, governed by [`Limits::max_parameter_entries`]. Component
/// configuration is the exception and keeps its own, larger bound, applied with
/// the rest of the graph.
///
/// Names are bounded by their own types, which check length before allocating.
/// What is left is [`ScalarValue::Text`], which any of these maps may hold and
/// which nothing else constrains.
fn check_scalar_bounds(manifest: &WorkloadManifest, limits: &Limits, errors: &mut Errors) {
    check_limit(
        "the label count",
        manifest.metadata.labels.len(),
        limits.max_labels,
        errors,
    );
    if let Some(integration) = &manifest.spec.domain.discretization.integration {
        check_limit(
            "the integration parameter count",
            integration.parameters.len(),
            limits.max_parameter_entries,
            errors,
        );
    }
    check_limit(
        "the hardware-requirement count",
        manifest.spec.requirements.hardware.len(),
        limits.max_parameter_entries,
        errors,
    );
    check_limit(
        "the execution-profile count",
        manifest.spec.requirements.execution_profile.len(),
        limits.max_parameter_entries,
        errors,
    );

    let mut check = |at: String, value: &ScalarValue| {
        if let ScalarValue::Text(text) = value
            && text.len() > limits.max_text_bytes
        {
            errors.push(ClosureError::TextTooLong {
                at,
                found: text.len(),
                limit: limits.max_text_bytes,
            });
        }
    };

    for component in &manifest.spec.compute.components {
        for (name, value) in &component.config {
            check(
                format!("component `{}` config `{name}`", component.instance_id),
                value,
            );
        }
    }
    for constraint in &manifest.spec.compute.placement_constraints {
        for (name, value) in &constraint.parameters {
            check(
                format!(
                    "placement constraint `{}` parameter `{name}`",
                    constraint.constraint
                ),
                value,
            );
        }
    }
    if let Some(integration) = &manifest.spec.domain.discretization.integration {
        for (name, value) in &integration.parameters {
            check(format!("integration parameter `{name}`"), value);
        }
    }
    for (name, value) in &manifest.spec.requirements.hardware {
        check(format!("hardware requirement `{name}`"), value);
    }
    for (name, value) in &manifest.spec.requirements.execution_profile {
        check(format!("execution profile `{name}`"), value);
    }
}

fn check_limit(what: &'static str, found: usize, limit: usize, errors: &mut Errors) {
    if found > limit {
        errors.push(ClosureError::LimitExceeded { what, found, limit });
    }
}

fn validate_graph(compute: &ComputeSpec, limits: &Limits, errors: &mut Errors) {
    check_limit(
        "the component count",
        compute.components.len(),
        limits.max_components,
        errors,
    );
    check_limit(
        "the channel count",
        compute.channels.len(),
        limits.max_channels,
        errors,
    );
    check_limit(
        "the step-invocation count",
        compute.step_plan.invocations.len(),
        limits.max_step_invocations,
        errors,
    );
    check_limit(
        "the placement-constraint count",
        compute.placement_constraints.len(),
        limits.max_placement_constraints,
        errors,
    );

    if compute.workload_graph_profile != compute.step_plan.profile {
        errors.push(ClosureError::GraphProfileMismatch {
            compute: compute.workload_graph_profile.clone(),
            plan: compute.step_plan.profile.clone(),
        });
    }

    // ── names must be unique and must resolve ──────────────────────────────
    let mut instances: BTreeSet<&ComponentInstanceId> = BTreeSet::new();
    for component in &compute.components {
        if !instances.insert(&component.instance_id) {
            errors.push(ClosureError::DuplicateInstance {
                instance: component.instance_id.clone(),
            });
        }
        if component.artifact.role.as_str() != ArtifactRole::COMPONENT {
            errors.push(ClosureError::NotAComponentArtifact {
                instance: component.instance_id.clone(),
                role: component.artifact.role.clone(),
            });
        }
        check_limit(
            "a component's role count",
            component.roles.len(),
            limits.max_roles_per_component,
            errors,
        );
        check_limit(
            "a component's owned-channel count",
            component.state_ownership.len(),
            limits.max_state_ownership_per_component,
            errors,
        );
        check_limit(
            "a component's configuration size",
            component.config.len(),
            limits.max_config_entries,
            errors,
        );
        check_limit(
            "a component's limit count",
            component.limits.len(),
            limits.max_limit_entries,
            errors,
        );
    }

    let mut channels: BTreeMap<&StateChannelId, &StateChannel> = BTreeMap::new();
    for channel in &compute.channels {
        check_limit(
            "a channel's shape rank",
            channel.shape.len(),
            limits.max_channel_shape_rank,
            errors,
        );
        if channels.insert(&channel.channel_id, channel).is_some() {
            errors.push(ClosureError::DuplicateChannel {
                channel: channel.channel_id.clone(),
            });
        }
        if let Some(owner) = &channel.owner
            && !instances.contains(owner)
        {
            errors.push(ClosureError::UnknownInstance {
                referrer: format!("channel {}", channel.channel_id),
                instance: owner.clone(),
            });
        }
    }

    for component in &compute.components {
        for owned in &component.state_ownership {
            if !channels.contains_key(owned) {
                errors.push(ClosureError::UnknownChannel {
                    referrer: format!("component instance {}", component.instance_id),
                    channel: owned.clone(),
                });
            }
        }
    }

    for constraint in &compute.placement_constraints {
        // A constraint names instances of the graph it constrains, so the
        // graph's own bound is the one that applies.
        check_limit(
            "a placement constraint's instance count",
            constraint.instances.len(),
            limits.max_components,
            errors,
        );
        check_limit(
            "a placement constraint's parameter count",
            constraint.parameters.len(),
            limits.max_parameter_entries,
            errors,
        );
        for instance in &constraint.instances {
            if !instances.contains(instance) {
                errors.push(ClosureError::UnknownInstance {
                    referrer: format!("placement constraint {}", constraint.constraint),
                    instance: instance.clone(),
                });
            }
        }
    }

    reconcile_ownership(compute, &channels, errors);

    // ── the step plan ──────────────────────────────────────────────────────
    let mut invocations: BTreeSet<&StepInvocationId> = BTreeSet::new();
    for invocation in &compute.step_plan.invocations {
        if !invocations.insert(&invocation.invocation_id) {
            errors.push(ClosureError::DuplicateInvocation {
                invocation: invocation.invocation_id.clone(),
            });
        }
    }

    // Which invocations write which channel, for the producer and writer rules.
    let mut producers: BTreeMap<&StateChannelId, Vec<&crate::graph::StepInvocation>> =
        BTreeMap::new();

    for invocation in &compute.step_plan.invocations {
        let referrer = format!("invocation {}", invocation.invocation_id);
        check_limit(
            "an invocation's input count",
            invocation.inputs.len(),
            limits.max_inputs_per_invocation,
            errors,
        );
        check_limit(
            "an invocation's output count",
            invocation.outputs.len(),
            limits.max_outputs_per_invocation,
            errors,
        );
        check_limit(
            "an invocation's dependency count",
            invocation.depends_on.len(),
            limits.max_dependencies_per_invocation,
            errors,
        );

        if !instances.contains(&invocation.instance) {
            errors.push(ClosureError::UnknownInstance {
                referrer: referrer.clone(),
                instance: invocation.instance.clone(),
            });
        }
        for channel in invocation.inputs.iter().chain(&invocation.outputs) {
            if !channels.contains_key(channel) {
                errors.push(ClosureError::UnknownChannel {
                    referrer: referrer.clone(),
                    channel: channel.clone(),
                });
            }
        }
        for dependency in &invocation.depends_on {
            if !invocations.contains(dependency) {
                errors.push(ClosureError::UnknownDependency {
                    invocation: invocation.invocation_id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        for output in &invocation.outputs {
            producers.entry(output).or_default().push(invocation);
        }
    }

    // Every consumed channel needs a producer, or a consumer would be invoked
    // with input the plan never supplies.
    for invocation in &compute.step_plan.invocations {
        for input in &invocation.inputs {
            if channels.contains_key(input) && !producers.contains_key(input) {
                errors.push(ClosureError::ChannelWithoutProducer {
                    channel: input.clone(),
                    consumer: invocation.invocation_id.clone(),
                });
            }
        }
    }

    check_writers(&channels, &producers, errors);

    if let Some(unreachable) = cyclic_invocation_count(compute) {
        errors.push(ClosureError::CyclicStepPlan { count: unreachable });
    }
}

/// Checks that the two declarations of ownership agree.
///
/// A channel names its `owner`, and a component lists its `stateOwnership`.
/// Both spellings are in the schema — `docs/protocol-client.md` commits to
/// both — so neither is derived from the other and the validator is where they
/// are reconciled. An owned channel must additionally be single-writer state:
/// there is nothing for a reduction to combine when one instance is the only
/// writer.
fn reconcile_ownership(
    compute: &ComputeSpec,
    channels: &BTreeMap<&StateChannelId, &StateChannel>,
    errors: &mut Errors,
) {
    // Who claims each channel, from the component side.
    let mut claimants: BTreeMap<&StateChannelId, Vec<&ComponentInstanceId>> = BTreeMap::new();
    for component in &compute.components {
        for owned in &component.state_ownership {
            claimants
                .entry(owned)
                .or_default()
                .push(&component.instance_id);
        }
    }

    for (channel_id, channel) in channels {
        let claimed: &[&ComponentInstanceId] = claimants
            .get(channel_id)
            .map_or(&[], |claimants| claimants.as_slice());
        let agrees = match (&channel.owner, claimed) {
            (None, []) => true,
            (Some(owner), [claimant]) => owner == *claimant,
            _ => false,
        };
        if !agrees {
            errors.push(ClosureError::OwnershipDisagreement {
                channel: (*channel_id).clone(),
                claimants: describe_claimants(claimed),
                owner: match &channel.owner {
                    Some(owner) => format!("`{owner}`"),
                    None => "no owner".to_owned(),
                },
            });
            continue;
        }
        if let Some(owner) = &channel.owner
            && channel.reduction != Reduction::Single
        {
            errors.push(ClosureError::OwnedChannelNotSingle {
                channel: (*channel_id).clone(),
                owner: owner.clone(),
                reduction: reduction_name(channel.reduction),
            });
        }
    }

    // A component may also claim a channel that does not exist; that is already
    // reported as an unknown channel, so it is not repeated here.
}

fn describe_claimants(claimants: &[&ComponentInstanceId]) -> String {
    match claimants {
        [] => "no instances".to_owned(),
        [one] => format!("`{one}`"),
        many => {
            let names: Vec<String> = many.iter().map(|id| format!("`{id}`")).collect();
            names.join(" and ")
        }
    }
}

fn reduction_name(reduction: Reduction) -> &'static str {
    match reduction {
        Reduction::Single => "single",
        Reduction::Sum => "sum",
        Reduction::Min => "min",
        Reduction::Max => "max",
    }
}

/// Checks who may write each channel.
///
/// Two rules, and they apply to different things. A channel that admits one
/// writer — owned state, or any channel declaring `single` — must have exactly
/// one producing invocation. A channel with an owner must additionally be
/// written only by invocations running that owner.
fn check_writers(
    channels: &BTreeMap<&StateChannelId, &StateChannel>,
    producers: &BTreeMap<&StateChannelId, Vec<&crate::graph::StepInvocation>>,
    errors: &mut Errors,
) {
    for (channel_id, channel) in channels {
        let writers: &[&crate::graph::StepInvocation] = producers
            .get(channel_id)
            .map_or(&[], |list| list.as_slice());

        // Owned state that nothing writes never advances.
        if let Some(owner) = &channel.owner
            && writers.is_empty()
        {
            errors.push(ClosureError::OwnedStateWithoutWriter {
                channel: (*channel_id).clone(),
                owner: owner.clone(),
            });
            continue;
        }

        // Ownership implies a single writer regardless of the declared
        // reduction; a mismatched reduction is reported separately, so this
        // stays correct even for a manifest that has both problems.
        let single_writer = channel.owner.is_some() || channel.reduction == Reduction::Single;
        if single_writer && writers.len() > 1 {
            errors.push(ClosureError::MultipleWriters {
                channel: (*channel_id).clone(),
                count: writers.len(),
            });
        }

        let Some(owner) = &channel.owner else {
            continue;
        };
        for invocation in writers {
            if invocation.instance != *owner {
                errors.push(ClosureError::WriterIsNotOwner {
                    invocation: invocation.invocation_id.clone(),
                    instance: invocation.instance.clone(),
                    channel: (*channel_id).clone(),
                    owner: owner.clone(),
                });
            }
        }
    }
}

/// How many invocations can never become ready, or `None` when the plan is a
/// DAG.
///
/// Kahn's algorithm: repeatedly retire nodes whose declared dependencies are
/// all retired. Anything left is in a cycle or behind one, and a plan with such
/// a node can never complete a boundary. Unknown dependencies are ignored here
/// because they are already reported separately; treating them as unsatisfiable
/// would report one mistake twice.
fn cyclic_invocation_count(compute: &ComputeSpec) -> Option<usize> {
    let declared: BTreeSet<&StepInvocationId> = compute
        .step_plan
        .invocations
        .iter()
        .map(|invocation| &invocation.invocation_id)
        .collect();

    let mut pending: BTreeMap<&StepInvocationId, BTreeSet<&StepInvocationId>> = compute
        .step_plan
        .invocations
        .iter()
        .map(|invocation| {
            let dependencies = invocation
                .depends_on
                .iter()
                .filter(|dependency| declared.contains(dependency))
                .collect();
            (&invocation.invocation_id, dependencies)
        })
        .collect();

    loop {
        let ready: Vec<&StepInvocationId> = pending
            .iter()
            .filter(|(_, dependencies)| dependencies.is_empty())
            .map(|(id, _)| *id)
            .collect();
        if ready.is_empty() {
            break;
        }
        for id in ready {
            pending.remove(id);
            for dependencies in pending.values_mut() {
                dependencies.remove(id);
            }
        }
    }

    if pending.is_empty() {
        None
    } else {
        Some(pending.len())
    }
}

// ── phase two: the bytes ────────────────────────────────────────────────────

/// Obtains and verifies each distinct blob, holding none of them.
fn verify_artifacts(
    declared: &[DeclaredArtifact],
    blobs: &impl BlobSource,
    errors: &mut Errors,
) -> BTreeMap<ArtifactDigest, VerifiedArtifact> {
    let mut verified = BTreeMap::new();
    for artifact in declared {
        let mut verifier = BlobVerifier {
            expected: &artifact.descriptor,
            hasher: Sha256::new(),
            seen: 0,
            overrun: false,
        };
        if !blobs.read_blob(&artifact.digest, &mut verifier) {
            errors.push(ClosureError::MissingArtifact {
                // Any of its roles identifies the artifact for a person; the
                // set order is stable, so the message is too.
                role: artifact
                    .roles
                    .first()
                    .expect("a declared artifact has at least one role")
                    .clone(),
                digest: artifact.digest,
            });
            continue;
        }
        match verifier.finish() {
            Ok(()) => {
                verified.insert(
                    artifact.digest,
                    VerifiedArtifact {
                        digest: artifact.digest,
                        size_bytes: artifact.size_bytes,
                        media_type: artifact.media_type.clone(),
                        schema: artifact.schema.clone(),
                        roles: artifact.roles.clone(),
                    },
                );
            }
            Err(error) => errors.push(error),
        }
    }
    verified
}
