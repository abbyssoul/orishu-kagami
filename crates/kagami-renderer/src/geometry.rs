use iced::wgpu;

pub const GRID_EXTENT: f32 = 1000.0;

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GridVertex {
    pub position: [f32; 3],
}

impl GridVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// A single quad on the Z=0 plane; the grid lines are computed procedurally
/// in the fragment shader from world-space position.
pub const GRID_VERTICES: [GridVertex; 6] = [
    GridVertex {
        position: [-GRID_EXTENT, -GRID_EXTENT, 0.0],
    },
    GridVertex {
        position: [GRID_EXTENT, -GRID_EXTENT, 0.0],
    },
    GridVertex {
        position: [GRID_EXTENT, GRID_EXTENT, 0.0],
    },
    GridVertex {
        position: [-GRID_EXTENT, -GRID_EXTENT, 0.0],
    },
    GridVertex {
        position: [GRID_EXTENT, GRID_EXTENT, 0.0],
    },
    GridVertex {
        position: [-GRID_EXTENT, GRID_EXTENT, 0.0],
    },
];

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct AxisVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl AxisVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
    ];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

const AXIS_LENGTH: f32 = 1000.0;
const RED: [f32; 3] = [0.85, 0.25, 0.25];
const GREEN: [f32; 3] = [0.3, 0.75, 0.3];
const BLUE: [f32; 3] = [0.3, 0.5, 0.9];

pub const AXIS_VERTICES: [AxisVertex; 6] = [
    AxisVertex {
        position: [0.0, 0.0, 0.0],
        color: RED,
    },
    AxisVertex {
        position: [AXIS_LENGTH, 0.0, 0.0],
        color: RED,
    },
    AxisVertex {
        position: [0.0, 0.0, 0.0],
        color: GREEN,
    },
    AxisVertex {
        position: [0.0, AXIS_LENGTH, 0.0],
        color: GREEN,
    },
    AxisVertex {
        position: [0.0, 0.0, 0.0],
        color: BLUE,
    },
    AxisVertex {
        position: [0.0, 0.0, AXIS_LENGTH],
        color: BLUE,
    },
];
