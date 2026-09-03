use crate::pipeline::{GridAxisPipeline, Uniforms};
use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};

#[derive(Debug, Clone, Copy)]
pub struct ScenePrimitive {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
}

impl shader::Primitive for ScenePrimitive {
    type Pipeline = GridAxisPipeline;

    fn prepare(
        &self,
        pipeline: &mut GridAxisPipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        pipeline.write_uniforms(
            queue,
            &Uniforms {
                view_proj: self.view_proj,
                camera_pos: [
                    self.camera_pos[0],
                    self.camera_pos[1],
                    self.camera_pos[2],
                    0.0,
                ],
            },
        );
    }

    fn render(
        &self,
        pipeline: &GridAxisPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, *clip_bounds);
    }
}
