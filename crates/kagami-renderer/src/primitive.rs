use crate::pipeline::{GridAxisPipeline, Uniforms};
use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};

#[derive(Debug, Clone)]
pub struct ScenePrimitive {
    pub markers: Option<std::sync::Arc<crate::MarkerBatch>>,
    pub arrows: Option<std::sync::Arc<crate::ArrowBatch>>,
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
}

impl shader::Primitive for ScenePrimitive {
    type Pipeline = GridAxisPipeline;

    fn prepare(
        &self,
        pipeline: &mut GridAxisPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        pipeline.write_uniforms(
            queue,
            &Uniforms {
                viewport: [bounds.width, bounds.height, 5.0, 0.0],
                view_proj: self.view_proj,
                camera_pos: [
                    self.camera_pos[0],
                    self.camera_pos[1],
                    self.camera_pos[2],
                    0.0,
                ],
            },
        );
        pipeline.prepare_glyphs(
            device,
            queue,
            self.markers.as_ref(),
            self.arrows.as_ref(),
            viewport.physical_size(),
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
