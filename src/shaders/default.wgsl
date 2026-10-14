struct TransformUniform {
    model_matrix: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> transform: TransformUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = transform.model_matrix * vec4<f32>(in.position, 1.0);
    out.tex_coords = in.tex_coords;
    out.world_normal = in.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);

    // Directional light from front-top-right
    let light_dir = normalize(vec3<f32>(0.5, 0.8, 1.0));
    let n = normalize(in.world_normal);
    let diff = max(dot(n, light_dir), 0.0);

    // Ambient + diffuse shading
    let ambient = 0.3;
    let lighting = ambient + diff * 0.7;

    // Warm terracotta/bronze base color
    let base_color = vec3<f32>(0.9, 0.65, 0.45);
    let final_rgb = base_color * tex_color.rgb * lighting;

    return vec4<f32>(final_rgb, 1.0);
}
