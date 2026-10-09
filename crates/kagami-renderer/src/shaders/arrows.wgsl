struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) vertex: u32,
    @location(0) start: vec3<f32>, @location(1) end: vec3<f32>, @location(2) color: vec3<f32>) -> Output {
    let a = uniforms.view_proj * vec4(start, 1.);
    let b = uniforms.view_proj * vec4(end, 1.);
    // Do not divide points behind the camera or connect through a clip plane.
    // A partially near/far-clipped glyph is deliberately omitted as a whole.
    if a.w <= 0. || b.w <= 0. || a.z < 0. || b.z < 0. || a.z > a.w || b.z > b.w {
        return Output(vec4(2., 2., 2., 1.), color);
    }
    let viewport = max(uniforms.viewport.xy, vec2(1.));
    let delta = (b.xy / b.w - a.xy / a.w) * viewport * 0.5;
    let distance = length(delta);
    // View-axis or subpixel arrows have no meaningful visible direction.
    if distance < 1. { return Output(vec4(2., 2., 2., 1.), color); }
    let direction = delta / distance;
    let normal = vec2(-direction.y, direction.x);
    let head = min(10., distance * 0.4);
    // Shaft quad followed by the tip triangle. Width is in logical pixels;
    // endpoints/depth still come from the real render-space segment.
    let shaft = array<vec2<f32>, 6>(vec2(0., -1.), vec2(1., -1.), vec2(1., 1.), vec2(0., -1.), vec2(1., 1.), vec2(0., 1.));
    var clip: vec4<f32>;
    var offset: vec2<f32>;
    if vertex < 6u {
        let corner = shaft[vertex];
        clip = select(a, b, corner.x > 0.5);
        offset = normal * corner.y * 1.2 - direction * head * corner.x;
    } else {
        clip = b;
        let side = f32(vertex) - 7.;
        offset = select(-direction * head + normal * side * head * 0.45, vec2(0.), vertex == 7u);
    }
    clip = vec4(clip.xy + offset * 2. / viewport * clip.w, clip.zw);
    return Output(clip, color);
}
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> { return vec4(input.color, 1.); }
