//! The 3D rendering engine: an orbit/pan/dolly camera driving a wgpu
//! grid+axis "stage", exposed as an `iced::widget::shader::Program`.

mod camera;
mod geometry;
mod pipeline;
mod primitive;
mod program;

pub use camera::Camera;
pub use pipeline::{GridAxisPipeline, Uniforms};
pub use primitive::ScenePrimitive;
pub use program::{CameraInputState, SceneProgram};
