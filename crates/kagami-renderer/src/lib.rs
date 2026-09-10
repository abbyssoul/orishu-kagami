//! The 3D rendering engine: a perspective/orthographic orbit camera driving a
//! wgpu grid+axis "stage", exposed as an `iced::widget::shader::Program`.
//!
//! The camera is an input, not state. [`SceneProgram`] is handed a [`Camera`]
//! each frame and publishes a [`CameraMotion`] for every gesture it
//! recognises, leaving the pose and its bounds to whoever has to save them
//! (ADR 0022). Only pointer bookkeeping stays in the widget.

mod camera;
mod geometry;
mod pipeline;
mod primitive;
mod program;

pub use camera::{Camera, Projection};
pub use pipeline::{GridAxisPipeline, Uniforms};
pub use primitive::ScenePrimitive;
pub use program::{CameraMotion, PointerState, SceneProgram};
