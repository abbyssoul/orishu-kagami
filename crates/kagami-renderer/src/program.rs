use crate::camera::Camera;
use crate::primitive::ScenePrimitive;
use iced::keyboard;
use iced::mouse::{self, Button, ScrollDelta};
use iced::widget::shader::{self, Action};
use iced::{Event, Rectangle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Orbit,
    Pan,
}

#[derive(Debug, Default)]
pub struct CameraInputState {
    pub camera: Camera,
    drag: Option<DragMode>,
    last_cursor: Option<iced::Point>,
    modifiers: keyboard::Modifiers,
}

/// The `shader::Program` that drives the 3D viewport: an orbit/pan/dolly
/// camera over an empty grid + axis stage.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneProgram;

impl<Message> shader::Program<Message> for SceneProgram {
    type State = CameraInputState;
    type Primitive = ScenePrimitive;

    fn update(
        &self,
        state: &mut CameraInputState,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(Button::Middle)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.drag = Some(if state.modifiers.shift() {
                        DragMode::Pan
                    } else {
                        DragMode::Orbit
                    });
                    state.last_cursor = Some(position);
                    return Some(Action::capture());
                }

                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Middle)) => {
                if state.drag.take().is_some() {
                    state.last_cursor = None;
                    return Some(Action::capture());
                }

                None
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let mode = state.drag?;
                let last = state.last_cursor.unwrap_or(*position);
                let delta_x = position.x - last.x;
                let delta_y = position.y - last.y;

                match mode {
                    DragMode::Orbit => state.camera.orbit(delta_x, delta_y),
                    DragMode::Pan => state.camera.pan(delta_x, delta_y),
                }

                state.last_cursor = Some(*position);
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !cursor.is_over(bounds) {
                    return None;
                }

                let amount = match delta {
                    ScrollDelta::Lines { y, .. } => *y,
                    ScrollDelta::Pixels { y, .. } => *y * 0.02,
                };

                state.camera.dolly(amount);
                Some(Action::request_redraw().and_capture())
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &CameraInputState,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> ScenePrimitive {
        let aspect_ratio = if bounds.height > 0.0 {
            bounds.width / bounds.height
        } else {
            1.0
        };

        ScenePrimitive {
            view_proj: state
                .camera
                .view_projection_matrix(aspect_ratio)
                .to_cols_array_2d(),
            camera_pos: state.camera.eye().into(),
        }
    }

    fn mouse_interaction(
        &self,
        state: &CameraInputState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::default()
        }
    }
}
