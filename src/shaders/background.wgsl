struct BackgroundUniform {
    color_top: vec4<f32>,
    color_bottom: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    _padding: f32,
};

@group(0) @binding(0)
var<uniform> u_bg: BackgroundUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((in_vertex_index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(in_vertex_index & 2u) * 2.0 - 1.0;
    out.clip_position = vec4<f32>(x, y, 1.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// Pseudo-random hash for procedural starfield
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Smooth vertical cosmic gradient
    var color = mix(u_bg.color_top, u_bg.color_bottom, in.uv.y);

    // Subtle procedural starfield
    let aspect = u_bg.resolution.x / max(u_bg.resolution.y, 1.0);
    let uv_scaled = vec2<f32>(in.uv.x * aspect, in.uv.y) * 120.0;
    let grid_id = floor(uv_scaled);
    let grid_uv = fract(uv_scaled) - 0.5;

    let star_hash = hash21(grid_id);
    if star_hash > 0.985 {
        let star_pos = vec2<f32>(
            (hash21(grid_id + vec2<f32>(1.0, 0.0)) - 0.5) * 0.7,
            (hash21(grid_id + vec2<f32>(0.0, 1.0)) - 0.5) * 0.7
        );
        let dist = length(grid_uv - star_pos);
        let twinkle = 0.7 + 0.3 * sin(u_bg.time * 2.0 + star_hash * 6.28);
        let brightness = smoothstep(0.06, 0.0, dist) * twinkle;
        color = color + vec4<f32>(vec3<f32>(brightness * 0.8), 0.0);
    }

    return color;
}
