//! What the window holds: one document, and how it is being looked at.
//!
//! The split is the point. [`Model::document`] is the authority — the only
//! thing that knows what an experiment is — and every other field is
//! presentation: which object is selected, which subtrees are open, what is
//! typed in the search box. ADR 0012 lists exactly those as client-local, so
//! none of them dirties the document or enters undo.

use std::collections::HashSet;
use std::time::Instant;

use kagami_catalog::{ComponentTypeId, PropertyName, SchemaRegistry};
use kagami_document::{Limits, ObjectId};
use kagami_renderer::SceneProgram;
use orishu::client::ClusterAddress;

use crate::document::Document;
use crate::launch::LaunchOptions;

/// A property field the user is part-way through editing.
///
/// Held here rather than submitted per keystroke: an expression is only
/// meaningful once it is finished, and a revision per character would fill the
/// undo history with fragments.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyEdit {
    /// Which object.
    pub object: ObjectId,
    /// Which of its components.
    pub component: ComponentTypeId,
    /// Which property.
    pub property: PropertyName,
    /// What has been typed so far.
    pub source: String,
}

pub struct Model {
    /// The Orishu endpoint this client will use once the remote adapter is wired.
    pub cluster_address: ClusterAddress,
    /// The experiment, and the only way to change one.
    pub document: Document,

    // Everything below is presentation state (ADR 0012).
    pub search_query: String,
    pub selected: Option<ObjectId>,
    pub expanded: HashSet<ObjectId>,
    /// Objects hidden in the viewport. Client-local: hiding an object neither
    /// dirties the experiment nor advances its revision.
    pub hidden: HashSet<ObjectId>,
    /// A property field being typed into, if any.
    pub editing: Option<PropertyEdit>,
    /// Numbers the default name of the next object added. Presentation only:
    /// the model mints identities, this only picks a label.
    pub next_object_number: u64,
    pub settings_open: bool,
    pub open_menu: Option<Menu>,
    pub scene_program: SceneProgram,
    /// When set, the app quits by itself once `Instant::now()` reaches this
    /// — from `--exit-after`, for automated testing.
    pub exit_deadline: Option<Instant>,
    pub active_tool: Tool,
    /// Background jobs pending. Always 0 for now — there is no async job
    /// system yet — but the toolbar already shows it, ready for one.
    pub queue_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Move,
    DrawFields,
}

impl Model {
    pub fn new(options: LaunchOptions) -> Self {
        let mut document = Document::new(bundled_schemas(), Limits::DEFAULT);
        if let Some(path) = options.open_path {
            // A fresh session has nothing to lose, so opening needs no
            // decision from anyone.
            document.open(path, true);
        }

        Self {
            cluster_address: options.cluster_address,
            document,
            search_query: String::new(),
            selected: None,
            expanded: HashSet::new(),
            hidden: HashSet::new(),
            editing: None,
            next_object_number: 1,
            settings_open: false,
            open_menu: None,
            scene_program: SceneProgram,
            exit_deadline: options.exit_after.map(|lifetime| Instant::now() + lifetime),
            active_tool: Tool::Select,
            queue_len: 0,
        }
    }

    /// The window title: the file name and whether it has unsaved changes.
    ///
    /// Derived from the authority every frame rather than remembered, so it
    /// cannot disagree with what is actually saved.
    pub fn title(&self) -> String {
        let name = self
            .document
            .target()
            .and_then(|target| {
                target
                    .path()
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled".to_owned());
        let dirty = if self.document.is_dirty() { "*" } else { "" };
        format!("{dirty}{name} — Kagami")
    }

    /// A default name for the next object the user adds.
    pub fn next_object_name(&mut self) -> String {
        let name = format!("Object {}", self.next_object_number);
        self.next_object_number += 1;
        name
    }

    /// Drop presentation state that names objects the experiment no longer
    /// has.
    ///
    /// Called after an accepted edit. A selection or an expanded node keyed to
    /// a removed object is not wrong so much as meaningless, and identities
    /// are never reused, so nothing can silently rebind.
    pub fn forget_missing(&mut self) {
        let snapshot = self.document.snapshot();
        self.selected = self.selected.filter(|id| snapshot.object(*id).is_some());
        self.expanded.retain(|id| snapshot.object(*id).is_some());
        self.hidden.retain(|id| snapshot.object(*id).is_some());
        if let Some(edit) = &self.editing
            && snapshot.object(edit.object).is_none()
        {
            self.editing = None;
        }
    }
}

/// The component schemas this build starts with.
///
/// A stand-in for X-PLUGIN's inventory, which does not exist yet. It is
/// deliberately *data*: nothing in the app branches on what is in here, so the
/// inspector renders whatever the registry happens to contain and a real
/// plugin inventory replaces this function without touching the UI.
fn bundled_schemas() -> SchemaRegistry {
    use kagami_catalog::{
        ComponentName, ComponentSchema, Dimension, PluginId, PropertyKind, PropertySchema,
        SchemaVersion,
    };

    let component = |plugin: &str, name: &str| {
        ComponentTypeId::new(
            PluginId::new(plugin).expect("a constant identifier is valid"),
            ComponentName::new(name).expect("a constant identifier is valid"),
        )
    };
    let property = |name: &str| PropertyName::new(name).expect("a constant identifier is valid");
    let quantity = |dimension| PropertySchema::required(PropertyKind::Quantity { dimension });

    SchemaRegistry::new()
        .with(
            ComponentSchema::new(component("kagami.dynamics", "dynamics"), SchemaVersion(1))
                .with_property(property("inertial_mass"), quantity(Dimension::MASS)),
        )
        .with(
            ComponentSchema::new(
                component("kagami.gravity", "gravitational_source"),
                SchemaVersion(1),
            )
            .with_property(property("mass"), quantity(Dimension::MASS)),
        )
        .with(
            ComponentSchema::new(
                component("kagami.electrostatics", "point_charge"),
                SchemaVersion(1),
            )
            .with_property(property("charge"), quantity(Dimension::CHARGE)),
        )
}
