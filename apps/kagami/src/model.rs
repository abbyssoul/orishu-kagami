//! What the window holds: one document, and how it is being looked at.
//!
//! The split is the point. [`Model::document`] is the authority — the only
//! thing that knows what an experiment is. Other fields hold client-local
//! presentation and IO adapters, including the separately owned run projection.
//! They do not introduce an editable scientific authority or enter document undo.
//!
//! # The one field that is presentation but still saved
//!
//! The authoring camera and projection are held by [`Document`], not here,
//! because saving has to capture them (ADR 0022). What *is* here is
//! [`Model::observing_view`]: the throwaway copy a window looks through while
//! watching a run. Every camera change made while observing goes there and is
//! dropped on leaving, which is how "playback camera changes dirty nothing"
//! ends up being structural rather than a rule someone has to remember.

use std::collections::HashSet;
use std::time::Instant;

use kagami_catalog::{ComponentTypeId, PropertyName, SchemaRegistry};
use kagami_document::{Limits, ObjectId};
use kagami_session::{AuthoringView, WorkspaceMode};
use orishu::client::ClusterAddress;

use crate::document::Document;
use crate::launch::LaunchOptions;
use crate::mcp::{self, McpState, SessionState};

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
    #[cfg(unix)]
    pub catalog: crate::catalog_form::Controller,
    #[cfg(unix)]
    pub component_form: crate::component_form::ComponentForm,
    /// Exact local proposal owning the scientific lane after dependency handoff.
    #[cfg(unix)]
    pub(crate) component_extension: Option<uuid::Uuid>,
    /// Transient complete-reset proposal; never a partially edited experiment.
    #[cfg(unix)]
    pub component_reset: crate::component_form::reset::Flow,
    #[cfg(unix)]
    pub component_dependencies: crate::physics_form::dependencies::Controller,
    #[cfg(unix)]
    pub physics_dependencies: crate::physics_form::dependencies::Controller,
    #[cfg(unix)]
    pub plugin_references: crate::plugins::references::References,
    #[cfg(unix)]
    pub plugin_management: crate::plugins::window::Controller,
    #[cfg(unix)]
    pub workload_preparation: crate::workload_preparation::Preparation,
    /// Independent remote run projection; never editable experiment state.
    pub run: crate::run::Controller,
    /// Pending attachment consent is invalidated by editing/replacing the draft
    /// or changing workspace modes while its network read is in flight.
    pub(crate) run_attach_guard: Option<crate::document::AuthoringGuard>,
    /// One bounded background scientific edit, separate from MCP lifecycle.
    #[cfg(unix)]
    pub scientific_effects: crate::scientific_effect::ScientificEffects,
    #[cfg(unix)]
    pub physics_form: crate::physics_form::PhysicsForm,
    #[cfg(unix)]
    pub kernel_choices: Vec<crate::plugins::KernelChoice>,
    #[cfg(unix)]
    pub unresolved_components: Vec<orishu_plugin::ContributionRef>,
    #[cfg(unix)]
    pub inventory_revision: Option<u64>,
    /// The explicit Orishu endpoint selected for this process.
    pub cluster_address: ClusterAddress,
    /// The experiment, and the only way to change one.
    ///
    /// Also holds the workspace mode, because the mode gate has to sit on the
    /// same seam every command passes through — see [`Document::submit`].
    pub document: Document,
    /// The camera being looked through while observing a run.
    ///
    /// `Some` exactly while the workspace is observing. Copied from the
    /// authoring view on entry, per ADR 0022, and discarded on leaving: an
    /// observer's camera is never inherited by anyone and never reaches the
    /// file.
    pub observing_view: Option<AuthoringView>,

    // Everything below is presentation state (ADR 0012).
    pub search_query: String,
    pub selected: Option<ObjectId>,
    pub expanded: HashSet<ObjectId>,
    /// Objects hidden in the viewport. Client-local: hiding an object neither
    /// dirties the experiment nor advances its revision.
    pub hidden: HashSet<ObjectId>,
    /// A property field being typed into, if any.
    pub editing: Option<PropertyEdit>,
    /// What has been typed into the metres-per-unit field, if anything.
    ///
    /// `None` means the field shows the scale in force. Held for the same
    /// reason [`PropertyEdit`] is: a partial entry like `1 n` is not a scale,
    /// and parsing per keystroke would dirty the file on the way to a value
    /// nobody has finished asking for.
    pub scale_entry: Option<String>,
    /// Numbers the default name of the next object added. Presentation only:
    /// the model mints identities, this only picks a label.
    pub next_object_number: u64,
    pub settings_open: bool,
    pub open_menu: Option<Menu>,
    /// When set, the app quits by itself once `Instant::now()` reaches this
    /// — from `--exit-after`, for automated testing.
    pub exit_deadline: Option<Instant>,
    pub active_tool: Tool,
    /// Background scientific edits pending, derived from the one-slot controller.
    pub queue_len: usize,
    /// The embedded MCP server's lifecycle state (ADR 0006).
    ///
    /// `Disabled` by default: no port is bound and no request can reach the
    /// session until the user enables it or `--mcp` is passed. Holding it on
    /// the model is what lets the UI show disabled/running/failed and the
    /// connected-client count.
    pub mcp: McpState,
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
        // Historical demo schemas retain their legacy IDs. Installed immutable
        // contributions are additional exact types, never replacements by name.
        let mut schemas = bundled_schemas();
        for schema in options.plugin_schemas.schemas() {
            schemas.insert(schema.clone());
        }
        let mut document = Document::new(schemas, Limits::DEFAULT);
        #[cfg(unix)]
        let plugin_references = crate::plugins::references::References::new(
            options.scientific_plugins.as_ref().map(|p| p.store.clone()),
            &mut document,
        );
        if let Some(path) = options.open_path {
            // A fresh session has nothing to lose, so opening needs no
            // decision from anyone.
            document.open(path, true);
        }
        if let Some(notice) = options.plugin_notice {
            document.notice = Some(match document.notice.take() {
                Some(previous) => format!("{previous}\n{notice}"),
                None => notice,
            });
        }

        // `--mcp` enables the server from startup, under the same rules as the
        // in-app toggle. A failed bind never blocks the app: `enable` returns
        // `Failed` and the window opens normally. The endpoint and token are
        // reported to the console so a windowless workflow can hand them to a
        // client.
        let mcp = if options.mcp {
            let state = mcp::enable(SessionState::from_document(&document), mcp::DEFAULT_ADDR);
            mcp::report_startup(&state);
            state
        } else {
            McpState::Disabled
        };

        let model = Self {
            #[cfg(unix)]
            catalog: crate::catalog_form::Controller::new(options.scientific_plugins.clone()),
            #[cfg(unix)]
            component_form: Default::default(),
            #[cfg(unix)]
            component_extension: None,
            #[cfg(unix)]
            component_reset: Default::default(),
            #[cfg(unix)]
            component_dependencies: crate::physics_form::dependencies::Controller::new(
                options.scientific_plugins.clone(),
            ),
            #[cfg(unix)]
            physics_dependencies: crate::physics_form::dependencies::Controller::new(
                options.scientific_plugins.clone(),
            ),
            #[cfg(unix)]
            plugin_references,
            #[cfg(unix)]
            plugin_management: crate::plugins::window::Controller::new(
                options.scientific_plugins.clone(),
            ),
            #[cfg(unix)]
            workload_preparation: crate::workload_preparation::Preparation::new(
                options.scientific_plugins.clone(),
            ),
            run_attach_guard: None,
            run: crate::run::Controller::new(options.operator_token_file.map(|token_file| {
                crate::run::Connection {
                    address: options.cluster_address.clone(),
                    token_file,
                    ca_cert: options.ca_cert,
                }
            })),
            #[cfg(unix)]
            inventory_revision: options.scientific_plugins.as_ref().map(|p| p.revision),
            #[cfg(unix)]
            kernel_choices: options.kernel_choices,
            #[cfg(unix)]
            unresolved_components: options.unresolved_components,
            #[cfg(unix)]
            physics_form: Default::default(),
            #[cfg(unix)]
            scientific_effects: crate::scientific_effect::ScientificEffects::new(
                options.scientific_plugins,
            ),
            cluster_address: options.cluster_address,
            document,
            mcp,
            observing_view: None,
            search_query: String::new(),
            selected: None,
            expanded: HashSet::new(),
            hidden: HashSet::new(),
            editing: None,
            scale_entry: None,
            next_object_number: 1,
            settings_open: false,
            open_menu: None,
            exit_deadline: options.exit_after.map(|lifetime| Instant::now() + lifetime),
            active_tool: Tool::Select,
            queue_len: 0,
        };
        #[cfg(unix)]
        let model = {
            let mut model = model;
            model.plugin_references.drive(&mut model.document);
            model
        };
        model
    }

    /// The camera and projection this window is currently looking through.
    ///
    /// One accessor rather than two call sites choosing: while observing it is
    /// the ephemeral copy, and while authoring it is the document's saved
    /// view. Anything that draws reads through here, so nothing can accidentally
    /// render the authoring camera over a run.
    pub fn current_view(&self) -> AuthoringView {
        self.observing_view
            .unwrap_or_else(|| self.document.authoring_view())
    }

    /// `true` when document commands, undo and redo are available.
    pub fn is_authoring(&self) -> bool {
        self.document.is_authoring()
    }

    /// What the mode indicator says.
    ///
    /// Names the run while observing, because ADR 0022 requires the run
    /// identity to be unmistakable — an observer must never be able to mistake
    /// playback for an editable experiment.
    pub fn mode_label(&self) -> String {
        match self.document.mode() {
            WorkspaceMode::Authoring => "Authoring".to_owned(),
            WorkspaceMode::Observing(attachment) => {
                format!("Observing {}", attachment.run())
            }
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
/// Historical demo vocabulary, retained alongside exact installed plugin
/// contributions for explicit old-document compatibility. These names never
/// select executable physics or replace immutable plugin pins.
pub(crate) fn bundled_schemas() -> SchemaRegistry {
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
