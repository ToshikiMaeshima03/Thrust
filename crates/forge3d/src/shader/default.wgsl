// forge3d デフォルトシェーダー

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct LightUniform {
    direction: vec3<f32>,
    intensity: f32,
    color: vec3<f32>,
    ambient_intensity: f32,
    ambient_color: vec3<f32>,
    _padding: f32,
};

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct MaterialUniform {
    base_color: vec4<f32>,
    has_texture: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> light: LightUniform;
@group(1) @binding(0) var<uniform> model_data: ModelUniform;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var t_diffuse: texture_2d<f32>;
@group(2) @binding(2) var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = model_data.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;
    out.world_normal = normalize((model_data.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.tex_coords = in.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let l = normalize(light.direction);

    let ambient = light.ambient_color * light.ambient_intensity;
    let diffuse = max(dot(n, l), 0.0) * light.color * light.intensity;
    let lit = ambient + diffuse;

    var base_color = material.base_color;
    if (material.has_texture > 0u) {
        let tex_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
        base_color = base_color * tex_color;
    }

    return vec4<f32>(base_color.rgb * lit, base_color.a);
}
