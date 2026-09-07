//! The structural envelope itself.
//!
//! [`Resource`] is generic over metadata, spec, and status precisely so that
//! it cannot become a universal type. Orishu's `ObjectMeta` and Kagami's
//! catalog metadata stay in their own crates, where their identity and
//! validation rules live; only the five-field shape and the discriminator are
//! shared.
//!
//! The serde implementations are written by hand rather than derived, for
//! three reasons that a derive cannot satisfy at once:
//!
//! - **Field order is a contract.** `kagami-catalog` fingerprints the exact
//!   YAML bytes of a re-encoded document, so `apiVersion`, `kind`, `metadata`,
//!   `spec`, `status` must be emitted in that order, with `status` absent
//!   rather than null when there is none.
//! - **The unknown-field policy differs by consumer.** Orishu's resources
//!   accept unknown top-level fields today; a catalog document refuses them.
//!   `deny_unknown_fields` is not conditional in a derive, and quietly
//!   changing either would be a wire change hidden inside a refactor.
//! - **No unnecessary bounds.** Consumers must not be forced to implement
//!   `Default` or `Clone` to be carried by the envelope.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::discriminator::{ApiVersion, Kind, ResourceHeader};
use crate::error::UnexpectedDiscriminator;

/// The envelope's field spellings, in the order they are emitted.
const FIELDS: &[&str] = &["apiVersion", "kind", "metadata", "spec", "status"];

mod sealed {
    /// Closed so that the set of policies stays reviewable: a third one would
    /// be a new wire behaviour, not a new type.
    pub trait Sealed {}
}

/// How a [`Resource`] treats a top-level field that carries no value.
///
/// That covers two spellings, and they are governed together on purpose:
///
/// - a field the envelope does not recognise at all, and
/// - an explicit `status: null`, which names a recognised field but supplies
///   nothing.
///
/// A permissive envelope tolerates both; a strict one refuses both. Splitting
/// the two would mean an authored document could be strict about a misspelled
/// key yet lenient about a valueless one.
pub trait UnknownFieldPolicy: sealed::Sealed {
    /// `true` when a top-level field carrying no value is an error.
    const DENY: bool;
}

/// Ignore top-level fields that carry no value.
///
/// The default, and what Orishu's resources do. An unknown field is consumed
/// as [`serde::de::IgnoredAny`], so it costs no allocation, and an explicit
/// `status: null` decodes as [`None`] — which is exactly how serde's derive
/// treats an `Option` field everywhere else, and therefore what the Orishu
/// manifest this envelope replaced already did. Tightening it here would be an
/// unlogged wire change, so it is deliberately not tightened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AllowUnknown;

/// Refuse top-level fields that carry no value.
///
/// What `kagami-catalog` does, so an authored document with a misspelled key
/// is reported rather than silently losing the value the user wrote. An
/// explicit `status: null` is refused for the same reason: on a resource whose
/// status type is [`NoStatus`] it would otherwise be the one status spelling
/// that slipped through as a no-op.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DenyUnknown;

impl sealed::Sealed for AllowUnknown {}
impl sealed::Sealed for DenyUnknown {}

impl UnknownFieldPolicy for AllowUnknown {
    const DENY: bool = false;
}

impl UnknownFieldPolicy for DenyUnknown {
    const DENY: bool = true;
}

/// The status slot of a resource that has no observed state.
///
/// Uninhabited, so `Option<NoStatus>` is always `None` and `status` is
/// structurally absent from the encoded form rather than present-and-null.
///
/// # What a document supplying a status gets
///
/// A `status` carrying an actual value is always refused, under either policy:
/// `Option`'s deserializer forwards a non-null value to this type, which has
/// no inhabitant to decode into.
///
/// An explicit `status: null` is different, because `Option` resolves null
/// itself and never consults this type. That spelling follows the envelope's
/// [`UnknownFieldPolicy`], like any other valueless top-level field: refused
/// under [`DenyUnknown`], accepted as `None` under [`AllowUnknown`].
///
/// A resource that must refuse every `status` spelling therefore pairs
/// `NoStatus` with `DenyUnknown`, as `kagami-catalog`'s template document
/// does. The combination is what makes the refusal total; neither half is
/// sufficient alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoStatus {}

impl fmt::Display for NoStatus {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl Serialize for NoStatus {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        match *self {}
    }
}

impl<'de> Deserialize<'de> for NoStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Consume the value before failing: leaving it unread would desync a
        // streaming format part-way through a document.
        de::IgnoredAny::deserialize(deserializer)?;
        Err(de::Error::custom("this resource kind has no `status`"))
    }
}

/// The shared resource envelope:
///
/// ```yaml
/// apiVersion: <domain/version>
/// kind: <resource kind>
/// metadata: <resource-specific metadata>
/// spec: <resource-specific desired or descriptive data>
/// status: <optional resource-specific observed state>
/// ```
///
/// Sharing this shape is a library-boundary decision, not a lifecycle one. It
/// confers no create/update/delete semantics, no controller, no authority, and
/// no identity: a resource that is a synthetic runtime projection stays a
/// projection, and a resource that is client-owned editable data stays that.
/// Every domain rule — which discriminators are supported, what the metadata
/// means, whether the resource is durable, and what its identity is — remains
/// with the crate that owns the resource.
///
/// `metadata`, `spec`, and `status` are public because reading and building
/// them is the domain's business. `apiVersion` and `kind` are private and
/// constructor-supplied, so a resource cannot exist carrying a placeholder
/// kind.
///
/// # Type parameters
///
/// - `M` — the resource-specific metadata type.
/// - `S` — the resource-specific spec.
/// - `T` — the resource-specific status, or [`NoStatus`].
/// - `P` — [`AllowUnknown`] or [`DenyUnknown`].
pub struct Resource<M, S, T = NoStatus, P = AllowUnknown> {
    api_version: ApiVersion,
    kind: Kind,
    /// Resource-specific metadata. Its schema, identity rules, and validation
    /// belong to the owning domain.
    pub metadata: M,
    /// The resource's desired or descriptive state.
    pub spec: S,
    /// The resource's observed state, when it has one.
    pub status: Option<T>,
    policy: PhantomData<P>,
}

impl<M, S, T, P> Resource<M, S, T, P> {
    /// Build a resource with an explicit discriminator and no status.
    pub fn new(api_version: ApiVersion, kind: Kind, metadata: M, spec: S) -> Self {
        Self {
            api_version,
            kind,
            metadata,
            spec,
            status: None,
            policy: PhantomData,
        }
    }

    /// Attach observed state.
    pub fn with_status(mut self, status: T) -> Self {
        self.status = Some(status);
        self
    }

    /// The declared format version.
    pub fn api_version(&self) -> &ApiVersion {
        &self.api_version
    }

    /// The declared resource kind.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// This resource's discriminator on its own.
    pub fn header(&self) -> ResourceHeader {
        ResourceHeader::new(self.api_version.clone(), self.kind.clone())
    }

    /// Check the discriminator against the one the caller supports.
    ///
    /// The expected values come from the caller: this crate never decides
    /// which resources exist.
    pub fn expect(
        &self,
        api_version: &ApiVersion,
        kind: &Kind,
    ) -> Result<(), UnexpectedDiscriminator> {
        self.header().expect(api_version, kind)
    }

    /// Decompose into owned parts, for a domain that needs to convert a
    /// resource into its own representation without cloning.
    pub fn into_parts(self) -> (ApiVersion, Kind, M, S, Option<T>) {
        (
            self.api_version,
            self.kind,
            self.metadata,
            self.spec,
            self.status,
        )
    }
}

// Written out rather than derived: a derive would add a `P: Clone` bound and
// friends for the marker parameter, which carries no data.
impl<M: Clone, S: Clone, T: Clone, P> Clone for Resource<M, S, T, P> {
    fn clone(&self) -> Self {
        Self {
            api_version: self.api_version.clone(),
            kind: self.kind.clone(),
            metadata: self.metadata.clone(),
            spec: self.spec.clone(),
            status: self.status.clone(),
            policy: PhantomData,
        }
    }
}

impl<M: fmt::Debug, S: fmt::Debug, T: fmt::Debug, P> fmt::Debug for Resource<M, S, T, P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Resource")
            .field("apiVersion", &self.api_version)
            .field("kind", &self.kind)
            .field("metadata", &self.metadata)
            .field("spec", &self.spec)
            .field("status", &self.status)
            .finish()
    }
}

impl<M: PartialEq, S: PartialEq, T: PartialEq, P> PartialEq for Resource<M, S, T, P> {
    fn eq(&self, other: &Self) -> bool {
        self.api_version == other.api_version
            && self.kind == other.kind
            && self.metadata == other.metadata
            && self.spec == other.spec
            && self.status == other.status
    }
}

impl<M: Eq, S: Eq, T: Eq, P> Eq for Resource<M, S, T, P> {}

impl<M, S, T, P> Serialize for Resource<M, S, T, P>
where
    M: Serialize,
    S: Serialize,
    T: Serialize,
{
    fn serialize<Ser: Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        // The length is what makes an absent status absent rather than null,
        // matching `skip_serializing_if = "Option::is_none"`.
        let len = 4 + usize::from(self.status.is_some());
        let mut state = serializer.serialize_struct("Resource", len)?;
        state.serialize_field("apiVersion", &self.api_version)?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.serialize_field("spec", &self.spec)?;
        match &self.status {
            Some(status) => state.serialize_field("status", status)?,
            None => state.skip_field("status")?,
        }
        state.end()
    }
}

/// One recognised envelope field, or an ignorable one.
enum Field {
    ApiVersion,
    Kind,
    Metadata,
    Spec,
    Status,
    Other,
}

/// Decodes a map key into a [`Field`], applying `P`'s unknown-field policy at
/// the point where the key's name is still borrowed.
struct FieldSeed<P>(PhantomData<P>);

impl<'de, P: UnknownFieldPolicy> DeserializeSeed<'de> for FieldSeed<P> {
    type Value = Field;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Field, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

impl<'de, P: UnknownFieldPolicy> Visitor<'de> for FieldSeed<P> {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a resource envelope field")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
        match value {
            "apiVersion" => Ok(Field::ApiVersion),
            "kind" => Ok(Field::Kind),
            "metadata" => Ok(Field::Metadata),
            "spec" => Ok(Field::Spec),
            "status" => Ok(Field::Status),
            other if P::DENY => Err(de::Error::unknown_field(other, FIELDS)),
            _ => Ok(Field::Other),
        }
    }
}

struct ResourceVisitor<M, S, T, P>(PhantomData<(M, S, T, P)>);

impl<'de, M, S, T, P> Visitor<'de> for ResourceVisitor<M, S, T, P>
where
    M: Deserialize<'de>,
    S: Deserialize<'de>,
    T: Deserialize<'de>,
    P: UnknownFieldPolicy,
{
    type Value = Resource<M, S, T, P>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a resource with apiVersion, kind, metadata and spec")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut api_version = None;
        let mut kind = None;
        let mut metadata = None;
        let mut spec = None;
        let mut status = None;
        let mut seen_status = false;

        while let Some(field) = map.next_key_seed(FieldSeed::<P>(PhantomData))? {
            match field {
                Field::ApiVersion => {
                    if api_version.is_some() {
                        return Err(de::Error::duplicate_field("apiVersion"));
                    }
                    api_version = Some(map.next_value()?);
                }
                Field::Kind => {
                    if kind.is_some() {
                        return Err(de::Error::duplicate_field("kind"));
                    }
                    kind = Some(map.next_value()?);
                }
                Field::Metadata => {
                    if metadata.is_some() {
                        return Err(de::Error::duplicate_field("metadata"));
                    }
                    metadata = Some(map.next_value()?);
                }
                Field::Spec => {
                    if spec.is_some() {
                        return Err(de::Error::duplicate_field("spec"));
                    }
                    spec = Some(map.next_value()?);
                }
                Field::Status => {
                    if seen_status {
                        return Err(de::Error::duplicate_field("status"));
                    }
                    seen_status = true;
                    // A non-null value reaches `T`, so a `NoStatus` resource
                    // refuses it here whatever the policy. Null never reaches
                    // `T` — `Option` resolves it first — so it is governed by
                    // the policy instead, exactly like any other valueless
                    // top-level field. Under `AllowUnknown` that keeps the
                    // derive's behaviour, which is what makes the migration
                    // wire-neutral; under `DenyUnknown` it closes the one
                    // `status` spelling that would otherwise be a no-op.
                    status = map.next_value::<Option<T>>()?;
                    if P::DENY && status.is_none() {
                        return Err(de::Error::custom("`status` must not be null"));
                    }
                }
                Field::Other => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        Ok(Resource {
            api_version: api_version.ok_or_else(|| de::Error::missing_field("apiVersion"))?,
            kind: kind.ok_or_else(|| de::Error::missing_field("kind"))?,
            metadata: metadata.ok_or_else(|| de::Error::missing_field("metadata"))?,
            spec: spec.ok_or_else(|| de::Error::missing_field("spec"))?,
            status,
            policy: PhantomData,
        })
    }
}

impl<'de, M, S, T, P> Deserialize<'de> for Resource<M, S, T, P>
where
    M: Deserialize<'de>,
    S: Deserialize<'de>,
    T: Deserialize<'de>,
    P: UnknownFieldPolicy,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("Resource", FIELDS, ResourceVisitor(PhantomData))
    }
}
