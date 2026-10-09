//! Durable client run intent: the pure ledger and its strict versioned codec.
//!
//! The ledger records what this Kagami instance may have sent to a worker, so a
//! restart reconciles the same operation instead of guessing its outcome. It
//! holds no credentials, credential paths or workload bytes; the IO shell stores
//! the frozen bytes separately under the request's workload digest.
//!
//! Every transition returns a new ledger and leaves the old one unchanged. The
//! shell adopts the new ledger only after its encoded bytes are durable, and it
//! sends nothing to a worker when that write fails (write-ahead intent).
use orishu::{
    client::ClusterAddress,
    model::{cluster::FormationId, run_command::*, run_load::*},
};
use serde::{Deserialize, Serialize};

/// Format identifier of the persisted ledger.
pub const API_VERSION: &str = "kagami.run-intents/v1";
/// Encoded size limit, checked before parsing and after encoding.
pub const MAX_LEDGER_BYTES: usize = 64 * 1024;

/// Refused ledger transition. The text is suitable for a window notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentError {
    /// A submission is already recorded; one submission at a time.
    LoadRetained,
    /// A command is already recorded; one command at a time.
    CommandRetained,
    /// No submission is recorded.
    NoLoad,
    /// No command is recorded.
    NoCommand,
    /// The request names a different formation than its target.
    TargetMismatch,
    /// The receipt belongs to another operation or request.
    ForeignReceipt,
    /// A final receipt is already recorded and the new one differs.
    ConflictingReceipt,
    /// The submission outcome is not final, so the record must stay.
    Unresolved,
    /// The worker address has no exact persisted form, for example a socket
    /// path that is not UTF-8.
    Address,
    /// The encoded ledger exceeds [`MAX_LEDGER_BYTES`].
    Oversized,
}
impl std::fmt::Display for IntentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LoadRetained => {
                "A previous submission is still recorded. Reconcile or clear it first."
            }
            Self::CommandRetained => {
                "A previous run command is still recorded. Reconcile it first."
            }
            Self::NoLoad => "No submission is recorded.",
            Self::NoCommand => "No run command is recorded.",
            Self::TargetMismatch => "The request formation does not match its worker target.",
            Self::ForeignReceipt => "The worker receipt does not belong to the recorded operation.",
            Self::ConflictingReceipt => {
                "The worker reported a different final outcome for a recorded operation."
            }
            Self::Unresolved => {
                "The submission outcome is not final. Reconcile it before clearing it."
            }
            Self::Address => "This worker address cannot be recorded for recovery.",
            Self::Oversized => "The run intent record exceeds its size limit.",
        })
    }
}
impl std::error::Error for IntentError {}

/// Rejected persisted ledger. Never reset or overwrite a rejected file: it can
/// hold the only record of an operation that a worker applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The file exceeds [`MAX_LEDGER_BYTES`].
    Oversized,
    /// The bytes are not a well-formed ledger.
    Malformed,
    /// Another Kagami version wrote the file.
    UnsupportedVersion,
    /// The fields are well-formed but contradict each other.
    Inconsistent,
}
impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Oversized => "The run intent record exceeds its size limit.",
            Self::Malformed => "The run intent record is damaged.",
            Self::UnsupportedVersion => {
                "The run intent record was written by an unsupported Kagami version."
            }
            Self::Inconsistent => "The run intent record contradicts itself.",
        })
    }
}
impl std::error::Error for DecodeError {}

/// Worker endpoint and formation that received a request. Operation IDs are
/// scoped to one worker and formation, so a recovered intent is reconciled only
/// against the same target. Another entry node could report it as absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    address: ClusterAddress,
    formation: FormationId,
}
impl Target {
    /// Refuses an address that does not have an exact persisted form.
    pub fn new(address: ClusterAddress, formation: FormationId) -> Result<Self, IntentError> {
        persisted_address(&address).ok_or(IntentError::Address)?;
        Ok(Self { address, formation })
    }
    pub fn address(&self) -> &ClusterAddress {
        &self.address
    }
    pub fn formation(&self) -> &FormationId {
        &self.formation
    }
}

/// `unix:` keeps relative socket paths unambiguous. The text must parse back to
/// the same address, so a reader never connects to a different endpoint.
fn persisted_address(address: &ClusterAddress) -> Option<String> {
    let text = match address {
        ClusterAddress::UnixSocket(path) => format!("unix:{}", path.to_str()?),
        other => other.to_string(),
    };
    (text.parse::<ClusterAddress>().ok()? == *address).then_some(text)
}

/// Document incarnation and revision that produced a submitted workload. This
/// is lineage for display, never workload identity or editable run state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub incarnation: uuid::Uuid,
    pub revision: u64,
}

/// Recorded upload and its last known receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadIntent {
    target: Target,
    request: LoadRequest,
    source: Source,
    receipt: Option<LoadReceipt>,
}
impl LoadIntent {
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn request(&self) -> &LoadRequest {
        &self.request
    }
    pub fn source(&self) -> Source {
        self.source
    }
    pub fn receipt(&self) -> Option<&LoadReceipt> {
        self.receipt.as_ref()
    }
    /// Accepted or refused. Pending, indeterminate and absent outcomes are not
    /// final: the operation can still have published, or can still publish.
    pub fn is_final(&self) -> bool {
        self.receipt.as_ref().is_some_and(final_load)
    }
}
fn final_load(receipt: &LoadReceipt) -> bool {
    matches!(
        receipt.state(),
        LoadState::Finished(LoadOutcome::Accepted { .. } | LoadOutcome::Refused { .. })
    )
}

/// Recorded run command and its last known unresolved receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandIntent {
    target: Target,
    request: RunCommandRequest,
    receipt: Option<RunCommandReceipt>,
}
impl CommandIntent {
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn request(&self) -> &RunCommandRequest {
        &self.request
    }
    pub fn receipt(&self) -> Option<&RunCommandReceipt> {
        self.receipt.as_ref()
    }
}
fn final_command(receipt: &RunCommandReceipt) -> bool {
    matches!(
        receipt.state(),
        RunCommandState::Finished(
            RunCommandOutcome::Applied { .. } | RunCommandOutcome::Refused { .. }
        )
    )
}

/// At most one submission and one command. A submission stays until it is
/// final and explicitly cleared; a command is removed when its outcome is final.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ledger {
    load: Option<LoadIntent>,
    command: Option<CommandIntent>,
}
impl Ledger {
    pub fn load(&self) -> Option<&LoadIntent> {
        self.load.as_ref()
    }
    pub fn command(&self) -> Option<&CommandIntent> {
        self.command.as_ref()
    }
    pub fn is_empty(&self) -> bool {
        self.load.is_none() && self.command.is_none()
    }

    /// Record an upload before any byte is sent.
    pub fn begin_load(
        &self,
        target: Target,
        request: LoadRequest,
        source: Source,
    ) -> Result<Self, IntentError> {
        if self.load.is_some() {
            return Err(IntentError::LoadRetained);
        }
        if request.formation_id() != target.formation() {
            return Err(IntentError::TargetMismatch);
        }
        Ok(Self {
            load: Some(LoadIntent {
                target,
                request,
                source,
                receipt: None,
            }),
            command: self.command.clone(),
        })
    }

    /// Record a submit or lookup reply. `None` means that the worker has no
    /// record of the operation. A final receipt is immutable.
    pub fn record_load(&self, receipt: Option<LoadReceipt>) -> Result<Self, IntentError> {
        let intent = self.load.as_ref().ok_or(IntentError::NoLoad)?;
        if receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request() != &intent.request)
        {
            return Err(IntentError::ForeignReceipt);
        }
        if intent.is_final() && receipt != intent.receipt {
            return Err(IntentError::ConflictingReceipt);
        }
        Ok(Self {
            load: Some(LoadIntent {
                receipt,
                ..intent.clone()
            }),
            command: self.command.clone(),
        })
    }

    /// Forget a final submission. This changes no worker state.
    pub fn clear_load(&self) -> Result<Self, IntentError> {
        let intent = self.load.as_ref().ok_or(IntentError::NoLoad)?;
        if !intent.is_final() {
            return Err(IntentError::Unresolved);
        }
        Ok(Self {
            load: None,
            command: self.command.clone(),
        })
    }

    /// Record a run command before it is sent.
    pub fn begin_command(
        &self,
        target: Target,
        request: RunCommandRequest,
    ) -> Result<Self, IntentError> {
        if self.command.is_some() {
            return Err(IntentError::CommandRetained);
        }
        if request.run().formation_id() != target.formation() {
            return Err(IntentError::TargetMismatch);
        }
        Ok(Self {
            load: self.load.clone(),
            command: Some(CommandIntent {
                target,
                request,
                receipt: None,
            }),
        })
    }

    /// Record a command or lookup reply. A final outcome removes the command:
    /// the worker keeps the receipt, so a lookup after a lost write returns it.
    pub fn record_command(&self, receipt: Option<RunCommandReceipt>) -> Result<Self, IntentError> {
        let intent = self.command.as_ref().ok_or(IntentError::NoCommand)?;
        if receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request() != &intent.request)
        {
            return Err(IntentError::ForeignReceipt);
        }
        let command = (!receipt.as_ref().is_some_and(final_command)).then(|| CommandIntent {
            receipt,
            ..intent.clone()
        });
        Ok(Self {
            load: self.load.clone(),
            command,
        })
    }

    /// Canonical bytes for durable storage.
    pub fn encode(&self) -> Result<Vec<u8>, IntentError> {
        let wire = LedgerWire {
            api_version: API_VERSION.into(),
            load: self.load.as_ref().map(|intent| LoadWire {
                target: TargetWire::from(&intent.target),
                request: intent.request.clone(),
                source: SourceWire {
                    incarnation: intent.source.incarnation.hyphenated().to_string(),
                    revision: intent.source.revision,
                },
                receipt: intent.receipt.clone(),
            }),
            command: self.command.as_ref().map(|intent| CommandWire {
                target: TargetWire::from(&intent.target),
                request: intent.request.clone(),
                receipt: intent.receipt.clone(),
            }),
        };
        let bytes = serde_json::to_vec_pretty(&wire).expect("ledger serialization is infallible");
        if bytes.len() > MAX_LEDGER_BYTES {
            return Err(IntentError::Oversized);
        }
        Ok(bytes)
    }

    /// Strict decode: unknown fields, duplicate fields, a different version and
    /// contradictory records are all refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() > MAX_LEDGER_BYTES {
            return Err(DecodeError::Oversized);
        }
        // Identify the version first, so that a file from a newer Kagami is
        // reported as such and not as damage.
        #[derive(Deserialize)]
        struct Probe {
            #[serde(rename = "apiVersion")]
            api_version: String,
        }
        let probe: Probe = serde_json::from_slice(bytes).map_err(|_| DecodeError::Malformed)?;
        if probe.api_version != API_VERSION {
            return Err(DecodeError::UnsupportedVersion);
        }
        let wire: LedgerWire = serde_json::from_slice(bytes).map_err(|_| DecodeError::Malformed)?;
        let load = match wire.load {
            None => None,
            Some(load) => {
                let incarnation = uuid::Uuid::try_parse(&load.source.incarnation)
                    .ok()
                    .filter(|id| id.hyphenated().to_string() == load.source.incarnation)
                    .ok_or(DecodeError::Malformed)?;
                Some(LoadIntent {
                    target: load.target.try_into()?,
                    request: load.request,
                    source: Source {
                        incarnation,
                        revision: load.source.revision,
                    },
                    receipt: load.receipt,
                })
            }
        };
        let command = match wire.command {
            None => None,
            Some(command) => Some(CommandIntent {
                target: command.target.try_into()?,
                request: command.request,
                receipt: command.receipt,
            }),
        };
        // Rebuild through the same transitions that produced the file.
        let mut ledger = Self::default();
        if let Some(intent) = load {
            ledger = ledger
                .begin_load(intent.target, intent.request, intent.source)
                .and_then(|ledger| ledger.record_load(intent.receipt))
                .map_err(|_| DecodeError::Inconsistent)?;
        }
        if let Some(intent) = command {
            if intent.receipt.as_ref().is_some_and(final_command) {
                return Err(DecodeError::Inconsistent);
            }
            ledger = ledger
                .begin_command(intent.target, intent.request)
                .and_then(|ledger| ledger.record_command(intent.receipt))
                .map_err(|_| DecodeError::Inconsistent)?;
        }
        Ok(ledger)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LedgerWire {
    api_version: String,
    load: Option<LoadWire>,
    command: Option<CommandWire>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetWire {
    address: String,
    formation_id: FormationId,
}
impl From<&Target> for TargetWire {
    fn from(target: &Target) -> Self {
        Self {
            address: persisted_address(&target.address).expect("checked by Target::new"),
            formation_id: target.formation.clone(),
        }
    }
}
impl TryFrom<TargetWire> for Target {
    type Error = DecodeError;
    fn try_from(wire: TargetWire) -> Result<Self, DecodeError> {
        let address: ClusterAddress = wire.address.parse().map_err(|_| DecodeError::Malformed)?;
        if persisted_address(&address).as_deref() != Some(wire.address.as_str()) {
            return Err(DecodeError::Malformed);
        }
        Ok(Self {
            address,
            formation: wire.formation_id,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceWire {
    incarnation: String,
    revision: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoadWire {
    target: TargetWire,
    request: LoadRequest,
    source: SourceWire,
    receipt: Option<LoadReceipt>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandWire {
    target: TargetWire,
    request: RunCommandRequest,
    receipt: Option<RunCommandReceipt>,
}

#[cfg(test)]
mod tests;
