// Foliage / Grass シェーダー (Round 8)
//
// インスタンスドメッシュとしてレンダリングし、頂点シェーダーで風揺れを加える。
// 草の上部 (UV.y > 0.5) が大きく揺れる重み付け。

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
    _pad0: f32,
    viewport: vec4<f32>,
    camera_params: vec4<f32>,
    time_params: vec4<f32>,
};

struct FoliageUniform {
    /// xyz = wind direction, w = wind strength
    wind: vec4<f32>,
    /// x = sway frequency, y = phase variance, z = bend curve, w = _
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> foliage: FoliageUniform;
@group(1) @binding(1) var t_color: texture_2d<f32>;
@group(1) @binding(2) var s_color: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    // インスタンス mat4
    @location(6) instance_row0: vec4<f32>,
    @location(7) instance_row1: vec4<f32>,
    @location(8) instance_row2: vec4<f32>,
    @location(9) instance_row3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let instance_mat = mat4x4<f32>(
        in.instance_row0,
        in.instance_row1,
        in.instance_row2,
        in.instance_row3,
    );

    var world_pos = (instance_mat * vec4<f32>(in.position, 1.0)).xyz;

    // 風揺れ: 草の上部ほど大きく動く
    let height_weight = clamp(in.tex_coords.y, 0.0, 1.0);
    let bend_weight = pow(height_weight, foliage.params.z);

    // 各インスタンスの位置からハッシュ風 phase を作る
    let phase = sin(instance_mat[3].x * 0.7 + instance_mat[3].z * 0.3) * foliage.params.y;
    let time = camera.time_params.x;
    let sway = sin(time * foliage.params.x + phase) * foliage.wind.w * bend_weight;

    let wind_dir = normalize(foliage.wind.xyz);
    world_pos = world_pos + wind_dir * sway;

    out.world_pos = world_pos;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);

    // 法線はインスタンスから抽出
    let n_world = normalize((instance_mat * vec4<f32>(in.normal, 0.0)).xyz);
    out.world_normal = n_world;
    out.tex_coords = in.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(t_color, s_color, in.tex_coords);
    if tex.a < 0.3 {
        discard;
    }

    // 単純なディフューズ + 環境光
    let l = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let n_dot_l = max(dot(in.world_normal, l), 0.0);
    let ambient = 0.4;
    let lit = tex.rgb * (ambient + n_dot_l * 0.7);
    return vec4<f32>(lit, tex.a);
}
