//! Immutable presentation arrows. Scientific meaning, units and normalization
//! are caller-owned; the renderer consumes only finite render-space endpoints.
use iced::wgpu;
use std::sync::Arc;

/// Maximum arrows in one presentation packet (144 KiB of GPU instance data).
pub const MAX_ARROWS: usize = 4096;

/// A direction glyph, not a physical surface or scientific vector value.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Arrow {
    start: [f32; 3],
    end: [f32; 3],
    color: [f32; 3],
}
impl Arrow {
    /// Reject nonfinite/degenerate endpoints or colors outside [0, 1].
    /// The caller owns its tighter resolvable scene extent/precision policy.
    pub fn new(start: [f32; 3], end: [f32; 3], color: [f32; 3]) -> Option<Self> {
        (start != end
            && start.iter().chain(&end).all(|v| v.is_finite())
            && color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
        .then_some(Self { start, end, color })
    }
    pub fn start(&self) -> [f32; 3] {
        self.start
    }
    pub fn end(&self) -> [f32; 3] {
        self.end
    }
}

/// Build once per observation/presentation change, not per frame. Pointer
/// identity lets unchanged frames reuse the uploaded GPU instance buffer.
#[derive(Debug)]
pub struct ArrowBatch {
    arrows: Vec<Arrow>,
}
impl ArrowBatch {
    /// Reject oversized or invalid Pod input, never silently truncate it.
    pub fn new(arrows: Vec<Arrow>) -> Option<Self> {
        (arrows.len() <= MAX_ARROWS
            && arrows
                .iter()
                .all(|a| Arrow::new(a.start, a.end, a.color).is_some()))
        .then_some(Self { arrows })
    }
    pub fn arrows(&self) -> &[Arrow] {
        &self.arrows
    }
}

pub(crate) struct ArrowPipeline {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
    cached: Option<Arc<ArrowBatch>>,
}
impl ArrowPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        layout: &wgpu::PipelineLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("direction arrows"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/arrows.wgsl").into()),
        });
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("direction arrow pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Arrow>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                }],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arrow instances"),
            size: std::mem::size_of::<Arrow>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            capacity: 1,
            count: 0,
            cached: None,
        }
    }
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: Option<&Arc<ArrowBatch>>,
    ) {
        self.count = batch.map_or(0, |b| b.arrows.len() as u32);
        if let Some(batch) = batch {
            if !self
                .cached
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, batch))
            {
                if batch.arrows.len() > self.capacity {
                    self.capacity = batch.arrows.len().next_power_of_two();
                    self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("reused arrow instances"),
                        size: (self.capacity * std::mem::size_of::<Arrow>()) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                if self.count > 0 {
                    queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&batch.arrows));
                }
                self.cached = Some(batch.clone());
            }
        } else {
            self.cached = None;
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Draw inside the position-glyph pass so arrows and objects share depth.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..9, 0..self.count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_pod_and_capacity() {
        assert!(Arrow::new([0.; 3], [0.; 3], [1.; 3]).is_none());
        assert!(Arrow::new([0.; 3], [f32::INFINITY; 3], [1.; 3]).is_none());
        assert!(Arrow::new([0.; 3], [1.; 3], [-1.; 3]).is_none());
        assert!(ArrowBatch::new(vec![bytemuck::cast([f32::NAN; 9])]).is_none());
        let arrow = Arrow::new([0.; 3], [1.; 3], [1.; 3]).unwrap();
        assert!(ArrowBatch::new(vec![arrow; MAX_ARROWS + 1]).is_none());
        assert_eq!(
            ArrowBatch::new(vec![arrow; MAX_ARROWS])
                .unwrap()
                .arrows()
                .len(),
            MAX_ARROWS
        );
    }
}
