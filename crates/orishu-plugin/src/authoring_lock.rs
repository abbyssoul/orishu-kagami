//! Durable provider intent independent of fields, kernels and installed inventory.
//! This is a bounded, closed, acyclic graph, not proof of scientific compatibility
//! or availability. Recheck through `Inventory::resolve_lock` before using it.
use crate::{
    codec,
    projection::{Project, object},
    resolution::Selection,
    *,
};
use orishu_resource::ApiVersion;
use orishu_workload::canonical::CanonicalValue as V;
use serde::{Deserialize, Serialize};
use std::io::Write;

mod composition;
pub use composition::{BindingConflict, CompositionError};

/// Version of the standalone provider-intent envelope; not workload evidence.
pub const LOCK_SCHEMA: &str = "orishu.plugin-authoring-lock/v1";

/// Caller-owned cold metadata budgets, never persisted as scientific identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockLimits {
    /// Maximum complete JSON or CBOR bytes, including the envelope.
    pub bytes: usize,
    /// Maximum entries independently in roots, contributions and bindings.
    pub items: usize,
    /// Maximum logical graph work: 3 per member/edge, 1 per root.
    pub work: usize,
    /// Longest dependency path in nodes, capped at the resolver's 64-node limit.
    pub dependency_depth: usize,
}
impl Default for LockLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
impl LockLimits {
    /// Shared interactive authoring profile; caller policies may tighten it.
    pub const DEFAULT: Self = Self {
        bytes: 1024 * 1024,
        items: 4096,
        work: 1_000_000,
        dependency_depth: 32,
    };
}
impl LockLimits {
    fn codec(self) -> Limits {
        Limits {
            max_payload_bytes: self.bytes,
            max_values: self.bytes,
            max_depth: 16,
            max_text_bytes: 256,
            max_contributions: self.items,
            max_schema_items: self.items,
            ..Limits::default()
        }
    }
}

/// Immutable provider intent with structural validation. Private fields and
/// bounded byte readers prevent unvalidated deserialization into this wrapper.
/// It intentionally owns no inventory revision, code, domain or scientific state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionLock {
    selection: Selection,
    canonical_bytes: usize,
}
/// Raw versioned read projection, not a validated lock. Adapters must bound
/// untrusted wire input and use the bounded lock readers before acceptance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockDescription {
    /// Standalone authoring schema version, never an inventory revision.
    pub api_version: ApiVersion,
    /// Raw exact provider intent; deserialization establishes no graph validity.
    pub selection: Selection,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRef<'a> {
    api_version: &'static str,
    selection: &'a Selection,
}

fn invalid(path: &str, message: &str) -> Error {
    Error::new(ErrorCode::InvalidSelection, path, message)
}
fn limit(path: &str) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        path,
        "authoring lock budget exceeded",
    )
}
fn sorted<T: Ord>(items: impl IntoIterator<Item = T>) -> bool {
    let mut prev = None;
    for item in items {
        if prev.as_ref().is_some_and(|p| p >= &item) {
            return false;
        }
        prev = Some(item);
    }
    true
}

impl SelectionLock {
    /// Read an embedded lock with collection/depth/value bounds before typed
    /// construction. The enclosing format must separately cap input bytes;
    /// this additionally checks canonical byte weight and graph validity.
    pub fn deserialize_bounded<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
        limits: LockLimits,
    ) -> Result<Self, D::Error> {
        let wire = codec::structured_from_deserializer(deserializer, &limits.codec())?;
        Self::wire(wire, limits).map_err(serde::de::Error::custom)
    }
    /// Borrowed versioned projection for embedding in authoring formats. The
    /// receiving adapter must use a bounded reader, not raw DTO deserialization.
    pub fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WireRef {
            api_version: LOCK_SCHEMA,
            selection: &self.selection,
        }
        .serialize(serializer)
    }
    /// Validate complete graph shape and canonical encoding capacity. This does
    /// not check declarations: use the inventory resolver or selected compiler too.
    pub fn new(selection: Selection, limits: LockLimits) -> Result<Self, Error> {
        let mut lock = Self {
            selection,
            canonical_bytes: 0,
        };
        lock.canonical_bytes = lock.to_cbor(limits)?.len();
        Ok(lock)
    }
    /// Exact immutable graph, including same-release dependency bindings.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    /// Exact canonical metadata weight cached at construction. This excludes
    /// allocator overhead and lets retention admission charge before allocation.
    pub const fn canonical_byte_length(&self) -> usize {
        self.canonical_bytes
    }
    /// Serializable read projection. Reconstructing a validated lock still
    /// requires the byte readers and the receiving authority's budgets.
    pub fn describe(&self) -> LockDescription {
        LockDescription {
            api_version: ApiVersion::from_static(LOCK_SCHEMA),
            selection: self.selection.clone(),
        }
    }
    /// A fresh inventory revision is supplied by the caller, never loaded from
    /// another machine's persisted lock. No choice or enablement is inferred.
    pub fn request(&self, inventory_revision: u64) -> resolution::ResolutionRequest {
        resolution::ResolutionRequest {
            expected_inventory_revision: inventory_revision,
            roots: self.selection.roots.clone(),
            bindings: self.selection.bindings.clone(),
        }
    }
    /// Deterministic canonical bytes of provider intent, not an execution artifact.
    pub fn to_cbor(&self, limits: LockLimits) -> Result<Vec<u8>, Error> {
        self.validate(limits)?;
        codec::encode(&self.project(), &limits.codec(), limits.bytes)
    }
    /// Read one canonical graph with byte/value/depth/count checks before typed
    /// construction. Noncanonical sets and unreachable/cyclic graphs refuse.
    pub fn from_cbor(bytes: &[u8], limits: LockLimits) -> Result<Self, Error> {
        let wire: LockDescription =
            codec::structured_from_cbor(bytes, &limits.codec(), limits.bytes)?;
        let lock = Self::wire(wire, limits)?;
        if lock.to_cbor(limits)? != bytes {
            return Err(invalid("lock", "noncanonical authoring lock"));
        }
        Ok(lock)
    }
    /// Bounded JSON reader. Rejects duplicate keys, nulls, unknown fields and
    /// excess array entries before deserializing the first rejected entry.
    pub fn from_json(bytes: &[u8], limits: LockLimits) -> Result<Self, Error> {
        let wire: LockDescription =
            codec::structured_from_json(bytes, &limits.codec(), limits.bytes)?;
        Self::wire(wire, limits)
    }
    /// Human-readable authoring representation, written through a bounded sink.
    /// JSON spelling/whitespace is not the lock's canonical identity.
    pub fn to_json(&self, limits: LockLimits) -> Result<Vec<u8>, Error> {
        self.validate(limits)?;
        let mut sink = Sink {
            bytes: Vec::new(),
            max: limits.bytes,
            exceeded: false,
        };
        let result = serde_json::to_writer(
            &mut sink,
            &WireRef {
                api_version: LOCK_SCHEMA,
                selection: &self.selection,
            },
        );
        if sink.exceeded {
            return Err(limit("bytes"));
        }
        result.map_err(|_| Error::malformed("lock", "JSON serialization failed"))?;
        Ok(sink.bytes)
    }
    fn wire(wire: LockDescription, limits: LockLimits) -> Result<Self, Error> {
        if wire.api_version.as_str() != LOCK_SCHEMA {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "apiVersion",
                "unsupported authoring lock version",
            ));
        }
        Self::new(wire.selection, limits)
    }
    /// Recheck caller bounds and graph closure without any inventory or IO.
    /// Cold O((members + bindings) log members) work, O(members + bindings) memory.
    pub fn validate(&self, limits: LockLimits) -> Result<(), Error> {
        if self.canonical_bytes > limits.bytes {
            return Err(limit("bytes"));
        }
        let Selection {
            roots,
            contributions: members,
            bindings,
        } = &self.selection;
        if roots.len() > limits.items
            || members.len() > limits.items
            || bindings.len() > limits.items
        {
            return Err(limit("entries"));
        }
        let work = members
            .len()
            .checked_add(bindings.len())
            .and_then(|n| n.checked_mul(3))
            .and_then(|n| n.checked_add(roots.len()));
        if work.is_none_or(|n| n > limits.work) {
            return Err(limit("work"));
        }
        if !sorted(roots) || !sorted(members) || !sorted(bindings.iter().map(|b| &b.requirement)) {
            return Err(invalid("selection", "duplicate or noncanonical set order"));
        }
        let index = |c| {
            members
                .binary_search(c)
                .map_err(|_| invalid("selection", "reference outside contribution set"))
        };
        let mut depth = vec![0usize; members.len()];
        for root in roots {
            depth[index(root)?] = 1;
        }
        // Sorted binding keys give one contiguous adjacency range per consumer.
        let mut offsets = vec![0usize; members.len() + 1];
        let mut targets = Vec::with_capacity(bindings.len());
        let mut incoming = vec![0usize; members.len()];
        for b in bindings {
            let from = index(&b.requirement.consumer)?;
            let to = index(&b.provider)?;
            offsets[from + 1] += 1;
            incoming[to] += 1;
            targets.push(to);
        }
        for i in 1..offsets.len() {
            offsets[i] += offsets[i - 1];
        }
        let mut ready = Vec::with_capacity(members.len());
        ready.extend(
            incoming
                .iter()
                .enumerate()
                .filter_map(|(i, n)| (*n == 0).then_some(i)),
        );
        let mut cursor = 0;
        while cursor < ready.len() {
            let from = ready[cursor];
            cursor += 1;
            if depth[from] > limits.dependency_depth.min(64) {
                return Err(limit("dependencyDepth"));
            }
            for &to in &targets[offsets[from]..offsets[from + 1]] {
                if depth[from] > 0 {
                    depth[to] = depth[to].max(depth[from] + 1);
                }
                incoming[to] -= 1;
                if incoming[to] == 0 {
                    ready.push(to);
                }
            }
        }
        if ready.len() != members.len() {
            return Err(invalid("bindings", "dependency cycle"));
        }
        if depth.contains(&0) {
            return Err(invalid("contributions", "unreachable contribution"));
        }
        Ok(())
    }
}
impl Project for SelectionLock {
    fn project(&self) -> V {
        let bindings = self
            .selection
            .bindings
            .iter()
            .map(|b| {
                object(vec![
                    (
                        "requirement",
                        Some(object(vec![
                            ("consumer", Some(b.requirement.consumer.project())),
                            ("slot", Some(b.requirement.slot.project())),
                        ])),
                    ),
                    ("provider", Some(b.provider.project())),
                ])
            })
            .collect();
        object(vec![
            ("apiVersion", Some(V::text(LOCK_SCHEMA))),
            (
                "selection",
                Some(object(vec![
                    ("roots", Some(self.selection.roots.project())),
                    (
                        "contributions",
                        Some(self.selection.contributions.project()),
                    ),
                    ("bindings", Some(V::Array(bindings))),
                ])),
            ),
        ])
    }
}
struct Sink {
    bytes: Vec<u8>,
    max: usize,
    exceeded: bool,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("authoring lock byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
