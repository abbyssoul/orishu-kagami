//! Pixel assertions over the same program/primitive path the window uses.
use super::{FORMAT, HEIGHT, WIDTH};
use iced::widget::shader::{Pipeline as _, Primitive as _, Program, Viewport};
use iced::{Rectangle, Size, mouse, wgpu};
use kagami_renderer::{
    Arrow, ArrowBatch, Camera, CameraMotion, GridAxisPipeline, Marker, MarkerBatch, Projection,
    SceneProgram,
};
use std::sync::Arc;

pub fn verify(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    let mut pipeline = GridAxisPipeline::new(device, queue, FORMAT);
    let marker = |position, color| Marker::new(position, color).unwrap();
    let batch = Arc::new(
        MarkerBatch::new(vec![
            marker([2., 0., 0.], [1., 0., 0.]),
            // Farther, submitted last: depth, not submission order, must win.
            marker([-2., 0., 0.], [0., 0., 1.]),
            marker([0., 3., 2.], [0., 0., 1.]),
            marker([30., 0., 0.], [0., 1., 0.]), // behind the camera
        ])
        .unwrap(),
    );
    for projection in [Projection::Perspective, Projection::Orthographic] {
        let camera = Camera::new([0.; 3], 20., 0., 0., projection);
        let program = SceneProgram::new(camera).with_markers(batch.clone());
        for _ in 0..2 {
            // exercise both upload and unchanged-Arc reuse
            let pixels = draw(device, queue, texture, &mut pipeline, &program);
            assert_eq!(pixel(&pixels, WIDTH / 2, HEIGHT / 2), [255, 0, 0, 255]);
            assert!(pixels.chunks_exact(4).any(|p| p == [0, 0, 255, 255]));
            assert!(!pixels.chunks_exact(4).any(|p| p == [0, 255, 0, 255]));
        }
        let moved = SceneProgram::new(camera).with_markers(Arc::new(
            MarkerBatch::new(vec![marker([2., 0., 3.], [1., 0., 0.])]).unwrap(),
        ));
        let pixels = draw(device, queue, texture, &mut pipeline, &moved);
        assert_ne!(pixel(&pixels, WIDTH / 2, HEIGHT / 2), [255, 0, 0, 255]);
        assert!(pixels.chunks_exact(4).any(|p| p == [255, 0, 0, 255]));
        let pixels = draw(
            device,
            queue,
            texture,
            &mut pipeline,
            &SceneProgram::new(camera),
        );
        assert!(!pixels.chunks_exact(4).any(|p| p == [255, 0, 0, 255]));
    }
    println!(
        "marker pixel checks passed: both projections, depth, clipping, reuse, replacement and removal"
    );
    verify_arrows(device, queue, texture);
}

fn verify_arrows(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    let mut pipeline = GridAxisPipeline::new(device, queue, FORMAT);
    let arrows = Arc::new(
        ArrowBatch::new(vec![
            Arrow::new([-2., -3., 0.], [-2., 3., 0.], [0., 0., 1.]).unwrap(),
            Arrow::new([0., -3., 2.], [0., 3., 2.], [1., 0., 1.]).unwrap(),
            Arrow::new([30., -3., 0.], [30., 3., 0.], [0., 1., 0.]).unwrap(),
        ])
        .unwrap(),
    );
    let markers =
        Arc::new(MarkerBatch::new(vec![Marker::new([2., 0., 0.], [1., 0., 0.]).unwrap()]).unwrap());
    for projection in [Projection::Perspective, Projection::Orthographic] {
        let camera = Camera::new([0.; 3], 20., 0., 0., projection);
        let program = SceneProgram::new(camera)
            .with_markers(markers.clone())
            .with_arrows(arrows.clone());
        for _ in 0..2 {
            let pixels = draw(device, queue, texture, &mut pipeline, &program);
            assert_eq!(
                pixel(&pixels, WIDTH / 2, HEIGHT / 2),
                [255, 0, 0, 255],
                "far arrow must not paint over a nearer object"
            );
            assert!(pixels.chunks_exact(4).any(|p| p == [0, 0, 255, 255]));
            assert!(pixels.chunks_exact(4).any(|p| p == [255, 0, 255, 255]));
            assert!(!pixels.chunks_exact(4).any(|p| p == [0, 255, 0, 255]));
        }
        // Camera movement reuses the same immutable geometry upload.
        let moved = SceneProgram::new(Camera::new([0.; 3], 20., 0.4, 0.3, projection))
            .with_arrows(arrows.clone());
        let pixels = draw(device, queue, texture, &mut pipeline, &moved);
        assert!(pixels.chunks_exact(4).any(|p| p == [255, 0, 255, 255]));
        let empty =
            SceneProgram::new(camera).with_arrows(Arc::new(ArrowBatch::new(vec![]).unwrap()));
        let pixels = draw(device, queue, texture, &mut pipeline, &empty);
        assert!(!pixels.chunks_exact(4).any(|p| p == [255, 0, 255, 255]));
        let pixels = draw(
            device,
            queue,
            texture,
            &mut pipeline,
            &SceneProgram::new(camera),
        );
        assert!(!pixels.chunks_exact(4).any(|p| p == [0, 0, 255, 255]));
    }
    println!(
        "arrow pixel checks passed: both projections, shared object depth, clipping, camera reuse, empty replacement and removal"
    );
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * WIDTH + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pipeline: &mut GridAxisPipeline,
    program: &SceneProgram,
) -> Vec<u8> {
    let bounds = Rectangle::new(iced::Point::ORIGIN, Size::new(WIDTH as f32, HEIGHT as f32));
    let viewport = Viewport::with_physical_size(Size::new(WIDTH, HEIGHT), 1.0);
    let primitive = <SceneProgram as Program<CameraMotion>>::draw(
        program,
        &Default::default(),
        mouse::Cursor::Unavailable,
        bounds,
    );
    primitive.prepare(pipeline, device, queue, &bounds, &viewport);
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pixel test clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    primitive.render(
        pipeline,
        &mut encoder,
        &view,
        &Rectangle {
            x: 0,
            y: 0,
            width: WIDTH,
            height: HEIGHT,
        },
    );
    let stride = (WIDTH * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pixel assertions readback"),
        size: u64::from(stride * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(HEIGHT),
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in mapped.chunks_exact(stride as usize) {
        pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}
