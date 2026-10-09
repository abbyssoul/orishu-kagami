struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport: vec4<f32>, // logical width, height, marker radius, unused
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec3<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) vertex: u32,
    @location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> Output {
    let corners = array<vec2<f32>, 6>(vec2(-1., -1.), vec2(1., -1.), vec2(1., 1.), vec2(-1., -1.), vec2(1., 1.), vec2(-1., 1.));
    let local = corners[vertex];
    var clip = uniforms.view_proj * vec4(position, 1.);
    clip = vec4(clip.xy + local * (2. * uniforms.viewport.z / max(uniforms.viewport.xy, vec2(1.))) * clip.w, clip.zw);
    return Output(clip, local, color);
}
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    if dot(input.local, input.local) > 1. { discard; }
    return vec4(input.color, 1.);
}
