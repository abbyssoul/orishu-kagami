//! The shared typed resource envelope.
//!
//! Orishu's workload, cluster, and node resources and Kagami's object
//! templates are all read, printed, saved, and inspected in the same
//! human-readable shape:
//!
//! ```yaml
//! apiVersion: <domain/version>
//! kind: <resource kind>
//! metadata: <resource-specific metadata>
//! spec: <resource-specific desired or descriptive data>
//! status: <optional resource-specific observed state>
//! ```
//!
//! This crate owns that shape and the [`ApiVersion`]/[`Kind`] discriminator,
//! so the two domains stop maintaining parallel definitions of it.
//!
//! # What this crate is not
//!
//! Sharing a shape is not sharing a lifecycle. This is a library boundary, not
//! a decision to behave like Kubernetes: there is no API server, no CRD
//! mechanism, no controller, no watch, and no generic create/update/delete
//! semantics anywhere in this crate. Nor is there a universal metadata type or
//! a universal string identity — metadata is a type parameter precisely so
//! that a formation identity, a node identity, a workload identity, and a
//! catalog template identity stay distinct types with their own authority and
//! scope.
//!
//! Each domain therefore keeps:
//!
//! | Concern | Owner |
//! | --- | --- |
//! | Which `apiVersion`/`kind` values are supported | the resource's crate |
//! | Metadata schema, identity, and validation | the resource's crate |
//! | Spec and status validation | the resource's crate |
//! | Authority, mutability, persistence, canonicalisation | the resource's crate |
//! | The five-field shape and the discriminator | here |
//!
//! # What this crate contains
//!
//! - [`Resource`] — the generic envelope.
//! - [`ResourceHeader`] — the discriminator alone, decodable before a body is
//!   trusted, so a document from an unknown future version is refused for its
//!   version rather than for whichever unrecognisable field is read first.
//! - [`ApiVersion`] and [`Kind`] — bounded, validated new-types.
//! - [`ResourceError`] and [`UnexpectedDiscriminator`] — structural failures
//!   only. Domain validation errors stay structured in the domain that owns
//!   them and are never flattened through here.
//!
//! # Bounds and untrusted input
//!
//! A discriminator is the one field a hostile document is guaranteed to reach,
//! because it is read before anything else is trusted. [`ApiVersion`] and
//! [`Kind`] check their declared [`ApiVersion::MAX_LEN`]/[`Kind::MAX_LEN`]
//! bound against the borrowed string, before any copy is made — on the
//! constructor as well as the deserialization path, so an oversized value is
//! refused without ever being allocated. `tests/allocation.rs` asserts that
//! ordering with a counting allocator, since the resulting error is the same
//! either way.
//!
//! Beyond that, this crate offers no parsing entry point of its own: it has no
//! codec, no reader, and no filesystem. Byte, nesting, and collection limits
//! belong to the bounded input layer that owns the document — `kagami-catalog`
//! has one, and an Orishu network-facing path must supply one. There is
//! deliberately no convenient unbounded parser here to reach for instead.
//!
//! # Dependencies
//!
//! `serde` and nothing else. No codec, no async runtime, no transport, no
//! filesystem, no UI, and no Orishu or Kagami domain crate; `tests/dependencies.rs`
//! asserts that against the resolved dependency graph rather than against the
//! import list, so a capability cannot arrive transitively.
//!
//! # An Orishu-style resource
//!
//! Orishu metadata carries a human name, an optional system-assigned UID, a
//! namespace, and labels, and its resources have observed state:
//!
//! ```
//! use orishu_resource::{ApiVersion, Kind, Resource};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! #[serde(rename_all = "camelCase")]
//! struct ObjectMeta {
//!     name: String,
//!     #[serde(skip_serializing_if = "Option::is_none")]
//!     uid: Option<String>,
//! }
//!
//! #[derive(Serialize, Deserialize)]
//! #[serde(rename_all = "camelCase")]
//! struct WorkloadSpec { domain_type: String }
//!
//! #[derive(Serialize, Deserialize)]
//! struct WorkloadStatus { epoch: u64 }
//!
//! type Workload = Resource<ObjectMeta, WorkloadSpec, WorkloadStatus>;
//!
//! let workload = Workload::new(
//!     ApiVersion::from_static("orishu.dev/v1"),
//!     Kind::from_static("Workload"),
//!     ObjectMeta { name: "em-cavity".to_owned(), uid: None },
//!     WorkloadSpec { domain_type: "electromagnetic".to_owned() },
//! );
//!
//! // Field order is fixed, and an absent status is absent rather than null.
//! assert_eq!(
//!     serde_json::to_string(&workload)?,
//!     r#"{"apiVersion":"orishu.dev/v1","kind":"Workload","metadata":{"name":"em-cavity"},"spec":{"domainType":"electromagnetic"}}"#
//! );
//!
//! let running = workload.with_status(WorkloadStatus { epoch: 3 });
//! assert!(serde_json::to_string(&running)?.ends_with(r#""status":{"epoch":3}}"#));
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! # A Kagami-style resource
//!
//! Catalog metadata is deliberately different — it names a catalog and a
//! template, not a namespace and a UID — an object template has no status, and
//! an authored document refuses a key it does not recognise so a typo is
//! reported rather than silently dropped:
//!
//! ```
//! use orishu_resource::{ApiVersion, DenyUnknown, Kind, NoStatus, Resource};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, PartialEq, Serialize, Deserialize)]
//! #[serde(deny_unknown_fields)]
//! struct MetadataDocument { catalog: String, name: String }
//!
//! #[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
//! #[serde(deny_unknown_fields)]
//! struct SpecDocument { components: Vec<String> }
//!
//! type TemplateDocument =
//!     Resource<MetadataDocument, SpecDocument, NoStatus, DenyUnknown>;
//!
//! let document: TemplateDocument = serde_yaml::from_str(
//!     "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\n\
//!      metadata: {catalog: planets, name: sun}\nspec: {components: [mass]}\n",
//! )?;
//! assert_eq!(document.metadata.catalog, "planets");
//!
//! // Re-encoding is symmetric, which is what makes a content fingerprint
//! // independent of how the file was actually laid out.
//! let reordered: TemplateDocument = serde_yaml::from_str(
//!     "kind: ObjectTemplate\napiVersion: kagami.catalog/v1\n\
//!      spec: {components: [mass]}\nmetadata: {name: sun, catalog: planets}\n",
//! )?;
//! assert_eq!(serde_yaml::to_string(&reordered)?, serde_yaml::to_string(&document)?);
//!
//! // A misspelled top-level key is refused, not dropped.
//! assert!(serde_yaml::from_str::<TemplateDocument>(
//!     "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\n\
//!      metadata: {catalog: planets, name: sun}\nspecc: {}\n",
//! ).is_err());
//! # Ok::<(), serde_yaml::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod discriminator;
pub mod error;
pub mod resource;

pub use discriminator::{ApiVersion, Kind, ResourceHeader};
pub use error::{Discriminator, ResourceError, UnexpectedDiscriminator};
pub use resource::{AllowUnknown, DenyUnknown, NoStatus, Resource, UnknownFieldPolicy};
