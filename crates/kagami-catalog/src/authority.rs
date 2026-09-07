//! The one catalog authority.
//!
//! ADR 0008 gives the catalog a single authority that owns the loaded
//! registry, the catalog revision, validation diagnostics, and every file
//! effect. UI and MCP are adapters over *this* type: they submit the same
//! [`CatalogCommand`] values and read the same projection, and neither writes
//! a catalog file or keeps a competing registry of its own.
//!
//! Three properties make that boundary useful rather than ceremonial:
//!
//! - **Guarded.** A command may name the [`CatalogRevision`] it was composed
//!   against; a write may name the file digest it read. Either mismatch is a
//!   refusal, so two adapters editing concurrently cannot silently clobber
//!   each other.
//! - **Idempotent.** A command carries a [`CommandId`]; resubmitting one that
//!   already succeeded replays its recorded outcome instead of applying the
//!   change twice. A *failed* command is not recorded, so a retry after a
//!   transient error still runs.
//! - **Observable.** Every accepted command appends one bounded
//!   [`CatalogEvent`], so a view can catch up from the revision it last saw
//!   rather than re-reading everything.
//!
//! Catalog edits advance only the catalog revision. Nothing here touches an
//! experiment: reloading changes this projection and nothing else.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::diagnostic::Diagnostic;
use crate::document::TemplateDocument;
use crate::entry::{CatalogEntry, CatalogSet, CatalogSummary, LoadResult};
use crate::limits::Limits;
use crate::load::{load_directory, parse_stream};
use crate::materialize::{InstantiationError, InstantiationRequest, ObjectCandidate, materialize};
use crate::name::{ComponentTypeId, ParameterName};
use crate::schema::SchemaRegistry;
use crate::source::{ContentFingerprint, SourceLocation, TemplateIdentity};
use crate::template::Template;
use crate::write::{self, CatalogRoot, WriteError, WriteTarget};

/// How many accepted commands are remembered for idempotent replay.
pub const MAX_COMMAND_HISTORY: usize = 256;

/// How many change events are retained for catch-up.
pub const MAX_EVENT_HISTORY: usize = 256;

/// Longest accepted [`CommandId`] or [`ActorId`], in bytes.
pub const MAX_IDENTITY_BYTES: usize = 128;

/// A monotonic counter over accepted catalog changes.
///
/// Distinct from an experiment revision by construction: nothing in this
/// crate can advance a document, and a catalog reload advances only this.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogRevision(u64);

impl CatalogRevision {
    /// The revision of a freshly loaded catalog.
    pub const INITIAL: Self = Self(0);

    /// The next revision.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The underlying counter.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CatalogRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "r{}", self.0)
    }
}

/// Why a bounded caller-supplied identity was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    /// The text was empty.
    #[error("identity must not be empty")]
    Empty,
    /// The text was longer than [`MAX_IDENTITY_BYTES`].
    #[error("identity is {found} bytes, over the limit of {limit}")]
    TooLong { found: usize, limit: usize },
}

macro_rules! bounded_identity {
    ($(#[$meta:meta])* $type_name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type_name(String);

        impl $type_name {
            /// Validate and construct the identity.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentityError::Empty);
                }
                if value.len() > MAX_IDENTITY_BYTES {
                    return Err(IdentityError::TooLong {
                        found: value.len(),
                        limit: MAX_IDENTITY_BYTES,
                    });
                }
                Ok(Self(value))
            }

            /// The identity's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $type_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

bounded_identity!(
    /// Caller-chosen identity of one submitted command, used to make a
    /// resubmission idempotent rather than duplicating its effect.
    CommandId
);
bounded_identity!(
    /// Who submitted a command. Recorded on the resulting event so a change
    /// can be attributed; never used for authorization here.
    ActorId
);

/// A typed catalog command. Both the UI and MCP adapters build these.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogCommand {
    /// Re-read the catalog directory from disk.
    Reload,
    /// Add a template to `file`, creating the file if it does not exist.
    Create {
        /// Catalog-root-relative file to write into.
        file: PathBuf,
        /// The template to add.
        document: Box<TemplateDocument>,
    },
    /// Replace exactly one document.
    Update {
        /// Which document, and the identity it must currently carry.
        target: WriteTarget,
        /// Its replacement.
        document: Box<TemplateDocument>,
    },
    /// Remove exactly one document.
    Delete {
        /// Which document, and the identity it must currently carry.
        target: WriteTarget,
    },
    /// Check authored text without writing anything.
    Validate {
        /// The candidate document stream.
        text: String,
    },
}

/// A command together with the guards and attribution it was submitted with.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogCommandEnvelope {
    /// Identity of this submission, for idempotent replay.
    pub command_id: CommandId,
    /// Who submitted it.
    pub actor: ActorId,
    /// The revision the caller composed the command against. `None` opts out
    /// of the guard.
    pub expected_revision: Option<CatalogRevision>,
    /// The command itself.
    pub command: CatalogCommand,
}

impl CatalogCommandEnvelope {
    /// An unguarded envelope.
    pub fn new(command_id: CommandId, actor: ActorId, command: CatalogCommand) -> Self {
        Self {
            command_id,
            actor,
            expected_revision: None,
            command,
        }
    }

    /// Guard this command with the revision the caller read.
    #[must_use]
    pub fn guarded_by(mut self, revision: CatalogRevision) -> Self {
        self.expected_revision = Some(revision);
        self
    }
}

/// What one accepted command did.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogOutcome {
    /// The catalog was re-read.
    Reloaded {
        /// The revision after the change.
        revision: CatalogRevision,
        /// Entry counts by state.
        summary: CatalogSummary,
    },
    /// A template was added.
    Created {
        /// The revision after the change.
        revision: CatalogRevision,
        /// The template that now exists.
        identity: TemplateIdentity,
        /// Its content fingerprint, when it loaded back as valid.
        fingerprint: Option<ContentFingerprint>,
    },
    /// A template was replaced.
    Updated {
        /// The revision after the change.
        revision: CatalogRevision,
        /// The identity the replaced document carried. Equal to `current`
        /// unless the edit renamed the template.
        previous: TemplateIdentity,
        /// The identity it now carries.
        current: TemplateIdentity,
        /// Its new content fingerprint.
        fingerprint: Option<ContentFingerprint>,
    },
    /// A template was removed.
    Deleted {
        /// The revision after the change.
        revision: CatalogRevision,
        /// The template that was removed.
        identity: TemplateIdentity,
    },
    /// Authored text was checked. The catalog is unchanged, so the revision
    /// is the one that was already current.
    Validated {
        /// The unchanged revision.
        revision: CatalogRevision,
        /// One report per document in the submitted text.
        reports: Vec<ValidationReport>,
    },
}

impl CatalogOutcome {
    /// The revision in force after this outcome.
    pub fn revision(&self) -> CatalogRevision {
        match self {
            CatalogOutcome::Reloaded { revision, .. }
            | CatalogOutcome::Created { revision, .. }
            | CatalogOutcome::Updated { revision, .. }
            | CatalogOutcome::Deleted { revision, .. }
            | CatalogOutcome::Validated { revision, .. } => *revision,
        }
    }
}

/// What checking one authored document found.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidationReport {
    /// The identity the document claims, when its names validated.
    pub identity: Option<TemplateIdentity>,
    /// Problems found. Empty means the document is structurally valid.
    pub diagnostics: Vec<Diagnostic>,
}

/// Why a command was refused. A refusal changes nothing.
#[derive(Debug, Error)]
pub enum CatalogRejection {
    /// The catalog moved on since the caller read it.
    #[error("catalog is at {actual}, not the expected {expected}")]
    RevisionConflict {
        expected: CatalogRevision,
        actual: CatalogRevision,
    },
    /// The submitted document is not structurally valid.
    #[error("submitted template is not valid")]
    InvalidDocument {
        /// What is wrong and where. Never empty.
        diagnostics: Vec<Diagnostic>,
    },
    /// Another entry already owns the identity being created.
    #[error("template `{identity}` already exists at {existing}")]
    DuplicateIdentity {
        identity: TemplateIdentity,
        existing: SourceLocation,
    },
    /// The file effect could not be applied.
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// One accepted change, for a view catching up.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEvent {
    /// The revision this change produced.
    pub revision: CatalogRevision,
    /// Who made it.
    pub actor: ActorId,
    /// What changed.
    pub change: CatalogChange,
}

/// What an accepted command changed.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogChange {
    /// The whole projection was replaced from disk.
    Reloaded {
        /// Entry counts by state after the reload.
        summary: CatalogSummary,
    },
    /// A template was added.
    Created(TemplateIdentity),
    /// A template was replaced, possibly under a new identity.
    ///
    /// Both identities are published because a rename is two things to a
    /// cached view: a row to remove and a row to insert. A consumer that drops
    /// `previous` and inserts `current` converges for an in-place edit too,
    /// where the two are equal.
    Updated {
        /// The identity the replaced document carried.
        previous: TemplateIdentity,
        /// The identity it now carries.
        current: TemplateIdentity,
    },
    /// A template was removed.
    Deleted(TemplateIdentity),
}

/// A read-model row: everything a browser or an MCP listing needs about one
/// entry, without exposing the loaded types themselves.
#[derive(Clone, Debug, PartialEq)]
pub struct EntrySummary {
    /// The entry's identity, when its names validated.
    pub identity: Option<TemplateIdentity>,
    /// Where it came from.
    pub source: SourceLocation,
    /// Its content fingerprint, for a guarded edit or instantiation.
    pub fingerprint: Option<ContentFingerprint>,
    /// `"available"`, `"unavailable"`, or `"invalid"`.
    pub state: &'static str,
    /// One-line human description.
    pub description: Option<String>,
    /// The component types it composes.
    pub components: Vec<ComponentTypeId>,
    /// The parameters an instantiation may bind.
    pub parameters: Vec<ParameterName>,
    /// Rendered diagnostics, for an invalid entry.
    pub diagnostics: Vec<String>,
    /// Rendered unavailability reasons, naming the missing plugin or
    /// dependency.
    pub unavailable: Vec<String>,
}

impl EntrySummary {
    fn of(entry: &CatalogEntry) -> Self {
        let template = entry.result.template();
        Self {
            identity: entry.identity.clone(),
            source: entry.source.clone(),
            fingerprint: entry.fingerprint,
            state: entry.result.state(),
            description: template.and_then(|template| template.metadata.description.clone()),
            components: template
                .map(|template| {
                    template
                        .spec
                        .components
                        .iter()
                        .map(|component| component.type_id.clone())
                        .collect()
                })
                .unwrap_or_default(),
            parameters: template
                .map(|template| template.spec.parameters.keys().cloned().collect())
                .unwrap_or_default(),
            diagnostics: match &entry.result {
                LoadResult::Invalid { diagnostics } => {
                    diagnostics.iter().map(ToString::to_string).collect()
                }
                _ => Vec::new(),
            },
            unavailable: match &entry.result {
                LoadResult::Unavailable { reasons, .. } => {
                    reasons.iter().map(ToString::to_string).collect()
                }
                _ => Vec::new(),
            },
        }
    }
}

/// A read request. Queries never change the revision, which is why they are
/// separate from [`CatalogCommand`].
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogQuery {
    /// Every entry.
    List,
    /// One entry by identity.
    Get(TemplateIdentity),
}

/// The answer to a [`CatalogQuery`].
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogView {
    /// Every entry, in load order.
    List {
        /// The revision this view was taken at.
        revision: CatalogRevision,
        /// The entries.
        entries: Vec<EntrySummary>,
    },
    /// One entry, or `None` when no entry owns that identity.
    Entry {
        /// The revision this view was taken at.
        revision: CatalogRevision,
        /// The entry.
        entry: Option<Box<EntrySummary>>,
    },
}

/// The owner of the loaded catalog, its revision, and its file effects.
pub struct CatalogAuthority {
    root: CatalogRoot,
    registry: SchemaRegistry,
    limits: Limits,
    set: CatalogSet,
    revision: CatalogRevision,
    events: VecDeque<CatalogEvent>,
    accepted: BTreeMap<CommandId, CatalogOutcome>,
    accepted_order: VecDeque<CommandId>,
}

impl fmt::Debug for CatalogAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogAuthority")
            .field("root", &self.root.path())
            .field("revision", &self.revision)
            .field("summary", &self.set.summary())
            .finish_non_exhaustive()
    }
}

impl CatalogAuthority {
    /// Load `root` and take ownership of it.
    ///
    /// A missing or unreadable directory is an empty catalog, not an error:
    /// Kagami must never fail to start over catalog state.
    pub fn open(root: impl Into<PathBuf>, registry: SchemaRegistry, limits: Limits) -> Self {
        let root = CatalogRoot::new(root);
        let set = load_directory(root.path(), &registry, &limits);
        Self {
            root,
            registry,
            limits,
            set,
            revision: CatalogRevision::INITIAL,
            events: VecDeque::new(),
            accepted: BTreeMap::new(),
            accepted_order: VecDeque::new(),
        }
    }

    /// The catalog directory this authority owns.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The current revision.
    pub fn revision(&self) -> CatalogRevision {
        self.revision
    }

    /// The currently loaded snapshot.
    pub fn set(&self) -> &CatalogSet {
        &self.set
    }

    /// The installed component schemas this catalog is resolved against.
    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }

    /// Retained change events, oldest first.
    pub fn events(&self) -> impl Iterator<Item = &CatalogEvent> {
        self.events.iter()
    }

    /// Events after `revision`, for a view catching up.
    ///
    /// A view whose last seen revision has already been evicted from the
    /// bounded history gets every retained event; it should treat that as a
    /// full refresh rather than assume continuity.
    pub fn events_since(&self, revision: CatalogRevision) -> impl Iterator<Item = &CatalogEvent> {
        self.events
            .iter()
            .filter(move |event| event.revision > revision)
    }

    /// Answer a read request.
    pub fn query(&self, query: &CatalogQuery) -> CatalogView {
        match query {
            CatalogQuery::List => CatalogView::List {
                revision: self.revision,
                entries: self.set.entries().iter().map(EntrySummary::of).collect(),
            },
            CatalogQuery::Get(identity) => CatalogView::Entry {
                revision: self.revision,
                entry: self
                    .set
                    .get(identity)
                    .map(|entry| Box::new(EntrySummary::of(entry))),
            },
        }
    }

    /// Materialise a template from the current snapshot.
    ///
    /// The catalog is not changed and its revision does not advance: the
    /// returned candidate is intent for the *document* authority to accept,
    /// which is what keeps a catalog edit from dirtying an experiment and an
    /// instantiation from advancing the catalog.
    pub fn instantiate(
        &self,
        request: &InstantiationRequest,
    ) -> Result<ObjectCandidate, InstantiationError> {
        materialize(&self.set, &self.registry, request)
    }

    /// Decide one command.
    ///
    /// Either the command is applied in full and the revision advances, or
    /// nothing changes at all.
    pub fn submit(
        &mut self,
        envelope: CatalogCommandEnvelope,
    ) -> Result<CatalogOutcome, CatalogRejection> {
        if let Some(recorded) = self.accepted.get(&envelope.command_id) {
            return Ok(recorded.clone());
        }
        if let Some(expected) = envelope.expected_revision
            && expected != self.revision
        {
            return Err(CatalogRejection::RevisionConflict {
                expected,
                actual: self.revision,
            });
        }

        let outcome = self.apply(&envelope)?;
        self.remember(envelope.command_id, outcome.clone());
        Ok(outcome)
    }

    fn apply(
        &mut self,
        envelope: &CatalogCommandEnvelope,
    ) -> Result<CatalogOutcome, CatalogRejection> {
        match &envelope.command {
            CatalogCommand::Validate { text } => Ok(CatalogOutcome::Validated {
                revision: self.revision,
                reports: self.validate(text),
            }),
            CatalogCommand::Reload => {
                self.reload();
                let summary = self.set.summary();
                let revision = self.record(&envelope.actor, CatalogChange::Reloaded { summary });
                Ok(CatalogOutcome::Reloaded { revision, summary })
            }
            CatalogCommand::Create { file, document } => {
                let template = self.validated(document)?;
                self.check_identity_available(&template.identity, None)?;
                let digest = write::file_digest(&self.root, file)?;
                write::create_entry(&self.root, file, &template, digest)?;
                self.reload();
                let revision = self.record(
                    &envelope.actor,
                    CatalogChange::Created(template.identity.clone()),
                );
                Ok(CatalogOutcome::Created {
                    revision,
                    fingerprint: self.fingerprint_of(&template.identity),
                    identity: template.identity,
                })
            }
            CatalogCommand::Update { target, document } => {
                let template = self.validated(document)?;
                self.check_identity_available(&template.identity, Some(&target.identity))?;
                let digest = self.digest_of(&target.file)?;
                write::update_entry(&self.root, target, &template, digest)?;
                self.reload();
                // The writer verified that the target document carried this
                // identity before replacing it, so it is what the edit
                // retired, not merely what the caller believed.
                let previous = target.identity.clone();
                let revision = self.record(
                    &envelope.actor,
                    CatalogChange::Updated {
                        previous: previous.clone(),
                        current: template.identity.clone(),
                    },
                );
                Ok(CatalogOutcome::Updated {
                    revision,
                    previous,
                    fingerprint: self.fingerprint_of(&template.identity),
                    current: template.identity,
                })
            }
            CatalogCommand::Delete { target } => {
                let digest = self.digest_of(&target.file)?;
                write::delete_entry(&self.root, target, digest)?;
                self.reload();
                let identity = target.identity.clone();
                let revision =
                    self.record(&envelope.actor, CatalogChange::Deleted(identity.clone()));
                Ok(CatalogOutcome::Deleted { revision, identity })
            }
        }
    }

    fn validate(&self, text: &str) -> Vec<ValidationReport> {
        parse_stream(Path::new("<submitted>"), text, &self.limits)
            .documents
            .into_iter()
            .map(|document| ValidationReport {
                identity: document.identity,
                diagnostics: document.outcome.err().unwrap_or_default(),
            })
            .collect()
    }

    fn validated(&self, document: &TemplateDocument) -> Result<Template, CatalogRejection> {
        Template::from_document(document, &self.limits)
            .map_err(|diagnostics| CatalogRejection::InvalidDocument { diagnostics })
    }

    /// Refuse a proposed identity another document already claims.
    ///
    /// `retiring` is the identity this command is replacing, when there is
    /// one. A command that proposes the identity it is already replacing is
    /// not a rename and claims no new name, so it stays legal; the writer's
    /// own target check is what proves the document really carries it.
    ///
    /// An invalid or unavailable entry still reserves the identity its
    /// metadata names. Preserving broken user data is a promise this crate
    /// makes; it must not turn into a rule that name ownership depends on
    /// which claimant happened to validate, because then repairing one file
    /// would retroactively steal a name from another.
    ///
    /// Called before any file effect, so a refusal leaves the files, the
    /// projection, the revision, and both histories exactly as they were.
    fn check_identity_available(
        &self,
        identity: &TemplateIdentity,
        retiring: Option<&TemplateIdentity>,
    ) -> Result<(), CatalogRejection> {
        if retiring == Some(identity) {
            return Ok(());
        }
        match self.set.get(identity) {
            None => Ok(()),
            Some(existing) => Err(CatalogRejection::DuplicateIdentity {
                identity: identity.clone(),
                existing: existing.source.clone(),
            }),
        }
    }

    fn digest_of(&self, file: &Path) -> Result<write::FileDigest, CatalogRejection> {
        write::file_digest(&self.root, file)?
            .ok_or_else(|| CatalogRejection::Write(WriteError::SourceMissing(file.to_owned())))
    }

    fn fingerprint_of(&self, identity: &TemplateIdentity) -> Option<ContentFingerprint> {
        self.set.get(identity).and_then(|entry| entry.fingerprint)
    }

    fn reload(&mut self) {
        self.set = load_directory(self.root.path(), &self.registry, &self.limits);
    }

    fn record(&mut self, actor: &ActorId, change: CatalogChange) -> CatalogRevision {
        self.revision = self.revision.next();
        self.events.push_back(CatalogEvent {
            revision: self.revision,
            actor: actor.clone(),
            change,
        });
        while self.events.len() > MAX_EVENT_HISTORY {
            self.events.pop_front();
        }
        self.revision
    }

    fn remember(&mut self, command_id: CommandId, outcome: CatalogOutcome) {
        self.accepted.insert(command_id.clone(), outcome);
        self.accepted_order.push_back(command_id);
        while self.accepted_order.len() > MAX_COMMAND_HISTORY {
            if let Some(evicted) = self.accepted_order.pop_front() {
                self.accepted.remove(&evicted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{
        ComponentDocument, MetadataDocument, PropertyValueDocument, QuantityDocument, SpecDocument,
    };
    use crate::name::{ComponentName, PluginId, PropertyName};
    use crate::quantity::Dimension;
    use crate::schema::{ComponentSchema, PropertyKind, PropertySchema, SchemaVersion};
    use crate::source::DocumentOrdinal;
    use std::fs;

    const SUN: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: sun}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: {expression: "1.989e30", unit: kg}}
"#;

    /// A second valid entry, so a rename has somewhere to collide.
    const EARTH: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: earth}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: {expression: "5.97e24", unit: kg}}
"#;

    /// Structurally valid, but composing a component type this installation
    /// has no schema for: loads as `Unavailable`, identity and all.
    const EARTH_UNAVAILABLE: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: earth}
spec:
  components:
  - type: {plugin: kagami.not_installed, name: exotic}
    properties:
      mass: {quantity: {expression: "5.97e24", unit: kg}}
"#;

    /// A broken expression under valid metadata: loads as `Invalid`, but its
    /// identity is still recoverable, so it still owns the name.
    const EARTH_INVALID: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: earth}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "1 +"}
"#;

    fn registry() -> SchemaRegistry {
        SchemaRegistry::new().with(
            ComponentSchema::new(
                ComponentTypeId::new(
                    PluginId::new("kagami.mass_sources").unwrap(),
                    ComponentName::new("inertial_mass").unwrap(),
                ),
                SchemaVersion(1),
            )
            .with_property(
                PropertyName::new("mass").unwrap(),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        )
    }

    fn document(name: &str, mass: &str) -> TemplateDocument {
        crate::document::new(
            MetadataDocument::new("planets".try_into().unwrap(), name.try_into().unwrap()),
            SpecDocument {
                components: vec![ComponentDocument {
                    component_type: ComponentTypeId::new(
                        PluginId::new("kagami.mass_sources").unwrap(),
                        ComponentName::new("inertial_mass").unwrap(),
                    ),
                    properties: [(
                        PropertyName::new("mass").unwrap(),
                        PropertyValueDocument::Quantity(QuantityDocument::canonical(mass)),
                    )]
                    .into_iter()
                    .collect(),
                }],
                ..SpecDocument::default()
            },
        )
    }

    fn identity(name: &str) -> TemplateIdentity {
        TemplateIdentity::new("planets".try_into().unwrap(), name.try_into().unwrap())
    }

    fn envelope(id: &str, command: CatalogCommand) -> CatalogCommandEnvelope {
        CatalogCommandEnvelope::new(
            CommandId::new(id).unwrap(),
            ActorId::new("researcher").unwrap(),
            command,
        )
    }

    fn authority(directory: &Path) -> CatalogAuthority {
        CatalogAuthority::open(directory, registry(), Limits::DEFAULT)
    }

    fn seeded() -> (tempfile::TempDir, CatalogAuthority) {
        seeded_with(SUN)
    }

    fn seeded_with(contents: &str) -> (tempfile::TempDir, CatalogAuthority) {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("planets.yaml"), contents).unwrap();
        let authority = authority(directory.path());
        (directory, authority)
    }

    /// The target naming document `index` of the seeded file, claiming
    /// `name`.
    fn target(name: &str, index: usize) -> WriteTarget {
        WriteTarget {
            file: PathBuf::from("planets.yaml"),
            document: DocumentOrdinal::from_index(index),
            identity: identity(name),
        }
    }

    /// Everything a rejected command must leave untouched.
    struct Snapshot {
        text: String,
        entries: Vec<CatalogEntry>,
        revision: CatalogRevision,
        events: usize,
        accepted: usize,
    }

    impl Snapshot {
        fn of(directory: &Path, authority: &CatalogAuthority) -> Self {
            Self {
                text: fs::read_to_string(directory.join("planets.yaml")).unwrap(),
                entries: authority.set().entries().to_vec(),
                revision: authority.revision(),
                events: authority.events().count(),
                accepted: authority.accepted.len(),
            }
        }

        fn assert_unchanged(&self, directory: &Path, authority: &CatalogAuthority) {
            let now = Self::of(directory, authority);
            assert_eq!(now.text, self.text, "the user's file changed");
            assert_eq!(now.entries, self.entries, "the projection changed");
            assert_eq!(now.revision, self.revision, "the revision advanced");
            assert_eq!(now.events, self.events, "an event was recorded");
            assert_eq!(now.accepted, self.accepted, "an outcome was remembered");
        }
    }

    #[test]
    fn opening_a_missing_directory_yields_an_empty_catalog_at_the_initial_revision() {
        let authority = authority(Path::new("/nonexistent/catalogs"));
        assert_eq!(authority.revision(), CatalogRevision::INITIAL);
        assert_eq!(authority.set().entries().len(), 0);
    }

    #[test]
    fn listing_returns_a_row_per_entry_at_the_current_revision() {
        let (_directory, authority) = seeded();
        let CatalogView::List { revision, entries } = authority.query(&CatalogQuery::List) else {
            panic!("expected a list");
        };
        assert_eq!(revision, CatalogRevision::INITIAL);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, "available");
        assert_eq!(entries[0].identity, Some(identity("sun")));
        assert_eq!(entries[0].components.len(), 1);
    }

    #[test]
    fn getting_an_unknown_identity_returns_nothing_rather_than_failing() {
        let (_directory, authority) = seeded();
        let CatalogView::Entry { entry, .. } =
            authority.query(&CatalogQuery::Get(identity("pluto")))
        else {
            panic!("expected an entry view");
        };
        assert!(entry.is_none());
    }

    #[test]
    fn creating_a_template_advances_the_revision_and_emits_one_event() {
        let (_directory, mut authority) = seeded();
        let outcome = authority
            .submit(envelope(
                "create-earth",
                CatalogCommand::Create {
                    file: PathBuf::from("planets.yaml"),
                    document: Box::new(document("earth", "5.97e24")),
                },
            ))
            .unwrap();
        assert_eq!(outcome.revision(), CatalogRevision(1));
        assert_eq!(authority.revision(), CatalogRevision(1));
        assert!(authority.set().get(&identity("earth")).is_some());
        let events: Vec<_> = authority.events().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change, CatalogChange::Created(identity("earth")));
        assert_eq!(events[0].actor.as_str(), "researcher");
    }

    #[test]
    fn resubmitting_an_accepted_command_replays_its_outcome_without_repeating_it() {
        let (_directory, mut authority) = seeded();
        let command = CatalogCommand::Create {
            file: PathBuf::from("planets.yaml"),
            document: Box::new(document("earth", "5.97e24")),
        };
        let first = authority
            .submit(envelope("create-earth", command.clone()))
            .unwrap();
        let replay = authority.submit(envelope("create-earth", command)).unwrap();
        assert_eq!(first, replay);
        assert_eq!(authority.revision(), CatalogRevision(1));
        assert_eq!(authority.set().entries().len(), 2);
    }

    #[test]
    fn a_command_guarded_by_a_stale_revision_is_refused_and_changes_nothing() {
        let (_directory, mut authority) = seeded();
        authority
            .submit(envelope("reload-1", CatalogCommand::Reload))
            .unwrap();
        let error = authority
            .submit(
                envelope(
                    "create-earth",
                    CatalogCommand::Create {
                        file: PathBuf::from("planets.yaml"),
                        document: Box::new(document("earth", "5.97e24")),
                    },
                )
                .guarded_by(CatalogRevision::INITIAL),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            CatalogRejection::RevisionConflict {
                expected: CatalogRevision(0),
                actual: CatalogRevision(1),
            }
        ));
        assert!(authority.set().get(&identity("earth")).is_none());
    }

    #[test]
    fn a_failed_command_is_not_remembered_so_a_retry_still_runs() {
        let (_directory, mut authority) = seeded();
        let create = || {
            envelope(
                "create-earth",
                CatalogCommand::Create {
                    file: PathBuf::from("planets.yaml"),
                    document: Box::new(document("earth", "5.97e24")),
                },
            )
        };
        assert!(
            authority
                .submit(create().guarded_by(CatalogRevision(99)))
                .is_err()
        );
        assert!(authority.submit(create()).is_ok());
        assert!(authority.set().get(&identity("earth")).is_some());
    }

    #[test]
    fn creating_an_identity_that_already_exists_is_refused() {
        let (_directory, mut authority) = seeded();
        let error = authority
            .submit(envelope(
                "create-sun-again",
                CatalogCommand::Create {
                    file: PathBuf::from("other.yaml"),
                    document: Box::new(document("sun", "1.0")),
                },
            ))
            .unwrap_err();
        assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));
    }

    #[test]
    fn creating_over_an_invalid_entry_that_still_owns_the_name_is_refused() {
        // Preserving broken user data must not make the name available: the
        // entry is exactly the one a repair would edit.
        let (directory, mut authority) = seeded_with(&format!("{SUN}---\n{EARTH_INVALID}"));
        let before = Snapshot::of(directory.path(), &authority);
        let error = authority
            .submit(envelope(
                "create-earth",
                CatalogCommand::Create {
                    file: PathBuf::from("other.yaml"),
                    document: Box::new(document("earth", "5.97e24")),
                },
            ))
            .unwrap_err();
        assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));
        before.assert_unchanged(directory.path(), &authority);
    }

    #[test]
    fn renaming_a_template_onto_another_entrys_identity_is_refused_before_the_file_changes() {
        let (directory, mut authority) = seeded_with(&format!("{SUN}---\n{EARTH}"));
        let before = Snapshot::of(directory.path(), &authority);

        let rename = || {
            envelope(
                "rename-sun-to-earth",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("earth", "1.989e30")),
                },
            )
        };
        let error = authority.submit(rename()).unwrap_err();
        let CatalogRejection::DuplicateIdentity { identity, existing } = error else {
            panic!("expected a duplicate-identity refusal");
        };
        assert_eq!(identity, self::identity("earth"));
        assert_eq!(
            existing,
            SourceLocation::new("planets.yaml", DocumentOrdinal::from_index(1))
        );
        before.assert_unchanged(directory.path(), &authority);

        // A refused command is not remembered, so the same id still runs once
        // the caller composes something acceptable.
        authority
            .submit(envelope(
                "rename-sun-to-earth",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("sol", "1.989e30")),
                },
            ))
            .expect("a non-colliding rename under the same command id succeeds");
    }

    #[test]
    fn an_unavailable_entry_still_reserves_its_identity_against_a_rename() {
        let (directory, mut authority) = seeded_with(&format!("{SUN}---\n{EARTH_UNAVAILABLE}"));
        assert_eq!(authority.set().summary().unavailable, 1);
        let before = Snapshot::of(directory.path(), &authority);

        let error = authority
            .submit(envelope(
                "rename",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("earth", "1.989e30")),
                },
            ))
            .unwrap_err();
        assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));
        before.assert_unchanged(directory.path(), &authority);
    }

    #[test]
    fn an_invalid_entry_with_a_recoverable_identity_still_reserves_it_against_a_rename() {
        let (directory, mut authority) = seeded_with(&format!("{SUN}---\n{EARTH_INVALID}"));
        assert_eq!(authority.set().summary().invalid, 1);
        let before = Snapshot::of(directory.path(), &authority);

        let error = authority
            .submit(envelope(
                "rename",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("earth", "1.989e30")),
                },
            ))
            .unwrap_err();
        assert!(matches!(error, CatalogRejection::DuplicateIdentity { .. }));
        before.assert_unchanged(directory.path(), &authority);
    }

    #[test]
    fn a_rename_reports_both_identities_and_lets_a_cached_view_converge() {
        let (_directory, mut authority) = seeded_with(&format!("{SUN}---\n{EARTH}"));
        let CatalogView::List {
            entries: before, ..
        } = authority.query(&CatalogQuery::List)
        else {
            panic!("expected a list");
        };

        let outcome = authority
            .submit(envelope(
                "rename-sun-to-sol",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("sol", "1.989e30")),
                },
            ))
            .unwrap();
        let CatalogOutcome::Updated {
            previous, current, ..
        } = &outcome
        else {
            panic!("expected an update outcome");
        };
        assert_eq!(previous, &identity("sun"));
        assert_eq!(current, &identity("sol"));
        assert!(authority.set().get(&identity("sun")).is_none());
        assert!(authority.set().get(&identity("sol")).is_some());

        // A view that only ever sees events reaches the authority's listing.
        let mut cached: Vec<Option<TemplateIdentity>> =
            before.iter().map(|entry| entry.identity.clone()).collect();
        for event in authority.events() {
            let CatalogChange::Updated { previous, current } = &event.change else {
                panic!("expected one update event");
            };
            let row = cached
                .iter_mut()
                .find(|identity| identity.as_ref() == Some(previous))
                .expect("the retired row is there to remove");
            *row = Some(current.clone());
        }
        let CatalogView::List { entries: after, .. } = authority.query(&CatalogQuery::List) else {
            panic!("expected a list");
        };
        let authoritative: Vec<Option<TemplateIdentity>> =
            after.iter().map(|entry| entry.identity.clone()).collect();
        assert_eq!(cached, authoritative);
    }

    #[test]
    fn an_invalid_submitted_document_is_refused_before_any_file_is_touched() {
        let (directory, mut authority) = seeded();
        let before = fs::read_to_string(directory.path().join("planets.yaml")).unwrap();
        let error = authority
            .submit(envelope(
                "create-broken",
                CatalogCommand::Create {
                    file: PathBuf::from("planets.yaml"),
                    document: Box::new(document("broken", "1 +")),
                },
            ))
            .unwrap_err();
        assert!(matches!(error, CatalogRejection::InvalidDocument { .. }));
        assert_eq!(
            fs::read_to_string(directory.path().join("planets.yaml")).unwrap(),
            before
        );
        assert_eq!(authority.revision(), CatalogRevision::INITIAL);
    }

    #[test]
    fn updating_a_template_replaces_it_and_advances_the_revision() {
        let (_directory, mut authority) = seeded();
        let outcome = authority
            .submit(envelope(
                "update-sun",
                CatalogCommand::Update {
                    target: target("sun", 0),
                    document: Box::new(document("sun", "1.9885e30")),
                },
            ))
            .unwrap();
        assert_eq!(authority.revision(), CatalogRevision(1));
        let entry = authority.set().get(&identity("sun")).unwrap();
        let template = entry.result.template().unwrap();
        assert_eq!(template.spec.components.len(), 1);
        // An edit that does not rename reports the same identity twice, so a
        // consumer applies one uniform rule.
        let CatalogOutcome::Updated {
            previous, current, ..
        } = outcome
        else {
            panic!("expected an update outcome");
        };
        assert_eq!(previous, identity("sun"));
        assert_eq!(current, identity("sun"));
    }

    #[test]
    fn deleting_a_template_removes_it_from_the_projection() {
        let (_directory, mut authority) = seeded();
        authority
            .submit(envelope(
                "delete-sun",
                CatalogCommand::Delete {
                    target: WriteTarget {
                        file: PathBuf::from("planets.yaml"),
                        document: DocumentOrdinal::from_index(0),
                        identity: identity("sun"),
                    },
                },
            ))
            .unwrap();
        assert!(authority.set().get(&identity("sun")).is_none());
        assert_eq!(authority.revision(), CatalogRevision(1));
    }

    #[test]
    fn validating_text_reports_problems_without_changing_anything() {
        let (_directory, mut authority) = seeded();
        let outcome = authority
            .submit(envelope(
                "validate",
                CatalogCommand::Validate {
                    text: SUN.replace("1.989e30", "1 +"),
                },
            ))
            .unwrap();
        let CatalogOutcome::Validated { revision, reports } = outcome else {
            panic!("expected a validation outcome");
        };
        assert_eq!(revision, CatalogRevision::INITIAL);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].identity, Some(identity("sun")));
        assert_eq!(reports[0].diagnostics.len(), 1);
        assert!(authority.events().next().is_none());
    }

    #[test]
    fn reload_picks_up_an_external_edit_and_changes_only_the_catalog_revision() {
        let (directory, mut authority) = seeded();
        fs::write(
            directory.path().join("planets.yaml"),
            SUN.replace("name: sun", "name: sol"),
        )
        .unwrap();
        assert!(authority.set().get(&identity("sol")).is_none());

        authority
            .submit(envelope("reload", CatalogCommand::Reload))
            .unwrap();
        assert!(authority.set().get(&identity("sol")).is_some());
        assert_eq!(authority.revision(), CatalogRevision(1));
    }

    #[test]
    fn a_write_whose_target_document_is_not_there_is_refused() {
        let (_directory, mut authority) = seeded();
        // The authority re-reads the file digest at write time, so the digest
        // is not what protects a stale *caller* — the revision guard is. What
        // the target identity check protects is aiming at the wrong document.
        let target = WriteTarget {
            file: PathBuf::from("planets.yaml"),
            document: DocumentOrdinal::from_index(5),
            identity: identity("sun"),
        };
        let error = authority
            .submit(envelope(
                "update-missing",
                CatalogCommand::Update {
                    target,
                    document: Box::new(document("sun", "1.0")),
                },
            ))
            .unwrap_err();
        assert!(matches!(
            error,
            CatalogRejection::Write(WriteError::TargetMismatch { .. })
        ));
    }

    #[test]
    fn events_can_be_replayed_from_a_known_revision() {
        let (_directory, mut authority) = seeded();
        authority
            .submit(envelope("r1", CatalogCommand::Reload))
            .unwrap();
        let checkpoint = authority.revision();
        authority
            .submit(envelope("r2", CatalogCommand::Reload))
            .unwrap();
        authority
            .submit(envelope("r3", CatalogCommand::Reload))
            .unwrap();
        let caught_up: Vec<_> = authority.events_since(checkpoint).collect();
        assert_eq!(caught_up.len(), 2);
        assert_eq!(caught_up[0].revision, CatalogRevision(2));
    }

    #[test]
    fn event_history_is_bounded() {
        let (_directory, mut authority) = seeded();
        for index in 0..MAX_EVENT_HISTORY + 10 {
            authority
                .submit(envelope(&format!("reload-{index}"), CatalogCommand::Reload))
                .unwrap();
        }
        assert_eq!(authority.events().count(), MAX_EVENT_HISTORY);
    }

    #[test]
    fn command_history_is_bounded() {
        let (_directory, mut authority) = seeded();
        for index in 0..MAX_COMMAND_HISTORY + 10 {
            authority
                .submit(envelope(&format!("reload-{index}"), CatalogCommand::Reload))
                .unwrap();
        }
        assert_eq!(authority.accepted.len(), MAX_COMMAND_HISTORY);
    }

    #[test]
    fn instantiating_through_the_authority_leaves_the_catalog_revision_alone() {
        let (_directory, authority) = seeded();
        let request = InstantiationRequest::new(
            identity("sun"),
            orishu_variables::Namespace::new("objects.o1"),
        );
        let candidate = authority.instantiate(&request).unwrap();
        assert_eq!(candidate.provenance.identity, identity("sun"));
        assert_eq!(authority.revision(), CatalogRevision::INITIAL);
    }

    #[cfg(unix)]
    #[test]
    fn a_create_below_a_symlinked_parent_is_refused_and_touches_nothing_outside() {
        let (directory, mut authority) = seeded();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), directory.path().join("linked")).unwrap();
        let before = Snapshot::of(directory.path(), &authority);

        let error = authority
            .submit(envelope(
                "create-outside",
                CatalogCommand::Create {
                    file: PathBuf::from("linked/created-outside/planets.yaml"),
                    document: Box::new(document("earth", "5.97e24")),
                },
            ))
            .unwrap_err();

        assert!(matches!(
            error,
            CatalogRejection::Write(WriteError::NotContained(_))
        ));
        assert!(
            !elsewhere.path().join("created-outside").exists(),
            "a rejected command created a directory outside the catalog root"
        );
        assert!(elsewhere.path().read_dir().unwrap().next().is_none());
        before.assert_unchanged(directory.path(), &authority);
    }

    #[test]
    fn a_bounded_identity_rejects_empty_and_oversized_text() {
        assert_eq!(CommandId::new(""), Err(IdentityError::Empty));
        assert!(matches!(
            ActorId::new("x".repeat(MAX_IDENTITY_BYTES + 1)),
            Err(IdentityError::TooLong { .. })
        ));
    }
}
