//! Offscreen GPU smoke test: creates a device, compiles the grid+axis
//! shaders, builds the pipelines, and renders N frames to an offscreen
//! texture. Never creates a window or a surface, so it cannot involve or
//! wedge a compositor. See docs/troubleshooting-graphics.md.

use iced::wgpu;
use iced::widget::shader::Pipeline as _;
use iced::{Rectangle, Size};
use kagami_renderer::{Camera, GridAxisPipeline, Uniforms};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn main() {
    let frames: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);

    pollster::block_on(run(frames));
}

async fn run(frames: u32) {
    let instance = wgpu::Instance::default();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("no suitable GPU adapter found");

    println!("adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("failed to create device");

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("smoke offscreen target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let pipeline = GridAxisPipeline::new(&device, &queue, FORMAT);

    let bounds = Rectangle {
        x: 0.0,
        y: 0.0,
        width: WIDTH as f32,
        height: HEIGHT as f32,
    };
    let clip_bounds = Rectangle {
        x: 0,
        y: 0,
        width: WIDTH,
        height: HEIGHT,
    };
    let aspect_ratio = bounds.width / bounds.height;

    let mut camera = Camera::default();

    let start = std::time::Instant::now();

    for frame in 0..frames {
        camera.orbit(4.0, 0.0);

        pipeline.write_uniforms(
            &queue,
            &Uniforms {
                view_proj: camera
                    .view_projection_matrix(aspect_ratio)
                    .to_cols_array_2d(),
                camera_pos: {
                    let eye = camera.eye();
                    [eye.x, eye.y, eye.z, 0.0]
                },
            },
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("smoke frame encoder"),
        });

        pipeline.render(&mut encoder, &view, clip_bounds);

        queue.submit(Some(encoder.finish()));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll failed");

        if frame % 20 == 0 {
            println!("frame {frame}/{frames} ok");
        }
    }

    let _ = Size::new(WIDTH, HEIGHT);
    println!(
        "smoke test complete: {frames} frames in {:?}, no window/surface created",
        start.elapsed()
    );
}
