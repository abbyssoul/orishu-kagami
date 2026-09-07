//! The immutable Orishu workload: what it is, what it depends on, and what
//! makes it that workload and not another.
//!
//! One definition, shared. Kagami compiles an experiment into these types and
//! Orishu admits them; there is no second application-specific schema and no
//! translation between two models that could disagree. That is why this crate
//! sits below both and stays dependency-light — see the budget in
//! `tests/dependencies.rs`.
//!
//! # The shape of a workload
//!
//! ```text
//! workload = manifest + the transitive closure of the artifacts it names
//! ```
//!
//! A [`WorkloadManifest`] declares a bounded graph of component instances,
//! the typed channels between them, and a deterministic step plan
//! ([ADR 0024]). Every executable and every input it needs is named by an
//! [`ArtifactDescriptor`] — a role, a content [`ArtifactDigest`], an exact
//! size, and a media type ([ADR 0010]).
//!
//! # Three things a workload deliberately cannot say
//!
//! - **Where to get its bytes.** A descriptor has no URI, path, registry tag,
//!   peer, or credential field. Location is distribution metadata; changing it
//!   does not produce a different workload, and a manifest that named one would
//!   let two workers execute different bytes for the same apparent workload.
//! - **What it is currently doing.** There is no status slot: the manifest's
//!   status type is uninhabited, so a phase, epoch, simulation time, or
//!   partition map is unrepresentable rather than merely skipped.
//! - **Which cluster it belongs to.** [`WorkloadMeta`] has no server-assigned
//!   uid and no namespace, so the identifier a worker assigns on admission
//!   cannot reach the digest.
//!
//! # Identity
//!
//! [`workload_digest`] hashes the [canonical encoding](canonical) of the root
//! manifest. Because every dependency is named by digest, that one value
//! commits to the whole closure. JSON and YAML are how a person writes a
//! workload; they never define its identity, so whitespace, key order,
//! comments, and the choice of codec cannot change it.
//!
//! The encoding is a full codec, not only a hash input:
//! [`manifest_from_canonical_bytes`] recovers a manifest from those bytes. That
//! is what lets a receiver work from canonical bytes alone, and it is also the
//! check that the encoding is *injective* — an encoder that quietly dropped a
//! field would give two different workloads one digest, and a round trip is
//! what notices.
//!
//! # Nothing large is held
//!
//! Closure verification streams. A candidate artifact arrives in chunks through
//! [`BlobVerifier`], is hashed as it goes, and is never materialised; a
//! [`VerifiedClosure`] records what was verified, not the bytes. That is why
//! [`Limits`] can honestly permit a 64 GiB artifact.
//!
//! # Example
//!
//! ```
//! use orishu_workload::{Limits, authoring, canonical, closure};
//!
//! let document = r#"
//! apiVersion: orishu.dev/v2
//! kind: Workload
//! metadata:
//!   name: cavity
//! spec:
//!   compute:
//!     workloadGraphProfile: orishu.workload-graph/v1
//!     components:
//!       - instanceId: field
//!         artifact:
//!           role: component
//!           digest: sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
//!           sizeBytes: 4
//!           mediaType: application/wasm
//!         pluginId: dev.orishu.em
//!         modelId: yee
//!         schemaId: em.field/v1
//!         engine: wasm-component
//!         lifecycle: orishu.component/v1
//!         stateOwnership: [e-field]
//!     channels:
//!       - channelId: e-field
//!         schema: {schemaId: em.field/v1, version: 1}
//!         owner: field
//!         reduction: single
//!     stepPlan:
//!       profile: orishu.workload-graph/v1
//!       invocations:
//!         - invocationId: advance
//!           instance: field
//!           phaseId: update
//!           outputs: [e-field]
//!   domain:
//!     dimensions: 3
//!     bounds: {shape: cube, sideMetres: 1.0}
//!     discretization: {spaceMetres: 0.001, timeSeconds: 1.5e-11}
//! "#;
//!
//! let limits = Limits::DEFAULT;
//! let manifest = authoring::parse_str(document, &limits)?;
//!
//! // The manifest alone has an identity, before any artifact is fetched.
//! let digest = canonical::workload_digest(&manifest, &limits)?;
//! assert!(digest.to_string().starts_with("sha256:"));
//!
//! // The closure is complete only once the bytes it names are supplied and
//! // verified. The source is addressed by digest and trusted for nothing.
//! let mut blobs = closure::InMemoryBlobs::new();
//! blobs.insert(*b"test");
//! let verified = closure::validate_closure(&manifest, &blobs, &limits)
//!     .expect("the declared artifact was supplied");
//! assert_eq!(verified.root(), digest);
//! assert_eq!(verified.len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [ADR 0010]: https://github.com/abbyssoul/orishu-kagami/blob/main/docs/adr/0010-content-addressed-workload-closure-and-portable-bundles.md
//! [ADR 0024]: https://github.com/abbyssoul/orishu-kagami/blob/main/docs/adr/0024-orishu-orchestrates-a-workload-component-graph.md

#![warn(missing_docs)]

pub mod artifact;
pub mod authoring;
pub mod canonical;
pub mod closure;
pub mod digest;
pub mod domain;
pub mod graph;
pub mod ids;
pub mod limits;
pub mod manifest;
pub mod value;

pub use artifact::{ArtifactDescriptor, ArtifactRole, SchemaCompat};
pub use authoring::AuthoringError;
pub use canonical::{
    CanonicalError, CanonicalMap, CanonicalValue, ToCanonical, manifest_from_canonical_bytes,
    workload_digest,
};
pub use closure::{
    BlobSource, BlobVerifier, ClosureError, ClosureReport, InMemoryBlobs, VerifiedArtifact,
    VerifiedClosure, validate_closure,
};
pub use digest::{ArtifactDigest, DigestAlgorithm, DigestError, WorkloadDigest};
pub use domain::{Discretization, DomainBounds, DomainError, DomainSpec, Integration};
pub use graph::{
    ComponentInstance, ComputeSpec, PlacementConstraint, Reduction, StateChannel, StepInvocation,
    StepPlan,
};
pub use ids::{
    ComponentInstanceId, ComponentRole, ConstraintName, Engine, GraphProfile, LabelKey, LabelValue,
    LifecycleId, LimitName, MediaType, ModelId, NameError, ParameterName, PhaseId, PluginId,
    SchemaId, StateChannelId, StepInvocationId, WorkloadName,
};
pub use limits::Limits;
pub use manifest::{
    WORKLOAD_API_VERSION, WORKLOAD_KIND, WorkloadInputs, WorkloadManifest, WorkloadMeta,
    WorkloadRequirements, WorkloadSpec, api_version, kind, manifest,
};
pub use value::{FiniteF64, FloatError, ScalarValue};
