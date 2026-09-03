use crate::message::Message;
use crate::model::{Model, Playback};

pub fn update(model: &mut Model, message: Message) -> iced::Task<Message> {
    match message {
        Message::FileNew => {
            log::info!("File > New (stub)");
            model.open_menu = None;
        }
        Message::FileOpen => {
            model.open_menu = None;
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                model.open_document(path);
            }
        }
        Message::FileSave => {
            model.open_menu = None;
            if model.document_path.is_none() {
                if let Some(path) = rfd::FileDialog::new().save_file() {
                    log::info!("File > Save -> {} (stub, not written)", path.display());
                    model.document_path = Some(path);
                }
            } else {
                log::info!("File > Save (stub)");
            }
        }
        Message::FileSaveAs => {
            model.open_menu = None;
            if let Some(path) = rfd::FileDialog::new().save_file() {
                log::info!("File > Save As -> {} (stub, not written)", path.display());
            }
        }
        Message::FileSettings => {
            model.settings_open = true;
            model.open_menu = None;
        }
        Message::FileExit => {
            return iced::exit();
        }
        Message::HelpAbout => {
            log::info!("Help > About (stub)");
            model.open_menu = None;
        }
        Message::SettingsClosed => {
            model.settings_open = false;
        }
        Message::MenuToggled(menu) => {
            model.open_menu = if model.open_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        }
        Message::MenuClosed => {
            model.open_menu = None;
        }
        Message::EditUndo => {
            log::info!("Edit > Undo (stub)");
            model.open_menu = None;
        }
        Message::EditRedo => {
            log::info!("Edit > Redo (stub)");
            model.open_menu = None;
        }
        Message::ToolSelected(tool) => {
            model.active_tool = tool;
        }
        Message::PlaybackPlay => {
            log::info!("Play (stub, no simulation loop yet)");
            model.playback = Playback::Playing;
        }
        Message::PlaybackPause => {
            model.playback = Playback::Paused;
        }
        Message::PlaybackStep => {
            log::info!("Step (stub, no simulation loop yet)");
        }
        Message::SearchChanged(query) => {
            model.search_query = query;
        }
        Message::NodeSelected(id) => {
            model.selected = Some(id);
        }
        Message::NodeVisibilityToggled(id) => {
            model.scene.toggle_visibility(id);
        }
        Message::NodeExpandToggled(id) => {
            if !model.expanded.remove(&id) {
                model.expanded.insert(id);
            }
        }
        Message::TransformPositionChanged(id, axis, value) => {
            if let Some(node) = model.scene.find_mut(id) {
                node.transform.position[axis_index(axis)] = value;
            }
        }
        Message::TransformRotationChanged(id, axis, value) => {
            if let Some(node) = model.scene.find_mut(id) {
                node.transform.rotation[axis_index(axis)] = value;
            }
        }
        Message::ExitTimerTick => {
            if let Some(deadline) = model.exit_deadline
                && std::time::Instant::now() >= deadline
            {
                log::info!("self-imposed lifetime reached; exiting");
                return iced::exit();
            }
        }
    }

    iced::Task::none()
}

fn axis_index(axis: crate::message::Axis) -> usize {
    match axis {
        crate::message::Axis::X => 0,
        crate::message::Axis::Y => 1,
        crate::message::Axis::Z => 2,
    }
}
