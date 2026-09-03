use crate::model::{Menu, Tool};
use crate::scene_model::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone)]
pub enum Message {
    FileNew,
    FileOpen,
    FileSave,
    FileSaveAs,
    FileSettings,
    FileExit,
    HelpAbout,
    SettingsClosed,
    MenuToggled(Menu),
    MenuClosed,

    EditUndo,
    EditRedo,
    ToolSelected(Tool),
    PlaybackPlay,
    PlaybackPause,
    PlaybackStep,

    SearchChanged(String),
    NodeSelected(NodeId),
    NodeVisibilityToggled(NodeId),
    NodeExpandToggled(NodeId),

    TransformPositionChanged(NodeId, Axis, f32),
    TransformRotationChanged(NodeId, Axis, f32),

    /// Periodic tick used to notice `--exit-after`'s deadline has passed.
    ExitTimerTick,
}
