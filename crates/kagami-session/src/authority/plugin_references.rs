//! Cold bounded reference projection. No inventory, IO, availability decisions,
//! provider substitution or scientific-buffer decoding belongs in this query.
use super::{DocumentAuthority, SessionCommand};
use kagami_catalog::ComponentTypeId;
use kagami_document::{ExperimentCommand, ExperimentSnapshot, scientific::ScientificSetup};
use orishu_plugin::PluginReleaseId;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Immutable shared handles for a bounded off-thread reference scan. Capturing
/// retains old state until the reader drops it; shell effect budgets must account
/// for that lifetime. No object, opaque buffer or accepted-command body is copied.
pub struct PluginReferenceSnapshot {
    current: ExperimentSnapshot,
    history: Vec<ExperimentSnapshot>,
    accepted: Vec<Arc<super::AcceptedRecord>>,
}

/// Caller-owned ceilings. Work counts retained snapshots, objects, component
/// slots, scientific contributions and retained edit commands, including legacy
/// slots which have no exact release. Duplicate references still consume work.
#[derive(Clone, Copy, Debug)]
pub struct PluginReferenceLimits {
    /// Maximum records examined across all retained sources.
    pub work: usize,
    /// Maximum distinct release identities returned.
    pub releases: usize,
}
impl Default for PluginReferenceLimits {
    fn default() -> Self {
        Self {
            work: 1_000_000,
            releases: 256,
        }
    }
}
/// Sources retaining an exact release. These are presence flags, not counts of
/// documents, executable invocations or unique heap allocations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginReferenceUse {
    /// Present in the current experiment (objects or selected scientific setup).
    pub current: bool,
    /// Present in an undo or redo snapshot.
    pub history: bool,
    /// Mentioned by retained accepted command data, including old Open requests.
    pub receipts: bool,
}
/// Complete result for this authority only, never discovery of files on disk.
/// A report proves references, not installation, availability or acquired leases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginReferenceReport {
    /// Deterministically ordered exact immutable release identities.
    pub releases: BTreeMap<PluginReleaseId, PluginReferenceUse>,
    /// Work actually charged for this completed scan.
    pub work: usize,
}
/// Fail closed: no partial prefix may be interpreted as an absence of references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PluginReferenceError {
    /// The scan stopped before examining all retained data.
    #[error("plugin reference scan exceeds its work limit")]
    WorkLimit,
    /// Another distinct release would exceed the output bound.
    #[error("plugin reference scan exceeds its release limit")]
    ReleaseLimit,
}
#[derive(Clone, Copy)]
enum Source {
    Current,
    History,
    Receipts,
}
struct Scan {
    limits: PluginReferenceLimits,
    report: PluginReferenceReport,
}
impl Scan {
    fn charge(&mut self) -> Result<(), PluginReferenceError> {
        if self.report.work == self.limits.work {
            return Err(PluginReferenceError::WorkLimit);
        }
        self.report.work += 1;
        Ok(())
    }
    fn release(
        &mut self,
        release: PluginReleaseId,
        source: Source,
    ) -> Result<(), PluginReferenceError> {
        if !self.report.releases.contains_key(&release)
            && self.report.releases.len() == self.limits.releases
        {
            return Err(PluginReferenceError::ReleaseLimit);
        }
        let usage = self.report.releases.entry(release).or_default();
        match source {
            Source::Current => usage.current = true,
            Source::History => usage.history = true,
            Source::Receipts => usage.receipts = true,
        }
        Ok(())
    }
    fn component(
        &mut self,
        kind: &ComponentTypeId,
        source: Source,
    ) -> Result<(), PluginReferenceError> {
        self.charge()?;
        if let Some(pin) = kind.contribution() {
            self.release(pin.release, source)?;
        }
        Ok(())
    }
    fn setup(
        &mut self,
        setup: &ScientificSetup,
        source: Source,
    ) -> Result<(), PluginReferenceError> {
        for pin in &setup.declarations().descriptor().contributions {
            self.charge()?;
            self.release(pin.release, source)?;
        }
        Ok(())
    }
    fn snapshot(
        &mut self,
        snapshot: &ExperimentSnapshot,
        source: Source,
    ) -> Result<(), PluginReferenceError> {
        self.charge()?;
        for object in snapshot.objects().values() {
            self.charge()?;
            for kind in object.components.keys() {
                self.component(kind, source)?;
            }
        }
        if let Some(setup) = snapshot.setup().scientific() {
            self.setup(setup, source)?;
        }
        if let Some(lock) = snapshot.dependencies() {
            self.dependencies(lock, source)?;
        }
        Ok(())
    }
    fn dependencies(
        &mut self,
        lock: &orishu_plugin::authoring_lock::SelectionLock,
        source: Source,
    ) -> Result<(), PluginReferenceError> {
        for pin in &lock.selection().contributions {
            self.charge()?;
            self.release(pin.release, source)?;
        }
        Ok(())
    }
    fn edit(&mut self, command: &ExperimentCommand) -> Result<(), PluginReferenceError> {
        self.charge()?;
        match command {
            ExperimentCommand::AdoptDependencies(lock) => {
                self.dependencies(lock, Source::Receipts)?
            }
            ExperimentCommand::AdoptScientificSetup(setup) => {
                self.setup(setup, Source::Receipts)?
            }
            ExperimentCommand::CreateObject(spec) => {
                for kind in spec.components.keys() {
                    self.component(kind, Source::Receipts)?;
                }
            }
            ExperimentCommand::AttachComponent { component, .. }
            | ExperimentCommand::DetachComponent { component, .. }
            | ExperimentCommand::SetComponentProperty { component, .. } => {
                self.component(component, Source::Receipts)?
            }
            // These commands carry no exact executable/vocabulary identity.
            ExperimentCommand::RemoveObject(_)
            | ExperimentCommand::RenameObject { .. }
            | ExperimentCommand::SetTransform { .. }
            | ExperimentCommand::SetVelocity { .. }
            | ExperimentCommand::SetShape { .. }
            | ExperimentCommand::DefineVariable(_)
            | ExperimentCommand::SetVariableExpression { .. }
            | ExperimentCommand::RenameVariable { .. }
            | ExperimentCommand::RemoveVariable(_)
            | ExperimentCommand::SetVariableDescription { .. }
            | ExperimentCommand::SetDomain(_)
            | ExperimentCommand::SetTimeStep(_)
            | ExperimentCommand::SetPluginEnabled { .. } => (),
        }
        Ok(())
    }
}
impl DocumentAuthority {
    /// Report exact releases in current state, both history stacks and retained
    /// accepted requests. Legacy logical names and catalog provenance never
    /// resolve to a release. Uses no installed schemas or catalog lookup.
    ///
    /// O(examined retained records × log(distinct releases)) time, O(distinct
    /// releases + retained snapshots/requests) temporary space, with caller-owned
    /// bounds and no object, request-body or opaque-buffer copies.
    /// Cold query: adapters must not scan whole histories on every UI frame.
    pub fn plugin_references(
        &self,
        limits: PluginReferenceLimits,
    ) -> Result<PluginReferenceReport, PluginReferenceError> {
        self.plugin_reference_snapshot(limits)?.scan(limits)
    }
    /// Capture only shared handles. Refuse the handle budget before growing the
    /// snapshot list; the subsequent scan independently charges its full work.
    pub fn plugin_reference_snapshot(
        &self,
        limits: PluginReferenceLimits,
    ) -> Result<PluginReferenceSnapshot, PluginReferenceError> {
        let mut history = Vec::new();
        for snapshot in self.history.snapshots() {
            if history
                .len()
                .saturating_add(self.accepted.len())
                .saturating_add(1)
                >= limits.work
            {
                return Err(PluginReferenceError::WorkLimit);
            }
            history.push(snapshot);
        }
        if history
            .len()
            .saturating_add(self.accepted.len())
            .saturating_add(1)
            > limits.work
        {
            return Err(PluginReferenceError::WorkLimit);
        }
        Ok(PluginReferenceSnapshot {
            current: self.experiment.snapshot(),
            history,
            accepted: self.accepted.iter().cloned().collect(),
        })
    }
}
impl PluginReferenceSnapshot {
    /// Scan this immutable retained image, independent of later authoring edits.
    pub fn scan(
        &self,
        limits: PluginReferenceLimits,
    ) -> Result<PluginReferenceReport, PluginReferenceError> {
        let mut scan = Scan {
            limits,
            report: PluginReferenceReport {
                releases: BTreeMap::new(),
                work: 0,
            },
        };
        scan.snapshot(&self.current, Source::Current)?;
        for snapshot in &self.history {
            scan.snapshot(snapshot, Source::History)?;
        }
        for record in &self.accepted {
            scan.charge()?;
            match &record.envelope.command {
                SessionCommand::Edit(commands) => {
                    for command in commands {
                        scan.edit(command)?;
                    }
                }
                SessionCommand::Open { experiment, .. } => {
                    scan.snapshot(&experiment.snapshot(), Source::Receipts)?
                }
                SessionCommand::Undo
                | SessionCommand::Redo
                | SessionCommand::BeginInteractiveEdit
                | SessionCommand::New { .. }
                | SessionCommand::InstantiateObjectTemplate(_)
                | SessionCommand::EndInteractiveEdit => (),
            }
        }
        Ok(scan.report)
    }
}
