//! Bounded generic position glyphs. Scientific identities and metre conversion
//! belong to the caller; a marker is not an object's physical radius or mesh.
use iced::wgpu::{self, util::DeviceExt};
use std::sync::Arc;

/// Maximum glyphs in one presentation packet (1.5 MiB of instance data).
pub const MAX_MARKERS: usize = 65_536;

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// A point glyph in render units, not a physical surface or radius.
pub struct Marker {
    position: [f32; 3],
    color: [f32; 3],
}
impl Marker {
    /// Require finite render coordinates and an RGB color in [0, 1]. Caller
    /// owns its tighter scene precision/extent policy, not this presentation API.
    pub fn new(position: [f32; 3], color: [f32; 3]) -> Option<Self> {
        (position.iter().all(|x| x.is_finite())
            && color
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x)))
        .then_some(Self { position, color })
    }
    pub fn position(&self) -> [f32; 3] {
        self.position
    }
    pub fn color(&self) -> [f32; 3] {
        self.color
    }
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBUTES,
        }
    }
}

/// Immutable presentation packet; pointer identity makes unchanged-frame upload
/// O(1). Construct once on snapshot/scale change, never inside a render loop.
#[derive(Debug)]
pub struct MarkerBatch {
    markers: Vec<Marker>,
}
impl MarkerBatch {
    /// Refuse oversized input rather than truncate it without caller diagnostics.
    pub fn new(markers: Vec<Marker>) -> Option<Self> {
        // Pod permits construction through byte casts as well as `new`.
        (markers.len() <= MAX_MARKERS
            && markers
                .iter()
                .all(|m| Marker::new(m.position, m.color).is_some()))
        .then_some(Self { markers })
    }
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }
}

pub(crate) struct MarkerPipeline {
    arrows: crate::arrows::ArrowPipeline,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    capacity: usize,
    count: u32,
    cached: Option<Arc<MarkerBatch>>,
    depth: Option<(iced::Size<u32>, wgpu::TextureView)>,
}
impl MarkerPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        layout: &wgpu::PipelineLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("position markers"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/markers.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("position marker pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Marker::layout()],
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
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("empty marker buffer"),
            contents: &[0; 24],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            arrows: crate::arrows::ArrowPipeline::new(device, format, layout),
            pipeline,
            vertices,
            capacity: 1,
            count: 0,
            cached: None,
            depth: None,
        }
    }
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: Option<&Arc<MarkerBatch>>,
        arrows: Option<&Arc<crate::ArrowBatch>>,
        size: iced::Size<u32>,
    ) {
        self.arrows.prepare(device, queue, arrows);
        self.count = batch.map_or(0, |b| b.markers.len() as u32);
        if let Some(batch) = batch {
            if !self
                .cached
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, batch))
            {
                if batch.markers.len() > self.capacity {
                    self.capacity = batch.markers.len().next_power_of_two();
                    self.vertices = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("reused marker instance buffer"),
                        size: (self.capacity * std::mem::size_of::<Marker>()) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                if self.count > 0 {
                    queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&batch.markers));
                }
                self.cached = Some(batch.clone());
            }
        } else {
            self.cached = None;
        }
        if (self.count > 0 || !self.arrows.is_empty())
            && size.width > 0
            && size.height > 0
            && self.depth.as_ref().is_none_or(|(old, _)| *old != size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("marker depth"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((size, texture.create_view(&Default::default())));
        }
    }
    pub(crate) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: iced::Rectangle<u32>,
        bind: &wgpu::BindGroup,
    ) {
        let Some((_, depth)) = &self.depth else {
            return;
        };
        if (self.count == 0 && self.arrows.is_empty()) || clip.width == 0 || clip.height == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("observation glyphs"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            clip.x as f32,
            clip.y as f32,
            clip.width as f32,
            clip.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..6, 0..self.count);
        self.arrows.draw(&mut pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_values_and_unannounced_truncation() {
        assert!(Marker::new([f32::NAN, 0.0, 0.0], [1.0; 3]).is_none());
        assert!(Marker::new([0.0; 3], [2.0, 0.0, 0.0]).is_none());
        let invalid: Marker = bytemuck::cast([f32::NAN; 6]);
        assert!(MarkerBatch::new(vec![invalid]).is_none());
        let marker = Marker::new([0.0; 3], [1.0; 3]).unwrap();
        assert!(MarkerBatch::new(vec![marker; MAX_MARKERS + 1]).is_none());
        assert_eq!(
            MarkerBatch::new(vec![marker; MAX_MARKERS])
                .unwrap()
                .markers()
                .len(),
            MAX_MARKERS
        );
    }
}
