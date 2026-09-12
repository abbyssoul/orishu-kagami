//! Canonical hashing and bounded, resumable anti-entropy.
//!
//! Gossip converges quickly but has no completion guarantee: a node that was
//! partitioned, or that joined late, can stay missing an entry indefinitely if
//! the delta carrying it retired before reaching it. Anti-entropy closes that
//! gap by comparing hashes of whole state rather than individual updates.
//!
//! # The canonical form
//!
//! Comparing hashes only works if two nodes holding equal state compute equal
//! bytes. A `MerkleDigest` struct alone is not an algorithm, so this module
//! fixes every degree of freedom:
//!
//! - **Leaf key** — a domain tag byte followed by the entity's identifier:
//!   `0x01` for a member (keyed by node ID), `0x02` for a tombstone (node ID),
//!   `0x03` for a blocklist entry (its rendered key), and `0x04` for the
//!   singleton membership policy (`membership`). The tag keeps the
//!   namespaces disjoint, so a member and a tombstone for the same node never
//!   collide.
//! - **Leaf hash** — `SHA-256(LEAF_DOMAIN || len(key) || key || value)`, where
//!   `value` is the length-prefixed, fixed-order encoding written by
//!   this module's canonical encoder. Every variable-length field is
//!   length-prefixed, so no two distinct records can encode to the same
//!   bytes.
//! - **Bucket assignment** — `SHA-256(BUCKET_DOMAIN || key)`, of which the top
//!   `depth` bits give the bucket index. Hashing rather than taking the key
//!   directly spreads sequentially named nodes evenly, so divergence tends to
//!   land in one bucket instead of all of them.
//! - **Bucket hash** — `SHA-256(BUCKET_DOMAIN || count || leaf hashes in
//!   ascending key order)`. Sorting is what makes the result independent of
//!   insertion history.
//! - **Tree** — a complete binary tree over exactly `2^depth` buckets, folded
//!   pairwise with `SHA-256(NODE_DOMAIN || left || right)` up to the root.
//!   Because the bucket count is fixed by configuration, there is no ambiguous
//!   padding rule.
//! - **Subtree addressing** — a bucket index in `0..2^depth`.
//! - **Continuation** — `(bucket, after_key)`. A truncated reply reports the
//!   last leaf key it emitted; the next request resumes strictly after it, in
//!   the same ascending order.
//!
//! All domain strings carry a version suffix, so changing the encoding later
//! is a visible protocol change rather than a silent divergence.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};

use crate::{
    gossip::{DeltaBody, GossipDelta},
    model::{AntiEntropyCursor, BlocklistAction, BlocklistEntry, Liveness, Member, Membership},
};

/// Domain separator for leaf hashing.
const LEAF_DOMAIN: &[u8] = b"orishu.membership.leaf/2";
/// Domain separator for bucket assignment and bucket hashing.
const BUCKET_DOMAIN: &[u8] = b"orishu.membership.bucket/2";
/// Domain separator for internal tree nodes.
const NODE_DOMAIN: &[u8] = b"orishu.membership.node/2";

/// Leaf-key tag for a member record.
const TAG_MEMBER: u8 = 0x01;
/// Leaf-key tag for a membership tombstone.
const TAG_TOMBSTONE: u8 = 0x02;
/// Leaf-key tag for a blocklist entry.
const TAG_BLOCKLIST: u8 = 0x03;

/// A 32-byte SHA-256 digest.
///
/// Hex in human-readable formats, a byte string in binary ones, matching the
/// peer protocol's use of CBOR byte strings for hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Hash256([u8; 32]);

impl Hash256 {
    /// Wraps raw digest bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Renders the digest as lowercase hex.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write as _;
        self.0.iter().fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }
}

impl std::fmt::Display for Hash256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Hash256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_hex())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Hash256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HashVisitor;

        impl<'de> de::Visitor<'de> for HashVisitor {
            type Value = Hash256;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a 32-byte SHA-256 digest")
            }

            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                <[u8; 32]>::try_from(value)
                    .map(Hash256)
                    .map_err(|_| E::custom(format!("expected 32 bytes, got {}", value.len())))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() != 64 {
                    return Err(E::custom(format!(
                        "expected 64 hex characters, got {}",
                        value.len()
                    )));
                }
                // Byte length alone does not make UTF-8 pair slicing safe.
                // Require hex digits before slicing; radix parsing also accepts
                // a leading `+`, which is not part of a digest's hex encoding.
                if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(E::custom("invalid hex digest"));
                }
                let mut bytes = [0u8; 32];
                for (index, byte) in bytes.iter_mut().enumerate() {
                    let pair = &value[index * 2..index * 2 + 2];
                    *byte = u8::from_str_radix(pair, 16).map_err(E::custom)?;
                }
                Ok(Hash256(bytes))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut bytes = [0u8; 32];
                for (index, slot) in bytes.iter_mut().enumerate() {
                    *slot = seq
                        .next_element::<u8>()?
                        .ok_or_else(|| de::Error::invalid_length(index, &self))?;
                }
                if seq.next_element::<u8>()?.is_some() {
                    return Err(de::Error::custom("expected exactly 32 bytes"));
                }
                Ok(Hash256(bytes))
            }
        }

        deserializer.deserialize_any(HashVisitor)
    }
}

/// A digest of one node's membership state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerkleDigest {
    /// Tree depth; the bucket count is `2^depth`.
    pub depth: u8,
    /// Root hash.
    pub root: Hash256,
    /// Bucket hashes in ascending index order. Exactly `2^depth` entries.
    pub buckets: Vec<Hash256>,
}

impl MerkleDigest {
    /// Whether this digest is internally consistent: the bucket vector has the
    /// length its depth implies, and the root folds from those buckets.
    ///
    /// A peer's digest is hostile input like anything else. Comparing against
    /// a malformed one would produce a divergence set that is nonsense rather
    /// than a security failure, but there is no reason to do the work.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        if self.depth == 0 || self.depth > 8 {
            return false;
        }
        if self.buckets.len() != 1usize << self.depth {
            return false;
        }
        fold_root(&self.buckets) == self.root
    }
}

/// Why two digests cannot be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// The peer's digest is not internally consistent.
    #[error("peer digest is malformed")]
    Malformed,
    /// The peer's tree depth differs from ours, so buckets do not correspond.
    #[error("peer digest depth {theirs} does not match local depth {ours}")]
    DepthMismatch {
        /// Local tree depth.
        ours: u8,
        /// The peer's tree depth.
        theirs: u8,
    },
}

/// Writes canonical bytes into a hasher.
///
/// Every variable-length field is length-prefixed and every field is written
/// in a fixed order, so two distinct records cannot produce the same bytes and
/// the same record always produces identical bytes.
struct Encoder(Sha256);

impl Encoder {
    fn new(domain: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(domain);
        Self(hasher)
    }

    fn u8(&mut self, value: u8) -> &mut Self {
        self.0.update([value]);
        self
    }

    fn u32(&mut self, value: u32) -> &mut Self {
        self.0.update(value.to_be_bytes());
        self
    }

    fn u64(&mut self, value: u64) -> &mut Self {
        self.0.update(value.to_be_bytes());
        self
    }

    fn bytes(&mut self, value: &[u8]) -> &mut Self {
        // A length that does not fit in u32 cannot occur: every field reaching
        // here is bounded far below 4 GiB by `Limits`.
        self.u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
        self.0.update(value);
        self
    }

    fn text(&mut self, value: &str) -> &mut Self {
        self.bytes(value.as_bytes())
    }

    fn version(&mut self, version: &orishu_identity::VersionTuple) -> &mut Self {
        self.u64(version.epoch)
            .u64(version.counter)
            .text(version.actor.as_str())
    }

    fn finish(self) -> Hash256 {
        Hash256(self.0.finalize().into())
    }
}

/// Canonical leaf key for a member record.
fn member_key(id: &orishu_identity::NodeId) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + id.as_str().len());
    key.push(TAG_MEMBER);
    key.extend_from_slice(id.as_str().as_bytes());
    key
}

/// Canonical leaf key for a membership tombstone.
fn tombstone_key(id: &orishu_identity::NodeId) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + id.as_str().len());
    key.push(TAG_TOMBSTONE);
    key.extend_from_slice(id.as_str().as_bytes());
    key
}

/// Canonical leaf key for a blocklist entry.
fn blocklist_key(key: &crate::model::BlocklistKey) -> Vec<u8> {
    let rendered = key.to_string();
    let mut bytes = Vec::with_capacity(1 + rendered.len());
    bytes.push(TAG_BLOCKLIST);
    bytes.extend_from_slice(rendered.as_bytes());
    bytes
}

/// Canonical value hash of a member record.
fn member_hash(key: &[u8], member: &Member) -> Hash256 {
    let mut encoder = Encoder::new(LEAF_DOMAIN);
    encoder
        .bytes(key)
        .u8(TAG_MEMBER)
        .version(&member.version)
        .text(member.name.as_str())
        .bytes(member.cert_fingerprint.as_bytes())
        .u32(member.protocol.0)
        .u8(match member.liveness {
            Liveness::Alive => 0,
            Liveness::Suspected => 1,
            Liveness::Dead => 2,
        })
        .u64(member.incarnation.0);

    encoder.u32(member.endpoints.peers.len() as u32);
    for address in &member.endpoints.peers {
        encoder.text(&address.0);
    }
    encoder.u32(member.endpoints.clients.len() as u32);
    for address in &member.endpoints.clients {
        encoder.text(&address.0);
    }

    encoder
        .u8(u8::from(member.accepts.clients))
        .u8(u8::from(member.accepts.peers))
        .u8(u8::from(member.accepts.work))
        .u32(member.capacity.peers)
        .u32(member.capacity.clients)
        .u32(member.capabilities.cpu_cores)
        .u64(member.capabilities.memory_bytes)
        .text(&member.capabilities.architecture)
        .text(&member.capabilities.storage_backend);

    match member.capabilities.storage_replicas {
        Some(replicas) => encoder.u8(1).u32(replicas),
        None => encoder.u8(0).u32(0),
    };

    encoder.u32(member.capabilities.accelerators.len() as u32);
    for accelerator in &member.capabilities.accelerators {
        encoder.text(accelerator);
    }
    encoder.u32(member.capabilities.engines.len() as u32);
    for engine in &member.capabilities.engines {
        encoder.text(&engine.engine).text(&engine.runtime_lifecycle);
    }

    encoder.finish()
}

/// Canonical value hash of a membership tombstone.
fn tombstone_hash(key: &[u8], tombstone: &orishu_identity::MembershipTombstone) -> Hash256 {
    let mut encoder = Encoder::new(LEAF_DOMAIN);
    encoder
        .bytes(key)
        .u8(TAG_TOMBSTONE)
        .version(&tombstone.version)
        .text(tombstone.name.as_str())
        .bytes(tombstone.cert_fingerprint.as_bytes())
        .u8(match tombstone.removal_mode {
            orishu_identity::RemovalMode::Graceful => 0,
            orishu_identity::RemovalMode::Force => 1,
            orishu_identity::RemovalMode::Dead => 2,
            orishu_identity::RemovalMode::Blocklist => 3,
        });
    match &tombstone.reason {
        Some(reason) => encoder.u8(1).text(reason),
        None => encoder.u8(0).text(""),
    };
    encoder.finish()
}

/// Canonical value hash of a blocklist entry.
fn blocklist_hash(key: &[u8], entry: &BlocklistEntry) -> Hash256 {
    let mut encoder = Encoder::new(LEAF_DOMAIN);
    encoder
        .bytes(key)
        .u8(TAG_BLOCKLIST)
        .version(&entry.version)
        .u8(match entry.action {
            BlocklistAction::Block => 0,
            BlocklistAction::Allow => 1,
        })
        .text(&entry.added_by);
    encoder.finish()
}

/// Bucket a leaf key belongs to, taking the top `depth` bits of its hash.
fn bucket_of(key: &[u8], depth: u8) -> u16 {
    let mut hasher = Sha256::new();
    hasher.update(BUCKET_DOMAIN);
    hasher.update(key);
    let digest = hasher.finalize();
    let leading = u16::from_be_bytes([digest[0], digest[1]]);
    leading >> (16 - u32::from(depth))
}

/// Folds bucket hashes pairwise into a root.
fn fold_root(buckets: &[Hash256]) -> Hash256 {
    if buckets.is_empty() {
        return Hash256::default();
    }
    let mut level: Vec<Hash256> = buckets.to_vec();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                let mut hasher = Sha256::new();
                hasher.update(NODE_DOMAIN);
                hasher.update(pair[0].as_bytes());
                // A complete tree over 2^depth buckets always pairs evenly.
                hasher.update(pair.get(1).unwrap_or(&pair[0]).as_bytes());
                Hash256(hasher.finalize().into())
            })
            .collect();
    }
    level[0]
}

/// One canonical leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Leaf {
    bucket: u16,
    key: Vec<u8>,
    hash: Hash256,
    body: DeltaBody,
}

/// A materialized canonical view of one node's membership state.
///
/// Building it is `O(n)` in the number of replicated entities, which is why an
/// anti-entropy round runs on a timer rather than per message.
#[derive(Debug, Clone)]
pub struct MembershipTree {
    depth: u8,
    /// Leaves grouped by bucket, each bucket sorted by ascending key.
    buckets: BTreeMap<u16, Vec<Leaf>>,
}

impl MembershipTree {
    /// Builds the canonical tree for `model` at its configured depth.
    #[must_use]
    pub fn build(model: &Membership) -> Self {
        let depth = model.limits().anti_entropy_depth();
        let mut buckets: BTreeMap<u16, Vec<Leaf>> = BTreeMap::new();

        let mut push = |key: Vec<u8>, hash: Hash256, body: DeltaBody| {
            let bucket = bucket_of(&key, depth);
            buckets.entry(bucket).or_default().push(Leaf {
                bucket,
                key,
                hash,
                body,
            });
        };

        for member in model.members().values() {
            let key = member_key(&member.id);
            let hash = member_hash(&key, member);
            push(key, hash, DeltaBody::MembershipUpdate(member.clone()));
        }
        for tombstone in model.tombstones().values() {
            let key = tombstone_key(&tombstone.node_id);
            let hash = tombstone_hash(&key, tombstone);
            push(key, hash, DeltaBody::TombstoneUpdate(tombstone.clone()));
        }
        for entry in model.blocklist().values() {
            let key = blocklist_key(&entry.key);
            let hash = blocklist_hash(&key, entry);
            push(key, hash, DeltaBody::BlocklistUpdate(entry.clone()));
        }

        if let Some(policy) = model.membership_policy() {
            let key = b"\x04membership".to_vec();
            let mut encoder = Encoder::new(LEAF_DOMAIN);
            encoder
                .bytes(&key)
                .u8(0x04)
                .version(&policy.version)
                .u8(u8::from(policy.locked));
            push(
                key,
                encoder.finish(),
                DeltaBody::MembershipPolicyUpdate(policy.clone()),
            );
        }

        for leaves in buckets.values_mut() {
            leaves.sort_by(|left, right| left.key.cmp(&right.key));
        }

        Self { depth, buckets }
    }

    /// Tree depth.
    #[must_use]
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// This tree's digest.
    #[must_use]
    pub fn digest(&self) -> MerkleDigest {
        let count = 1usize << self.depth;
        let buckets: Vec<Hash256> = (0..count)
            .map(|index| self.bucket_hash(index as u16))
            .collect();
        MerkleDigest {
            depth: self.depth,
            root: fold_root(&buckets),
            buckets,
        }
    }

    /// Hash of one bucket, including the empty case.
    fn bucket_hash(&self, index: u16) -> Hash256 {
        let leaves = self.buckets.get(&index);
        let mut encoder = Encoder::new(BUCKET_DOMAIN);
        let count = leaves.map_or(0, Vec::len);
        encoder.u32(count as u32);
        if let Some(leaves) = leaves {
            for leaf in leaves {
                encoder.0.update(leaf.hash.as_bytes());
            }
        }
        encoder.finish()
    }

    /// Buckets whose contents differ from `theirs`.
    ///
    /// # Errors
    ///
    /// Returns [`DigestError`] when the peer's digest is malformed or uses a
    /// different depth, in which case no meaningful comparison exists.
    pub fn divergent_buckets(&self, theirs: &MerkleDigest) -> Result<Vec<u16>, DigestError> {
        if theirs.depth != self.depth {
            return Err(DigestError::DepthMismatch {
                ours: self.depth,
                theirs: theirs.depth,
            });
        }
        if !theirs.is_well_formed() {
            return Err(DigestError::Malformed);
        }
        let ours = self.digest();
        if ours.root == theirs.root {
            return Ok(Vec::new());
        }
        Ok(ours
            .buckets
            .iter()
            .zip(&theirs.buckets)
            .enumerate()
            .filter(|(_, (ours, theirs))| ours != theirs)
            .map(|(index, _)| index as u16)
            .collect())
    }

    /// Collects deltas from `buckets`, resuming after `cursor` and stopping at
    /// `max` entries.
    ///
    /// Returns the deltas, whether the requested range was exhausted, and
    /// where to resume when it was not. A truncated reply is explicitly
    /// incomplete: the caller must not conclude convergence from it.
    #[must_use]
    pub fn collect(
        &self,
        buckets: &[u16],
        cursor: Option<&AntiEntropyCursor>,
        max: usize,
    ) -> AntiEntropyBatch {
        let mut deltas = Vec::new();
        let mut last: Option<(u16, &[u8])> = None;

        for &bucket in buckets {
            if let Some(cursor) = cursor
                && bucket < cursor.bucket
            {
                continue;
            }
            let Some(leaves) = self.buckets.get(&bucket) else {
                continue;
            };
            // Keys are sorted at construction. Seek a continuation in
            // O(log leaves in bucket), instead of rescanning its entire prefix.
            let start = cursor
                .filter(|cursor| bucket == cursor.bucket)
                .map_or(0, |cursor| {
                    leaves.partition_point(|leaf| leaf.key <= cursor.after_key)
                });
            for leaf in &leaves[start..] {
                if deltas.len() == max {
                    return AntiEntropyBatch {
                        deltas,
                        complete: false,
                        cursor: last.map(|(bucket, after_key)| AntiEntropyCursor {
                            bucket,
                            after_key: after_key.to_vec(),
                        }),
                    };
                }
                deltas.push(GossipDelta {
                    hops: 0,
                    body: leaf.body.clone(),
                });
                // Only a truncated reply needs an owned cursor key.
                last = Some((bucket, &leaf.key));
            }
        }

        AntiEntropyBatch {
            deltas,
            complete: true,
            cursor: None,
        }
    }
}

/// One bounded slice of an anti-entropy reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntiEntropyBatch {
    /// Entries in this slice.
    pub deltas: Vec<GossipDelta>,
    /// Whether the requested buckets were exhausted.
    pub complete: bool,
    /// Where to resume when they were not.
    pub cursor: Option<AntiEntropyCursor>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        limits::{Limits, LimitsSpec},
        testing,
    };

    #[test]
    fn identical_state_produces_identical_digests() {
        let left = testing::model_with_members(32);
        let right = testing::model_with_members(32);
        assert_eq!(
            MembershipTree::build(&left).digest(),
            MembershipTree::build(&right).digest()
        );
    }

    #[test]
    fn digest_is_stable_against_insertion_order() {
        // The same members inserted in the opposite order must hash the same,
        // otherwise two converged nodes would still disagree.
        let forward = testing::model_with_members(16);
        let mut reverse = testing::standalone("node-self");
        for index in (0..16).rev() {
            let member = testing::member(&format!("node-{index:04}"), 1);
            reverse.members_mut().insert(member.id.clone(), member);
        }
        assert_eq!(
            MembershipTree::build(&forward).digest(),
            MembershipTree::build(&reverse).digest()
        );
    }

    #[test]
    fn any_field_change_changes_the_digest() {
        let base = testing::model_with_members(8);
        let baseline = MembershipTree::build(&base).digest();

        let mut changed = base.clone();
        let id = orishu_identity::NodeId::new("node-0003").unwrap();
        changed.members_mut().get_mut(&id).unwrap().liveness = Liveness::Suspected;
        assert_ne!(MembershipTree::build(&changed).digest(), baseline);

        let mut changed = base.clone();
        changed
            .members_mut()
            .get_mut(&id)
            .unwrap()
            .capabilities
            .cpu_cores += 1;
        assert_ne!(MembershipTree::build(&changed).digest(), baseline);
    }

    #[test]
    fn member_and_tombstone_for_one_node_do_not_collide() {
        // The same node ID appears in two namespaces. Without the tag byte
        // they would share a leaf key, and a tombstone would silently replace
        // the member record in the digest.
        let node = orishu_identity::NodeId::new("node-0001").unwrap();
        let member_leaf = member_key(&node);
        let tombstone_leaf = tombstone_key(&node);
        assert_ne!(member_leaf, tombstone_leaf);
        assert_ne!(
            member_hash(&member_leaf, &testing::member("node-0001", 1)),
            tombstone_hash(
                &tombstone_leaf,
                &testing::tombstone("node-0001", orishu_identity::RemovalMode::Force)
            )
        );
    }

    #[test]
    fn length_prefixing_prevents_field_boundary_confusion() {
        // Without length prefixes, ("ab", "c") and ("a", "bc") would encode
        // identically.
        let mut first = testing::member("node-0001", 1);
        first.capabilities.architecture = "ab".into();
        first.capabilities.storage_backend = "c".into();
        let mut second = first.clone();
        second.capabilities.architecture = "a".into();
        second.capabilities.storage_backend = "bc".into();

        let key = member_key(&first.id);
        assert_ne!(member_hash(&key, &first), member_hash(&key, &second));
    }

    #[test]
    fn divergent_buckets_are_empty_for_equal_state() {
        let left = MembershipTree::build(&testing::model_with_members(24));
        let right = MembershipTree::build(&testing::model_with_members(24));
        assert_eq!(
            left.divergent_buckets(&right.digest()).unwrap(),
            Vec::<u16>::new()
        );
    }

    #[test]
    fn one_differing_member_localizes_to_one_bucket() {
        let left = testing::model_with_members(24);
        let mut right = left.clone();
        let id = orishu_identity::NodeId::new("node-0007").unwrap();
        right.members_mut().get_mut(&id).unwrap().liveness = Liveness::Dead;

        let divergent = MembershipTree::build(&left)
            .divergent_buckets(&MembershipTree::build(&right).digest())
            .unwrap();
        assert_eq!(divergent.len(), 1);
        assert_eq!(divergent[0], bucket_of(&member_key(&id), 4));
    }

    #[test]
    fn a_malformed_peer_digest_is_refused() {
        let tree = MembershipTree::build(&testing::model_with_members(4));
        let mut digest = tree.digest();
        digest.buckets.truncate(3);
        assert_eq!(tree.divergent_buckets(&digest), Err(DigestError::Malformed));

        let mut lying = tree.digest();
        lying.root = Hash256::from_bytes([0xFF; 32]);
        assert_eq!(
            tree.divergent_buckets(&lying),
            Err(DigestError::Malformed),
            "a root that does not fold from its buckets is malformed"
        );
    }

    #[test]
    fn every_peer_declared_depth_is_refused_without_shifting_by_it() {
        // Promoted from the `membership_messages` fuzz target, which parses this
        // type straight from peer JSON. `depth` is attacker-chosen, so the range
        // check must run before `1 << depth`: any depth at or above the usize
        // width would otherwise overflow the shift rather than return false.
        // Bucket counts are varied so a matching length cannot mask the check.
        for depth in (0..=u8::MAX).filter(|depth| *depth == 0 || *depth > 8) {
            for buckets in [0, 1, 16, 256] {
                let digest = MerkleDigest {
                    depth,
                    root: Hash256::default(),
                    buckets: vec![Hash256::default(); buckets],
                };
                assert!(
                    !digest.is_well_formed(),
                    "depth {depth} with {buckets} buckets must be refused, not folded"
                );
            }
        }
    }

    #[test]
    fn depth_mismatch_is_reported_rather_than_compared() {
        let ours = MembershipTree::build(&testing::model_with_members(4));
        let mut theirs = ours.digest();
        theirs.depth = 5;
        theirs.buckets = vec![Hash256::default(); 32];
        theirs.root = fold_root(&theirs.buckets);
        assert_eq!(
            ours.divergent_buckets(&theirs),
            Err(DigestError::DepthMismatch { ours: 4, theirs: 5 })
        );
    }

    #[test]
    fn collection_is_bounded_and_resumable() {
        let model = testing::model_with_members(40);
        let tree = MembershipTree::build(&model);
        let all: Vec<u16> = (0..16).collect();

        let first = tree.collect(&all, None, 10);
        assert_eq!(first.deltas.len(), 10);
        assert!(!first.complete);
        let cursor = first.cursor.expect("truncation must report a cursor");

        let mut seen = first.deltas.clone();
        let mut next = tree.collect(&all, Some(&cursor), 10);
        while !next.complete {
            seen.extend(next.deltas.clone());
            next = tree.collect(&all, next.cursor.as_ref(), 10);
        }
        seen.extend(next.deltas);

        // 40 members + the local node.
        assert_eq!(seen.len(), 41);
        let mut entities: Vec<String> = seen.iter().map(|d| d.body.entity()).collect();
        entities.sort();
        entities.dedup();
        assert_eq!(entities.len(), 41, "resumption must not repeat or skip");
    }

    #[test]
    fn cursor_seeking_matches_linear_collection_at_boundaries() {
        let tree = MembershipTree::build(&testing::model_with_members(128));
        let mut cursors = vec![None];
        for bucket in [0, 7, 15, 16, u16::MAX] {
            for key in [vec![], vec![0xff]] {
                cursors.push(Some(AntiEntropyCursor {
                    bucket,
                    after_key: key,
                }));
            }
        }
        for (&bucket, leaves) in &tree.buckets {
            for leaf in leaves {
                for after_key in [leaf.key.clone(), [leaf.key.as_slice(), &[0]].concat()] {
                    cursors.push(Some(AntiEntropyCursor { bucket, after_key }));
                }
            }
        }
        // Preserve the public helper's behavior even for repeated, out-of-order
        // and nonexistent requested buckets; wire validation is a separate layer.
        for buckets in [(0..16).collect::<Vec<_>>(), vec![15, 0, 7, 7, 16]] {
            for cursor in &cursors {
                let remaining: Vec<_> = buckets
                    .iter()
                    .flat_map(|&bucket| {
                        tree.buckets
                            .get(&bucket)
                            .into_iter()
                            .flatten()
                            .filter_map(move |leaf| {
                                if cursor.as_ref().is_some_and(|cursor| {
                                    bucket < cursor.bucket
                                        || (bucket == cursor.bucket && leaf.key <= cursor.after_key)
                                }) {
                                    None
                                } else {
                                    Some((bucket, leaf))
                                }
                            })
                    })
                    .collect();
                for max in [0, 1, 7, 129, usize::MAX] {
                    let taken: Vec<_> = remaining.iter().take(max).collect();
                    let complete = remaining.len() <= max;
                    let expected = AntiEntropyBatch {
                        deltas: taken
                            .iter()
                            .map(|(_, leaf)| GossipDelta {
                                hops: 0,
                                body: leaf.body.clone(),
                            })
                            .collect(),
                        complete,
                        cursor: if complete {
                            None
                        } else {
                            taken.last().map(|(bucket, leaf)| AntiEntropyCursor {
                                bucket: *bucket,
                                after_key: leaf.key.clone(),
                            })
                        },
                    };
                    assert_eq!(tree.collect(&buckets, cursor.as_ref(), max), expected);
                }
            }
        }
    }

    #[test]
    fn continuation_order_is_canonical_across_nodes() {
        let left = MembershipTree::build(&testing::model_with_members(30));
        let right = MembershipTree::build(&testing::model_with_members(30));
        let buckets: Vec<u16> = (0..16).collect();
        assert_eq!(
            left.collect(&buckets, None, 7),
            right.collect(&buckets, None, 7)
        );
    }

    #[test]
    fn depth_changes_the_bucket_count_but_not_the_leaf_hashes() {
        let deep = testing::model_with_limits(
            8,
            Limits::try_from(LimitsSpec {
                anti_entropy_depth: 6,
                ..LimitsSpec::default()
            })
            .unwrap(),
        );
        let digest = MembershipTree::build(&deep).digest();
        assert_eq!(digest.buckets.len(), 64);
        assert!(digest.is_well_formed());
    }

    #[test]
    fn golden_digest_is_stable() {
        // Pins the canonical encoding. A change here is a wire-visible
        // protocol change and must be accompanied by a version bump in the
        // domain separators.
        let model = testing::golden_model();
        let digest = MembershipTree::build(&model).digest();
        assert_eq!(
            digest.root.to_hex(),
            "58260ddd44d437ba3fdce9277a6e4bf22c8dd65ee937e5edbd46f77dfc9609a7"
        );
    }

    #[test]
    fn digest_round_trips_through_json() {
        let digest = MembershipTree::build(&testing::model_with_members(4)).digest();
        let json = serde_json::to_string(&digest).unwrap();
        assert_eq!(serde_json::from_str::<MerkleDigest>(&json).unwrap(), digest);
    }
}
