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

/// One camera gesture the viewport recognised, in pointer-travel units.
///
/// Published rather than applied. Where the camera is has to survive a save
/// (ADR 0022), so the pose belongs to the application and its bounds belong to
/// `kagami-session`; this crate reports what the pointer did and lets the
/// owner decide what it means. That also makes the whole gesture visible to
/// the application, which is what a viewport drag needs in order to become one
/// undo entry once interactive edits arrive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMotion {
    /// Swing around the target.
    Orbit {
        /// Horizontal pointer travel.
        dx: f32,
        /// Vertical pointer travel.
        dy: f32,
    },
    /// Slide the target across the view plane.
    Pan {
        /// Horizontal pointer travel.
        dx: f32,
        /// Vertical pointer travel.
        dy: f32,
    },
    /// Move towards or away from the target. Positive moves closer.
    Dolly {
        /// How far.
        amount: f32,
    },
}

/// What the widget itself has to remember between events.
///
/// Only the pointer bookkeeping: which button is held, where the cursor was
/// last, which modifiers are down. All of it is transient by nature — nothing
/// here is worth saving, and nothing here is the camera. That separation is
/// why the camera no longer resets when the widget's position in the tree
/// changes.
#[derive(Debug, Default)]
pub struct PointerState {
    drag: Option<DragMode>,
    last_cursor: Option<iced::Point>,
    modifiers: keyboard::Modifiers,
}

/// The `shader::Program` that draws the 3D viewport: a grid + axis stage seen
/// through the camera it is handed.
///
/// Holds the camera as a value the application supplies each frame, so the
/// window's pose is the one thing being drawn and the one thing being saved.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneProgram {
    /// The camera to draw through.
    pub camera: Camera,
}

impl SceneProgram {
    /// Draw through `camera`.
    pub const fn new(camera: Camera) -> Self {
        Self { camera }
    }
}

impl<Message> shader::Program<Message> for SceneProgram
where
    Message: From<CameraMotion>,
{
    type State = PointerState;
    type Primitive = ScenePrimitive;

    fn update(
        &self,
        state: &mut PointerState,
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
                let dx = position.x - last.x;
                let dy = position.y - last.y;
                state.last_cursor = Some(*position);

                // A move that travelled nowhere is not a gesture. Publishing it
                // would ask the application to consider a camera change that
                // cannot change anything.
                if dx == 0.0 && dy == 0.0 {
                    return Some(Action::capture());
                }

                let motion = match mode {
                    DragMode::Orbit => CameraMotion::Orbit { dx, dy },
                    DragMode::Pan => CameraMotion::Pan { dx, dy },
                };
                Some(Action::publish(motion.into()).and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !cursor.is_over(bounds) {
                    return None;
                }

                let amount = match delta {
                    ScrollDelta::Lines { y, .. } => *y,
                    ScrollDelta::Pixels { y, .. } => *y * 0.02,
                };
                if amount == 0.0 {
                    return None;
                }

                Some(Action::publish(CameraMotion::Dolly { amount }.into()).and_capture())
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
        _state: &PointerState,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> ScenePrimitive {
        let aspect_ratio = if bounds.height > 0.0 {
            bounds.width / bounds.height
        } else {
            1.0
        };

        ScenePrimitive {
            view_proj: self
                .camera
                .view_projection_matrix(aspect_ratio)
                .to_cols_array_2d(),
            camera_pos: self.camera.eye().into(),
        }
    }

    fn mouse_interaction(
        &self,
        state: &PointerState,
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
