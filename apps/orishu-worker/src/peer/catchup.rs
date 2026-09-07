//! Complete, immutable public admission-state baselines. No credentials enter
//! this module. Transfer authorization, retention and atomic core installation
//! belong to the owner and are not implied by successful baseline validation.

use super::codec;
use orishu_membership::{
    FormationId, Membership, MembershipTombstone, NodeId,
    model::{BlocklistEntry, BlocklistKey, MembershipPolicy},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod client;
pub mod store;
pub mod wire;

/// Exact per-page wire limit, checked before typed allocation.
pub const MAX_PAGE_BYTES: usize = 65_536;
const PAGE_RECORDS: usize = 32;
const MAX_BLOCKS: usize = 1024;
const MAX_TOMBSTONES: usize = 4096;
const MAX_PAGES: usize = (1 + MAX_BLOCKS + MAX_TOMBSTONES).div_ceil(PAGE_RECORDS);
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Secret-free baseline identity, pinned before receiving any page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    /// Formation whose admission rules were captured.
    pub formation: FormationId,
    /// Admitted source identity, independently authenticated by the adapter.
    pub source: NodeId,
    /// Source-assigned identity; owner retention must bind it to a requester.
    pub snapshot: u64,
    /// Complete page count, including the explicit policy-only empty baseline.
    pub pages: u16,
    /// Digest of ordered, length-prefixed canonical page encodings.
    pub root: [u8; 32],
}

/// Only admission-relevant public entities can appear in this transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", deny_unknown_fields)]
pub enum Record {
    /// Absence explicitly means the initial unlocked policy, not a missing page.
    Policy(Option<MembershipPolicy>),
    /// A restriction or versioned lift, never omitted because it allows access.
    Block(BlocklistEntry),
    /// An active or explicitly cleared membership-removal barrier.
    Tombstone(MembershipTombstone),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Policy,
    Block(BlocklistKey),
    Tombstone(NodeId),
}
impl Record {
    fn key(&self) -> Key {
        match self {
            Self::Policy(_) => Key::Policy,
            Self::Block(record) => Key::Block(record.key.clone()),
            Self::Tombstone(record) => Key::Tombstone(record.node_id.clone()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    schema_version: u8,
    formation: FormationId,
    source: NodeId,
    snapshot: u64,
    index: u16,
    pages: u16,
    records: Vec<Record>,
}

/// Bounded failure without peer payload text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Transfer bytes, page count or record count exceeds the fixed profile.
    #[error("admission baseline exceeds limits")]
    Limit,
    /// Identity, schema, ordering, digest or completion is inconsistent.
    #[error("admission baseline is invalid or incomplete")]
    Invalid,
    /// The underlying bounded CBOR profile rejected input.
    #[error(transparent)]
    Codec(#[from] codec::CodecError),
}

fn hasher() -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"orishu/admission-baseline/1\0");
    hash
}

fn hash_page(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u32).to_be_bytes());
    hash.update(bytes);
}

/// Frozen owner-local pages. Capturing a baseline does not authorize serving it.
pub struct Frozen {
    descriptor: Descriptor,
    pages: Vec<Vec<u8>>,
}
impl Frozen {
    /// Call in one serialized owner turn, with no intervening model mutation.
    /// Oversized complete state is refused, never silently truncated.
    pub fn capture(model: &Membership, snapshot: u64) -> Result<Self, Error> {
        if model.blocklist().len() > MAX_BLOCKS || model.tombstones().len() > MAX_TOMBSTONES {
            return Err(Error::Limit);
        }
        let records: Vec<_> = std::iter::once(Record::Policy(model.membership_policy().cloned()))
            .chain(model.blocklist().values().cloned().map(Record::Block))
            .chain(model.tombstones().values().cloned().map(Record::Tombstone))
            .collect();
        let total = records.len().div_ceil(PAGE_RECORDS) as u16;
        let mut pages = Vec::new();
        let mut bytes = 0;
        let mut hash = hasher();
        for (index, chunk) in records.chunks(PAGE_RECORDS).enumerate() {
            let encoded = codec::encode(&Page {
                schema_version: 1,
                formation: model.formation().clone(),
                source: model.local_id().clone(),
                snapshot,
                index: index as u16,
                pages: total,
                records: chunk.to_vec(),
            })?;
            bytes += encoded.len();
            if encoded.len() > MAX_PAGE_BYTES || bytes > MAX_BYTES {
                return Err(Error::Limit);
            }
            hash_page(&mut hash, &encoded);
            pages.push(encoded);
        }
        Ok(Self {
            descriptor: Descriptor {
                formation: model.formation().clone(),
                source: model.local_id().clone(),
                snapshot,
                pages: total,
                root: hash.finalize().into(),
            },
            pages,
        })
    }

    /// Metadata that the authenticated transfer pins before requesting pages.
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    /// Borrow an immutable encoded page; out-of-range indexes return None.
    pub fn page(&self, index: u16) -> Option<&[u8]> {
        self.pages.get(usize::from(index)).map(Vec::as_slice)
    }
}

/// Private partial state; records cannot escape before verified completion.
pub struct Receiver {
    descriptor: Descriptor,
    next: u16,
    bytes: usize,
    hash: Sha256,
    records: Vec<Record>,
    blocks: usize,
    tombstones: usize,
}
impl Receiver {
    /// Start a receiver for an independently authenticated descriptor. Reject
    /// zero/excessive page counts without reserving peer-claimed storage.
    pub fn new(descriptor: Descriptor) -> Result<Self, Error> {
        if descriptor.pages == 0 || usize::from(descriptor.pages) > MAX_PAGES {
            return Err(Error::Limit);
        }
        Ok(Self {
            descriptor,
            next: 0,
            bytes: 0,
            hash: hasher(),
            records: Vec::new(),
            blocks: 0,
            tombstones: 0,
        })
    }

    /// Consume self on error, preventing a failed transfer from being resumed
    /// with partially accepted metadata. Decode only after checking page bytes.
    pub fn push(mut self, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_PAGE_BYTES || self.bytes + bytes.len() > MAX_BYTES {
            return Err(Error::Limit);
        }
        let page: Page = codec::decode(bytes)?;
        if page.schema_version != 1
            || page.formation != self.descriptor.formation
            || page.source != self.descriptor.source
            || page.snapshot != self.descriptor.snapshot
            || page.pages != self.descriptor.pages
            || page.index != self.next
            || self.next >= self.descriptor.pages
            || page.records.is_empty()
            || page.records.len() > PAGE_RECORDS
            || (self.next + 1 < page.pages && page.records.len() != PAGE_RECORDS)
            || codec::encode(&page)? != bytes
        {
            return Err(Error::Invalid);
        }
        let mut previous = self.records.last().map(Record::key);
        for record in &page.records {
            let key = record.key();
            if previous.as_ref().is_some_and(|last| last >= &key)
                || (previous.is_none() && key != Key::Policy)
            {
                return Err(Error::Invalid);
            }
            match record {
                Record::Block(_) => self.blocks += 1,
                Record::Tombstone(_) => self.tombstones += 1,
                Record::Policy(_) => {}
            }
            previous = Some(key);
        }
        if self.blocks > MAX_BLOCKS || self.tombstones > MAX_TOMBSTONES {
            return Err(Error::Limit);
        }
        self.next += 1;
        self.bytes += bytes.len();
        hash_page(&mut self.hash, bytes);
        self.records.extend(page.records);
        Ok(self)
    }

    /// Complete content proof only. The owner must additionally validate domain
    /// bounds, atomically merge, and install target credentials before readiness.
    pub fn finish(self) -> Result<Vec<Record>, Error> {
        let root: [u8; 32] = self.hash.finalize().into();
        if self.next != self.descriptor.pages || root != self.descriptor.root {
            return Err(Error::Invalid);
        }
        Ok(self.records)
    }

    /// Convert only a proven-complete transfer into the core's atomic command
    /// value. Formation/snapshot binding is retained, not supplied by a caller.
    pub fn finish_baseline(self) -> Result<orishu_membership::AdmissionBaseline, Error> {
        let mut baseline = orishu_membership::AdmissionBaseline {
            schema_version: 1,
            formation_id: self.descriptor.formation.clone(),
            snapshot: self.descriptor.snapshot,
            policy: None,
            blocklist: Vec::new(),
            tombstones: Vec::new(),
        };
        for record in self.finish()? {
            match record {
                Record::Policy(policy) => baseline.policy = policy,
                Record::Block(block) => baseline.blocklist.push(block),
                Record::Tombstone(tombstone) => baseline.tombstones.push(tombstone),
            }
        }
        Ok(baseline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::{Command, Message, testing};

    #[test]
    fn baseline_retains_lifted_blocks_and_cleared_tombstones() {
        let mut model = testing::model_with_members(2);
        let node = model
            .members()
            .keys()
            .find(|id| *id != model.local_id())
            .unwrap()
            .clone();
        model = orishu_membership::update(
            model,
            Message::Local(Command::RemoveMember {
                node: node.clone(),
                mode: orishu_membership::RemovalMode::Force,
                reason: None,
            }),
        )
        .model;
        model = orishu_membership::update(
            model,
            Message::Local(Command::ClearTombstone { node: node.clone() }),
        )
        .model;
        let mut lifted = testing::blocklist_entry("lifted");
        lifted.action = orishu_membership::model::BlocklistAction::Allow;
        model = orishu_membership::update(
            model,
            Message::Local(Command::UpdateBlocklist(lifted.clone())),
        )
        .model;
        let frozen = Frozen::capture(&model, 10).unwrap();
        let records = Receiver::new(frozen.descriptor().clone())
            .unwrap()
            .push(frozen.page(0).unwrap())
            .unwrap()
            .finish()
            .unwrap();
        assert!(records.contains(&Record::Block(lifted)));
        assert!(records.iter().any(
            |record| matches!(record, Record::Tombstone(t) if t.node_id == node && t.cleared)
        ));
    }

    #[test]
    fn empty_baseline_is_explicit_and_mutation_cannot_change_frozen_pages() {
        let model = testing::standalone("source");
        let frozen = Frozen::capture(&model, 1).unwrap();
        assert_eq!(frozen.descriptor().pages, 1);
        let records = Receiver::new(frozen.descriptor().clone())
            .unwrap()
            .push(frozen.page(0).unwrap())
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(records, vec![Record::Policy(None)]);
        let baseline = Receiver::new(frozen.descriptor().clone())
            .unwrap()
            .push(frozen.page(0).unwrap())
            .unwrap()
            .finish_baseline()
            .unwrap();
        assert_eq!(baseline.formation_id, *model.formation());
        assert_eq!(baseline.snapshot, 1);
        let applied = orishu_membership::update(
            model.clone(),
            Message::Local(Command::InstallAdmissionBaseline(baseline)),
        );
        assert_eq!(applied.model, model);
        assert!(matches!(
            applied.effects.as_slice(),
            [orishu_membership::Effect::Publish(
                orishu_membership::ChangeRecord::AdmissionBaselineApplied {
                    self_removed: false,
                    ..
                }
            )]
        ));
        assert!(
            Receiver::new(frozen.descriptor().clone())
                .unwrap()
                .finish()
                .is_err()
        );
        let changed =
            orishu_membership::update(model, Message::Local(Command::SetMembershipLock(true)))
                .model;
        assert_ne!(
            Frozen::capture(&changed, 1).unwrap().descriptor().root,
            frozen.descriptor().root
        );
        assert_eq!(
            Receiver::new(frozen.descriptor().clone())
                .unwrap()
                .push(frozen.page(0).unwrap())
                .unwrap()
                .finish()
                .unwrap(),
            records
        );
    }

    #[test]
    fn pages_require_complete_ordered_identity_bound_content() {
        let mut model = testing::standalone("source");
        for index in 0..40 {
            model = orishu_membership::update(
                model,
                Message::Local(Command::UpdateBlocklist(testing::blocklist_entry(
                    &format!("blocked-{index:02}"),
                ))),
            )
            .model;
        }
        let frozen = Frozen::capture(&model, 8).unwrap();
        assert_eq!(frozen.descriptor().pages, 2);
        let receiver = || Receiver::new(frozen.descriptor().clone()).unwrap();
        assert!(receiver().push(frozen.page(1).unwrap()).is_err());
        assert!(
            receiver()
                .push(frozen.page(0).unwrap())
                .unwrap()
                .finish()
                .is_err()
        );
        assert!(
            receiver()
                .push(frozen.page(0).unwrap())
                .unwrap()
                .push(frozen.page(0).unwrap())
                .is_err()
        );
        assert_eq!(
            receiver()
                .push(frozen.page(0).unwrap())
                .unwrap()
                .push(frozen.page(1).unwrap())
                .unwrap()
                .finish()
                .unwrap()
                .len(),
            41
        );
        let other = Frozen::capture(&model, 9).unwrap();
        assert!(receiver().push(other.page(0).unwrap()).is_err());
        let mut descriptor = frozen.descriptor().clone();
        descriptor.root[0] ^= 1;
        assert!(
            Receiver::new(descriptor)
                .unwrap()
                .push(frozen.page(0).unwrap())
                .unwrap()
                .push(frozen.page(1).unwrap())
                .unwrap()
                .finish()
                .is_err()
        );
        assert!(receiver().push(&vec![0; MAX_PAGE_BYTES + 1]).is_err());
    }
}
