struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// --- Grid pipeline -----------------------------------------------------

struct GridVertexInput {
    @location(0) position: vec3<f32>,
};

struct GridVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
};

@vertex
fn grid_vs_main(input: GridVertexInput) -> GridVertexOutput {
    var out: GridVertexOutput;
    out.world_position = input.position;
    out.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    return out;
}

fn grid_line_mask(coord: vec2<f32>, cell_size: f32) -> f32 {
    let scaled = coord / cell_size;
    let derivative = fwidth(scaled);
    let grid = abs(fract(scaled - 0.5) - 0.5) / max(derivative, vec2<f32>(0.0001));
    return 1.0 - min(min(grid.x, grid.y), 1.0);
}

@fragment
fn grid_fs_main(in: GridVertexOutput) -> @location(0) vec4<f32> {
    let distance_to_camera = length(in.world_position.xy - uniforms.camera_pos.xy);
    let fade = clamp(1.0 - distance_to_camera / 300.0, 0.0, 1.0);

    let minor = grid_line_mask(in.world_position.xy, 1.0);
    let major = grid_line_mask(in.world_position.xy, 10.0);
    let line_strength = max(minor * 0.35, major * 0.75);

    let alpha = line_strength * fade;
    if (alpha <= 0.001) {
        discard;
    }

    return vec4<f32>(vec3<f32>(0.55, 0.6, 0.68), alpha);
}

// --- Axis pipeline -------------------------------------------------------

struct AxisVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct AxisVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn axis_vs_main(input: AxisVertexInput) -> AxisVertexOutput {
    var out: AxisVertexOutput;
    out.color = input.color;
    out.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    return out;
}

@fragment
fn axis_fs_main(in: AxisVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
