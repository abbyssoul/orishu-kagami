//! Deciding whether a manifest plus some candidate bytes is a complete,
//! self-consistent workload.
//!
//! # What "transport-neutral" means here
//!
//! The validator asks a [`BlobSource`] for bytes **by digest and nothing
//! else**. It never reads a role, a name, a declared length, a media type, or a
//! location from the provider, and it re-hashes everything it is handed. A
//! source that lies about what it holds cannot make a workload validate; the
//! worst it can do is fail to supply something.
//!
//! That is why this module has no notion of a cache, a peer, a file, or an
//! archive. Thin submission, a local directory import, a portable bundle, and a
//! peer fetch are all just different things that can implement one trait, and
//! all of them face the same verification.
//!
//! # Every error, not the first
//!
//! [`validate_closure`] returns a [`ClosureReport`] carrying every problem it
//! found, following `kagami-catalog`'s diagnostics. A submitter fixing a
//! workload wants the whole list, and re-running the validator once per
//! mistake is a poor substitute. No partial [`VerifiedClosure`] is produced on
//! failure: a workload is complete or it is not.
//!
//! # What this checks, and what it does not
//!
//! Checked here: artifact presence, digest, exact size, declared-byte budgets,
//! role cardinality, and the *structure* of the component graph — that names
//! resolve, that consumed channels have producers, that owned state has exactly
//! one writer, and that the step plan is a DAG within its bounds.
//!
//! Deliberately not checked here, and owned by later work: expression
//! resolution and the variables language, physical dimension compatibility
//! between channels, finite-value numerical rules beyond
//! [`crate::value::FiniteF64`], model-family exclusivity, per-model policy,
//! hardware-requirement matching, and placement feasibility against a real
//! cluster. A caller must not read a successful validation as a statement that
//! the physics is admissible.

use std::collections::{BTreeMap, BTreeSet};

use crate::artifact::{ArtifactDescriptor, ArtifactRole};
use crate::digest::ArtifactDigest;
use crate::domain::DomainError;
use crate::graph::ComputeSpec;
use crate::ids::{ComponentInstanceId, StateChannelId, StepInvocationId};
use crate::limits::Limits;
use crate::manifest::WorkloadManifest;

/// Somewhere candidate bytes can be obtained, addressed only by digest.
///
/// Implement this over whatever holds bytes — an in-memory map, a verified
/// cache, an unpacked archive. The implementation is trusted to return *some*
/// bytes and trusted for nothing else.
pub trait BlobSource {
    /// The candidate bytes for `digest`, if this source has any.
    ///
    /// Returning the wrong bytes is not an error the implementation needs to
    /// detect: the validator re-hashes what it receives and reports a mismatch.
    fn blob(&self, digest: &ArtifactDigest) -> Option<&[u8]>;
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
    fn blob(&self, digest: &ArtifactDigest) -> Option<&[u8]> {
        self.blobs.get(digest).map(Vec::as_slice)
    }
}

/// One artifact whose bytes have been obtained and verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    /// What the manifest said this artifact is.
    pub descriptor: ArtifactDescriptor,
    /// Its verified bytes.
    pub content: Vec<u8>,
}

/// A manifest and its complete, verified closure.
///
/// Only constructible by [`validate_closure`] returning `Ok`, so holding one is
/// evidence that every declared artifact was present and matched.
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
    /// keyed by digest, so content is stored once.
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
        /// The role the manifest declared.
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
    /// Supplied bytes are not the declared length.
    #[error("artifact {digest} declares {declared} bytes but {actual} were supplied")]
    SizeMismatch {
        /// The artifact.
        digest: ArtifactDigest,
        /// Length declared.
        declared: u64,
        /// Length supplied.
        actual: u64,
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
    /// A channel is read but never written.
    ///
    /// A channel with no producer means a consumer would be invoked with input
    /// that nothing in the admitted plan supplies.
    #[error("channel `{channel}` is consumed by `{consumer}` but no invocation produces it")]
    ChannelWithoutProducer {
        /// The unproduced channel.
        channel: StateChannelId,
        /// One invocation that consumes it.
        consumer: StepInvocationId,
    },
    /// An owned state channel has more than one writer.
    ///
    /// ADR 0024 requires a single allowed writer for authoritative state;
    /// several producers are only legal for a contribution channel under a
    /// declared reduction.
    #[error(
        "channel `{channel}` is owned state with reduction `single` but is produced by {count} invocations"
    )]
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
}

/// Everything wrong with a workload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureReport {
    errors: Vec<ClosureError>,
}

impl ClosureReport {
    /// The problems found, in the order they were detected.
    #[must_use]
    pub fn errors(&self) -> &[ClosureError] {
        &self.errors
    }

    /// How many problems were found.
    #[must_use]
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Whether the report is empty. Never true for a report a caller receives.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl std::fmt::Display for ClosureReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the workload is not admissible:")?;
        for error in &self.errors {
            write!(formatter, "\n  - {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ClosureReport {}

/// Validates a manifest and its candidate bytes.
///
/// Returns a [`VerifiedClosure`] only when the manifest is structurally sound
/// *and* every artifact it declares was supplied and verified.
///
/// # Errors
///
/// Returns a [`ClosureReport`] listing every problem found. The report is never
/// empty and no partial closure accompanies it.
pub fn validate_closure(
    manifest: &WorkloadManifest,
    blobs: &impl BlobSource,
    limits: &Limits,
) -> Result<VerifiedClosure, ClosureReport> {
    let mut errors = Vec::new();

    if let Err(error) = manifest.expect(&crate::manifest::api_version(), &crate::manifest::kind()) {
        // Nothing below is meaningful for a document that is not a workload of
        // this version, so this is the one check that stops early.
        return Err(ClosureReport {
            errors: vec![error.into()],
        });
    }

    if let Err(error) = manifest.spec.domain.validate(limits) {
        errors.push(error.into());
    }

    validate_graph(&manifest.spec.compute, limits, &mut errors);
    let artifacts = validate_artifacts(manifest, blobs, limits, &mut errors);

    let root = match crate::canonical::workload_digest(manifest, limits) {
        Ok(root) => Some(root),
        Err(error) => {
            errors.push(error.into());
            None
        }
    };

    match (errors.is_empty(), root) {
        (true, Some(root)) => Ok(VerifiedClosure { root, artifacts }),
        _ => Err(ClosureReport { errors }),
    }
}

/// Collects every descriptor a manifest declares, in a stable order.
fn declared_artifacts(manifest: &WorkloadManifest) -> Vec<&ArtifactDescriptor> {
    manifest
        .spec
        .compute
        .components
        .iter()
        .map(|component| &component.artifact)
        .chain(manifest.spec.inputs.geometry.iter())
        .chain(manifest.spec.inputs.initial_conditions.iter())
        .chain(manifest.spec.inputs.additional.iter())
        .collect()
}

fn validate_artifacts(
    manifest: &WorkloadManifest,
    blobs: &impl BlobSource,
    limits: &Limits,
    errors: &mut Vec<ClosureError>,
) -> BTreeMap<ArtifactDigest, VerifiedArtifact> {
    let declared = declared_artifacts(manifest);

    if declared.len() > limits.max_artifacts {
        errors.push(ClosureError::LimitExceeded {
            what: "the artifact count",
            found: declared.len(),
            limit: limits.max_artifacts,
        });
    }
    if manifest.spec.inputs.initial_conditions.len() > limits.max_initial_conditions {
        errors.push(ClosureError::LimitExceeded {
            what: "the initial-condition count",
            found: manifest.spec.inputs.initial_conditions.len(),
            limit: limits.max_initial_conditions,
        });
    }

    // Role cardinality, over the declarations rather than over the deduplicated
    // blobs: declaring one geometry twice is a manifest mistake even when both
    // declarations name identical bytes.
    let mut role_counts: BTreeMap<&ArtifactRole, usize> = BTreeMap::new();
    for descriptor in &declared {
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

    // Aggregate declared size is checked before a single blob is requested, so
    // a manifest cannot make a reader fetch more than it agreed to hold.
    let mut aggregate: u64 = 0;
    let mut counted: BTreeSet<ArtifactDigest> = BTreeSet::new();
    for descriptor in &declared {
        if descriptor.size_bytes > limits.max_artifact_bytes {
            errors.push(ClosureError::ArtifactTooLarge {
                digest: descriptor.digest,
                declared: descriptor.size_bytes,
                limit: limits.max_artifact_bytes,
            });
        }
        // Identical content shared by two roles is transferred and stored once,
        // so it counts once against the budget.
        if counted.insert(descriptor.digest) {
            aggregate = aggregate.saturating_add(descriptor.size_bytes);
        }
    }
    if aggregate > limits.max_aggregate_declared_bytes {
        errors.push(ClosureError::ClosureTooLarge {
            declared: aggregate,
            limit: limits.max_aggregate_declared_bytes,
        });
        // Do not go on to fetch bytes for a closure already over budget.
        return BTreeMap::new();
    }

    let mut verified: BTreeMap<ArtifactDigest, VerifiedArtifact> = BTreeMap::new();
    for descriptor in declared {
        if let Some(existing) = verified.get(&descriptor.digest) {
            if !describes_the_same_bytes(&existing.descriptor, descriptor) {
                errors.push(ClosureError::ConflictingDescriptor {
                    digest: descriptor.digest,
                });
            }
            continue;
        }

        let Some(content) = blobs.blob(&descriptor.digest) else {
            errors.push(ClosureError::MissingArtifact {
                role: descriptor.role.clone(),
                digest: descriptor.digest,
            });
            continue;
        };

        // Size before hashing: a cheap check that refuses an oversized or
        // truncated candidate without paying for a digest over it.
        let actual_size = content.len() as u64;
        if actual_size != descriptor.size_bytes {
            errors.push(ClosureError::SizeMismatch {
                digest: descriptor.digest,
                declared: descriptor.size_bytes,
                actual: actual_size,
            });
            continue;
        }

        // The provider said nothing that got us here except "these are the
        // bytes"; this is what decides they are the artifact.
        if !descriptor.digest.matches(content) {
            errors.push(ClosureError::DigestMismatch {
                declared: descriptor.digest,
                actual: ArtifactDigest::sha256_of(content),
            });
            continue;
        }

        verified.insert(
            descriptor.digest,
            VerifiedArtifact {
                descriptor: descriptor.clone(),
                content: content.to_vec(),
            },
        );
    }
    verified
}

/// Whether two descriptors make the same claims about one blob's bytes.
///
/// Role is excluded: it says what the artifact is *for*, and one blob may serve
/// two purposes without contradiction. Everything else here is a claim about
/// the bytes, and two different claims about one digest cannot both hold.
fn describes_the_same_bytes(left: &ArtifactDescriptor, right: &ArtifactDescriptor) -> bool {
    left.size_bytes == right.size_bytes
        && left.media_type == right.media_type
        && left.schema == right.schema
}

fn validate_graph(compute: &ComputeSpec, limits: &Limits, errors: &mut Vec<ClosureError>) {
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

    let mut channels: BTreeMap<&StateChannelId, &crate::graph::StateChannel> = BTreeMap::new();
    for channel in &compute.channels {
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
        for instance in &constraint.instances {
            if !instances.contains(instance) {
                errors.push(ClosureError::UnknownInstance {
                    referrer: format!("placement constraint {}", constraint.constraint),
                    instance: instance.clone(),
                });
            }
        }
    }

    // ── the step plan ──────────────────────────────────────────────────────
    let mut invocations: BTreeSet<&StepInvocationId> = BTreeSet::new();
    for invocation in &compute.step_plan.invocations {
        if !invocations.insert(&invocation.invocation_id) {
            errors.push(ClosureError::DuplicateInvocation {
                invocation: invocation.invocation_id.clone(),
            });
        }
    }

    // Which invocations write which channel, for producer and writer rules.
    let mut producers: BTreeMap<&StateChannelId, Vec<&StepInvocationId>> = BTreeMap::new();

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
            producers
                .entry(output)
                .or_default()
                .push(&invocation.invocation_id);
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

    // Authoritative state has exactly one writer, and that writer runs the
    // instance that owns it.
    for (channel_id, writers) in &producers {
        let Some(channel) = channels.get(channel_id) else {
            continue;
        };
        if channel.reduction == crate::graph::Reduction::Single && writers.len() > 1 {
            errors.push(ClosureError::MultipleWriters {
                channel: (*channel_id).clone(),
                count: writers.len(),
            });
        }
        let Some(owner) = &channel.owner else {
            continue;
        };
        for writer in writers {
            let Some(invocation) = compute
                .step_plan
                .invocations
                .iter()
                .find(|candidate| candidate.invocation_id == **writer)
            else {
                continue;
            };
            if invocation.instance != *owner {
                errors.push(ClosureError::WriterIsNotOwner {
                    invocation: (*writer).clone(),
                    instance: invocation.instance.clone(),
                    channel: (*channel_id).clone(),
                    owner: owner.clone(),
                });
            }
        }
    }

    if let Some(unreachable) = cyclic_invocation_count(compute) {
        errors.push(ClosureError::CyclicStepPlan { count: unreachable });
    }
}

fn check_limit(what: &'static str, found: usize, limit: usize, errors: &mut Vec<ClosureError>) {
    if found > limit {
        errors.push(ClosureError::LimitExceeded { what, found, limit });
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
