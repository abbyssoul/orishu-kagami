use crate::launch::LaunchOptions;
use crate::scene_model::{NodeId, ObjectKind, SceneNode, SceneTree};
use kagami_renderer::SceneProgram;
use orishu::client::ClusterAddress;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

pub struct Model {
    /// The Orishu endpoint this client will use once the remote adapter is wired.
    pub cluster_address: ClusterAddress,
    pub scene: SceneTree,
    pub search_query: String,
    pub selected: Option<NodeId>,
    pub expanded: HashSet<NodeId>,
    pub settings_open: bool,
    pub open_menu: Option<Menu>,
    pub scene_program: SceneProgram,
    /// The document this scene was opened from, if any — shown in the
    /// window title, the way most apps show "which document" rather than
    /// a path. `None` means the built-in demo scene.
    pub document_path: Option<PathBuf>,
    /// When set, the app quits by itself once `Instant::now()` reaches this
    /// — from `--exit-after`, for automated testing.
    pub exit_deadline: Option<Instant>,
    pub active_tool: Tool,
    pub playback: Playback,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Playing,
    Paused,
}

impl Model {
    pub fn new(options: LaunchOptions) -> Self {
        let mut next_id = 0u64;
        let mut id = || {
            let id = NodeId(next_id);
            next_id += 1;
            id
        };

        let objects =
            SceneNode::new(id(), "Objects (2)", ObjectKind::Category).with_children(vec![
                SceneNode::new(id(), "Earth", ObjectKind::Planet),
                SceneNode::new(id(), "Moon", ObjectKind::Planet),
            ]);

        let measurement = SceneNode::new(id(), "Measurement (2)", ObjectKind::Category)
            .with_children(vec![
                SceneNode::new(id(), "Field probe", ObjectKind::Probe),
                SceneNode::new(id(), "Distance 1", ObjectKind::Probe),
            ]);

        let slice_planes =
            SceneNode::new(id(), "Slice planes (1)", ObjectKind::Category).with_children(vec![
                SceneNode::new(id(), "XY field plane", ObjectKind::SlicePlane),
            ]);

        let expanded = [objects.id, measurement.id, slice_planes.id]
            .into_iter()
            .collect();

        let mut model = Self {
            cluster_address: options.cluster_address,
            scene: SceneTree {
                roots: vec![objects, measurement, slice_planes],
            },
            search_query: String::new(),
            selected: None,
            expanded,
            settings_open: false,
            open_menu: None,
            scene_program: SceneProgram,
            document_path: None,
            exit_deadline: options.exit_after.map(|lifetime| Instant::now() + lifetime),
            active_tool: Tool::Select,
            playback: Playback::Paused,
            queue_len: 0,
        };

        if let Some(path) = options.open_path {
            model.open_document(path);
        }

        model
    }

    /// Open `path` as the current document — used identically by `--scene`
    /// at startup and by the interactive File > Open dialog, so both take
    /// the same path through the model.
    pub fn open_document(&mut self, path: PathBuf) {
        log::info!("scene deserialization isn't implemented yet; showing the demo scene");
        self.document_path = Some(path);
    }

    /// The window title — just the file name, not the full path (the title
    /// bar is for "which document", not a path browser).
    pub fn title(&self) -> String {
        match self
            .document_path
            .as_ref()
            .and_then(|path| path.file_name())
        {
            Some(name) => format!("{} — Kagami", name.to_string_lossy()),
            None => "Kagami".to_owned(),
        }
    }
}
