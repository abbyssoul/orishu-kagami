//! Worker-private, bounded durable operation history; not execution authority.
//!
//! Call on an IO lane, never the formation owner. A daemon coordinator must keep
//! completion tickets independently of client connections, reserve BEFORE reading
//! a workload/action, and finish Accepted/Applied only AFTER known publication.
//! Load serving uses the load profile; command serving remains separate work.
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use orishu::model::{node::NodeId, run_load::LoadReceipt};
use rustix::{
    fd::OwnedFd,
    fs::{AtFlags, FlockOperation},
};
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};

use crate::{
    credentials::{self, CredentialError},
    peer::codec,
};

mod record;
pub use record::ReceiptRecord;

/// Existing load journal and on-disk format, unchanged by command support.
pub type ReceiptStore = ReceiptJournal<LoadReceipt>;
/// Separate fixed-profile command journal; never reads or rewrites load records.
pub type CommandReceiptStore = ReceiptJournal<orishu::model::run_command::RunCommandReceipt>;
/// Load completion capability, retained for existing admission callers.
pub type CompletionTicket = JournalTicket<LoadReceipt>;
/// Exact load replay or newly reserved durable load intent.
pub type BeginLoad = BeginReceipt<LoadReceipt>;
/// Command completion capability, independent of load tickets and other stores.
pub type CommandCompletionTicket = JournalTicket<orishu::model::run_command::RunCommandReceipt>;
/// Exact command replay or newly reserved durable command intent.
pub type BeginCommand = BeginReceipt<orishu::model::run_command::RunCommandReceipt>;
pub(crate) type ReceiptHistory = JournalHistory<LoadReceipt>;

/// Lifetime history cap; no eviction may turn an old operation into new work.
pub const MAX_RECEIPTS: usize = 256;
/// Checked before reading/parsing an existing snapshot, including recovery.
pub const MAX_RECEIPT_BYTES: usize = 524_288;
const SNAPSHOT: &str = "workload-load-receipts.cbor";
const STAGING: &str = ".workload-load-receipts.pending";
const LOCK: &str = ".workload-load-receipts.lock";

/// Failures do not include arbitrary input strings or local paths.
#[derive(Debug, thiserror::Error)]
pub enum ReceiptStoreError {
    /// Local storage could not establish durability; reopening is required.
    #[error("operation receipt persistence failed")]
    Io(#[source] std::io::Error),
    /// Directory/file ownership, type or permissions are unsafe.
    #[error("unsafe operation receipt storage")]
    UnsafePath,
    /// Another instance holds the receipt writer lock.
    #[error("operation receipt storage already in use")]
    InUse,
    /// Existing bytes/shape/version/identity are not a supported bounded snapshot.
    #[error("invalid operation receipt snapshot")]
    Invalid,
    /// Same profile/formation/operation ID attempted different immutable intent.
    #[error("operation identity conflict")]
    Conflict,
    /// History is full. Replays still work; new requests are refused.
    #[error("operation receipt history capacity exhausted")]
    Full,
    /// Ticket is from another store/incarnation or its receipt is already final.
    #[error("operation receipt ticket is not pending in this store")]
    Ticket,
    /// Proposed acceptance does not identify the requested execution.
    #[error("operation receipt outcome identity mismatch")]
    Outcome,
    /// An earlier uncertain write prohibits further operations in this instance.
    #[error("operation receipt durability is uncertain; reopen storage")]
    Poisoned,
}

impl From<std::io::Error> for ReceiptStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<CredentialError> for ReceiptStoreError {
    fn from(error: CredentialError) -> Self {
        match error {
            CredentialError::Io(error) => Self::Io(error),
            _ => Self::UnsafePath,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(bound(deserialize = "R: Deserialize<'de>"))]
struct Snapshot<R> {
    api_version: String,
    #[serde(deserialize_with = "bounded_receipts")]
    receipts: Vec<R>,
}

fn bounded_receipts<'de, D: de::Deserializer<'de>, R: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<R>, D::Error> {
    struct Receipts<R>(std::marker::PhantomData<R>);
    impl<'de, R: Deserialize<'de>> Visitor<'de> for Receipts<R> {
        type Value = Vec<R>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("bounded operation receipts")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|length| length > MAX_RECEIPTS) {
                return Err(de::Error::custom("operation receipt count exceeded"));
            }
            let mut result = Vec::new();
            while result.len() < MAX_RECEIPTS {
                let Some(receipt) = seq.next_element()? else {
                    return Ok(result);
                };
                result.push(receipt);
            }
            // Refuse the next value without deserializing its body. CBOR also
            // receives the allocation-bounded preflight before this projection.
            struct Refuse;
            impl<'de> de::DeserializeSeed<'de> for Refuse {
                type Value = ();
                fn deserialize<D: de::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
                    Err(de::Error::custom("operation receipt count exceeded"))
                }
            }
            seq.next_element_seed(Refuse)?;
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Receipts(std::marker::PhantomData))
}

/// Nonserializable completion capability from one durable reservation.
/// Dropping it does not erase intent or authorize another admission.
pub struct JournalTicket<R> {
    incarnation: Arc<()>,
    receipt: R,
}
impl<R> JournalTicket<R> {
    /// Durable pending fact; this is NOT run acceptance.
    pub fn receipt(&self) -> &R {
        &self.receipt
    }
}

/// Replays never yield a fresh completion ticket or permission to execute.
pub enum BeginReceipt<R> {
    /// Durable pending reservation created exactly once.
    Started(JournalTicket<R>),
    /// Historical state of the exact same logical request.
    Replay(R),
}

/// Bounded read-only projection of the last known durable history. Only the IO
/// owner can publish a replacement; readers cannot issue completion tickets.
#[derive(Clone)]
pub(crate) struct JournalHistory<R>(Vec<R>);
impl<R: ReceiptRecord> JournalHistory<R> {
    pub(crate) fn lookup(&self, request: &R::Request) -> Result<Option<R>, ReceiptStoreError> {
        lookup(&self.0, request).map(|receipt| receipt.cloned())
    }
}

/// Single-writer Unix IO shell retaining bounded history across worker restarts.
pub struct ReceiptJournal<R: ReceiptRecord> {
    directory: OwnedFd,
    _lock: File,
    incarnation: Arc<()>,
    receipts: Vec<R>,
    poisoned: bool,
    #[cfg(test)]
    fail_at: Option<WriteStage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteStage {
    Created,
    Written,
    Synced,
    Renamed,
    DirectorySynced,
}

impl<R: ReceiptRecord> ReceiptJournal<R> {
    pub(crate) fn history(&self) -> Result<JournalHistory<R>, ReceiptStoreError> {
        if self.poisoned {
            return Err(ReceiptStoreError::Poisoned);
        }
        Ok(JournalHistory(self.receipts.clone()))
    }
    /// Open an existing owned/private directory (normally worker credentials).
    /// Corrupt committed files fail closed, never reset history. Pending records
    /// become Indeterminate durably before this instance is returned. A separate
    /// nonblocking writer lock allows coexistence with the credentials lock.
    pub fn open(path: &Path) -> Result<Self, ReceiptStoreError> {
        let directory = credentials::open_private_directory(path)?;
        let lock = credentials::open_private(&directory, R::LOCK, true)?;
        rustix::fs::flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(|error| {
            if error == rustix::io::Errno::WOULDBLOCK {
                ReceiptStoreError::InUse
            } else {
                ReceiptStoreError::Io(error.into())
            }
        })?;
        let receipts = match credentials::open_private(&directory, R::SNAPSHOT, false) {
            Ok(file) => {
                if file.metadata()?.len() > MAX_RECEIPT_BYTES as u64 {
                    return Err(ReceiptStoreError::Invalid);
                }
                let mut bytes = Vec::new();
                file.take(MAX_RECEIPT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > MAX_RECEIPT_BYTES {
                    return Err(ReceiptStoreError::Invalid);
                }
                let snapshot: Snapshot<R> = codec::decode_receipts(&bytes, R::FINITE_FLOATS)
                    .map_err(|_| ReceiptStoreError::Invalid)?;
                if snapshot.api_version != R::VERSION {
                    return Err(ReceiptStoreError::Invalid);
                }
                // Reject even identical duplicate keys: no file ordering may
                // decide which history is authoritative.
                let mut keys = std::collections::BTreeSet::new();
                for receipt in &snapshot.receipts {
                    if !keys.insert(R::key(receipt.request())) {
                        return Err(ReceiptStoreError::Invalid);
                    }
                }
                snapshot.receipts
            }
            Err(CredentialError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Vec::new()
            }
            Err(error) => return Err(error.into()),
        };
        // A staging name is not an acknowledged snapshot. Remove only this exact
        // owned/private regular single-link file while holding the writer lock.
        match credentials::open_private(&directory, R::STAGING, false) {
            Ok(_) => {
                rustix::fs::unlinkat(&directory, R::STAGING, AtFlags::empty())
                    .map_err(std::io::Error::from)?;
            }
            Err(CredentialError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut store = Self {
            directory,
            _lock: lock,
            incarnation: Arc::new(()),
            receipts,
            poisoned: false,
            #[cfg(test)]
            fail_at: None,
        };
        let recovered: Vec<_> = store.receipts.iter().map(ReceiptRecord::recover).collect();
        // Establish even an empty history durably before exposing a usable store.
        store.replace(recovered)?;
        Ok(store)
    }

    /// Read exact retry state. Conflict is checked even at capacity. Callers must
    /// authenticate first; this store does not make a network authorization choice.
    pub fn lookup(&self, request: &R::Request) -> Result<Option<&R>, ReceiptStoreError> {
        if self.poisoned {
            return Err(ReceiptStoreError::Poisoned);
        }
        lookup(&self.receipts, request)
    }

    /// Persist intent before input/admission or run control. History replays must precede current
    /// formation eligibility checks; eligibility for NEW work remains the owner's
    /// responsibility. The passed source node is ignored for historical replays.
    pub fn begin(
        &mut self,
        request: R::Request,
        source_node_id: NodeId,
    ) -> Result<BeginReceipt<R>, ReceiptStoreError> {
        if let Some(receipt) = self.lookup(&request)? {
            return Ok(BeginReceipt::Replay(receipt.clone()));
        }
        if self.receipts.len() == MAX_RECEIPTS {
            return Err(ReceiptStoreError::Full);
        }
        let receipt = R::pending(request, source_node_id);
        let mut next = self.receipts.clone();
        next.push(receipt.clone());
        self.replace(next)?;
        Ok(BeginReceipt::Started(JournalTicket {
            incarnation: Arc::clone(&self.incarnation),
            receipt,
        }))
    }

    /// Record an outcome exactly once. Accepted/Applied requires known owner
    /// publication; uncertain publication must use Indeterminate, never Refused.
    /// IO errors poison this instance, since even a failed fsync may have exposed
    /// a new snapshot. A final receipt does not retain or restore a live RunHandle.
    pub fn finish(
        &mut self,
        ticket: &JournalTicket<R>,
        outcome: R::Outcome,
    ) -> Result<R, ReceiptStoreError> {
        if self.poisoned {
            return Err(ReceiptStoreError::Poisoned);
        }
        if !Arc::ptr_eq(&self.incarnation, &ticket.incarnation) {
            return Err(ReceiptStoreError::Ticket);
        }
        let index = self
            .receipts
            .iter()
            .position(|receipt| receipt == &ticket.receipt && receipt.is_pending())
            .ok_or(ReceiptStoreError::Ticket)?;
        let receipt = ticket.receipt.finished(outcome)?;
        let mut next = self.receipts.clone();
        next[index] = receipt.clone();
        self.replace(next)?;
        Ok(receipt)
    }

    fn replace(&mut self, receipts: Vec<R>) -> Result<(), ReceiptStoreError> {
        if receipts.len() > MAX_RECEIPTS {
            return Err(ReceiptStoreError::Full);
        }
        let bytes = codec::encode_receipts(
            &Snapshot {
                api_version: R::VERSION.into(),
                receipts: receipts.clone(),
            },
            R::FINITE_FLOATS,
        )
        .map_err(|_| ReceiptStoreError::Invalid)?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(ReceiptStoreError::Invalid);
        }
        if let Err(error) = self.write(&bytes) {
            self.poisoned = true;
            return Err(error);
        }
        self.receipts = receipts;
        Ok(())
    }

    fn write(&self, bytes: &[u8]) -> Result<(), ReceiptStoreError> {
        let mut file = credentials::create_private(&self.directory, R::STAGING)?;
        self.checkpoint(WriteStage::Created)?;
        file.write_all(bytes)?;
        self.checkpoint(WriteStage::Written)?;
        file.sync_all()?;
        self.checkpoint(WriteStage::Synced)?;
        rustix::fs::renameat(&self.directory, R::STAGING, &self.directory, R::SNAPSHOT)
            .map_err(std::io::Error::from)?;
        self.checkpoint(WriteStage::Renamed)?;
        rustix::fs::fsync(&self.directory).map_err(std::io::Error::from)?;
        self.checkpoint(WriteStage::DirectorySynced)?;
        Ok(())
    }

    fn checkpoint(&self, _stage: WriteStage) -> Result<(), ReceiptStoreError> {
        #[cfg(test)]
        if self.fail_at == Some(_stage) {
            return Err(std::io::Error::other("injected receipt write failure").into());
        }
        Ok(())
    }
}

fn lookup<'a, R: ReceiptRecord>(
    receipts: &'a [R],
    request: &R::Request,
) -> Result<Option<&'a R>, ReceiptStoreError> {
    let receipt = receipts
        .iter()
        .find(|receipt| R::key(receipt.request()) == R::key(request));
    if receipt.is_some_and(|receipt| receipt.request() != request) {
        return Err(ReceiptStoreError::Conflict);
    }
    Ok(receipt)
}

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod tests;
