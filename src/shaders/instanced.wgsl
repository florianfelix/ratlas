struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) color_tint: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color_tint: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    var out: VertexOutput;
    let world_pos = model_matrix * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.tex_coords = in.tex_coords;
    out.world_normal = (model_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.color_tint = instance.color_tint;
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
    let ambient = 0.35;
    let lighting = ambient + diff * 0.65;

    let final_rgb = tex_color.rgb * in.color_tint.rgb * lighting;

    return vec4<f32>(final_rgb, tex_color.a * in.color_tint.a);
}
