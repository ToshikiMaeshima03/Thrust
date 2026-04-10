// Geometry G-Buffer Prepass (Round 7)
//
// 1× 非 MSAA で、後段のスクリーン空間エフェクト (SSAO/SSR/decals/motion blur) が
// 必要とする情報をすべて 1 パスで書き込む。
//
// 出力アタッチメント:
//   0: Rgba16Float — view-space normal (xyz, encoded 0..1) + linear depth (w)
//   1: Rgba8Unorm  — metallic (r), roughness (g), specular weight (b), id-mask (a)
//   2: Rg16Float   — motion vector (xy = current_uv - prev_uv)

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

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct MaterialUniform {
    base_color_factor: vec4<f32>,
    mr_no: vec4<f32>,
    emissive: vec4<f32>,
    texture_flags: vec4<u32>,
    extended: vec4<f32>,
    aniso_dir: vec4<f32>,
    subsurface_color: vec4<f32>,
    _padding2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model_data: ModelUniform;
@group(1) @binding(1) var<storage, read> joint_matrices: array<mat4x4<f32>>;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var t_base_color: texture_2d<f32>;
@group(2) @binding(2) var s_base_color: sampler;
@group(2) @binding(3) var t_metallic_roughness: texture_2d<f32>;
@group(2) @binding(4) var t_normal: texture_2d<f32>;
@group(2) @binding(5) var t_occlusion: texture_2d<f32>;
@group(2) @binding(6) var t_emissive: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_normal: vec3<f32>,
    @location(1) view_tangent: vec3<f32>,
    @location(2) view_bitangent: vec3<f32>,
    @location(3) view_z: f32,
    @location(4) tex_coords: vec2<f32>,
    @location(5) curr_clip: vec4<f32>,
    @location(6) prev_clip: vec4<f32>,
};

fn compute_skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let total = weights.x + weights.y + weights.z + weights.w;
    if total < 0.001 {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }
    return joint_matrices[joints.x] * weights.x
         + joint_matrices[joints.y] * weights.y
         + joint_matrices[joints.z] * weights.z
         + joint_matrices[joints.w] * weights.w;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let skin_mat = compute_skin_matrix(in.joints, in.weights);
    let skinned_pos = (skin_mat * vec4<f32>(in.position, 1.0)).xyz;
    let skinned_normal = (skin_mat * vec4<f32>(in.normal, 0.0)).xyz;
    let skinned_tangent = (skin_mat * vec4<f32>(in.tangent.xyz, 0.0)).xyz;

    let world_pos = model_data.model * vec4<f32>(skinned_pos, 1.0);
    out.clip_position = camera.view_proj * world_pos;

    // ビュー空間法線/接線/従法線
    let view_world = mat3x3<f32>(
        camera.view[0].xyz,
        camera.view[1].xyz,
        camera.view[2].xyz,
    );
    let world_normal = normalize((model_data.normal_matrix * vec4<f32>(skinned_normal, 0.0)).xyz);
    let world_tangent = normalize((model_data.model * vec4<f32>(skinned_tangent, 0.0)).xyz);
    out.view_normal = normalize(view_world * world_normal);
    out.view_tangent = normalize(view_world * world_tangent);
    out.view_bitangent = cross(out.view_normal, out.view_tangent) * in.tangent.w;
    out.view_z = (camera.view * world_pos).z;
    out.tex_coords = in.tex_coords;

    out.curr_clip = out.clip_position;
    out.prev_clip = camera.prev_view_proj * world_pos;
    return out;
}

struct GBufferOutput {
    @location(0) normal_depth: vec4<f32>,
    @location(1) material: vec4<f32>,
    @location(2) motion: vec2<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> GBufferOutput {
    var out: GBufferOutput;
    let flags = material.texture_flags.x;

    // ノーマルマップを適用したビュー空間法線
    var n = normalize(in.view_normal);
    if (flags & 4u) != 0u {
        let tangent_normal = textureSample(t_normal, s_base_color, in.tex_coords).xyz * 2.0 - 1.0;
        let scaled = vec3<f32>(
            tangent_normal.x * material.mr_no.z,
            tangent_normal.y * material.mr_no.z,
            tangent_normal.z,
        );
        let t = normalize(in.view_tangent);
        let b = normalize(in.view_bitangent);
        let tbn = mat3x3<f32>(t, b, n);
        n = normalize(tbn * scaled);
    }

    // 法線を [0,1] にエンコード
    let encoded_normal = n * 0.5 + 0.5;

    // リニア深度 (camera params から計算)
    let z_far = camera.camera_params.y;
    let lin_depth = -in.view_z / z_far;

    out.normal_depth = vec4<f32>(encoded_normal, lin_depth);

    // メタリック・ラフネス取得
    var metallic = material.mr_no.x;
    var roughness = material.mr_no.y;
    if (flags & 2u) != 0u {
        let mr = textureSample(t_metallic_roughness, s_base_color, in.tex_coords);
        roughness = roughness * mr.g;
        metallic = metallic * mr.b;
    }
    roughness = clamp(roughness, 0.04, 1.0);
    out.material = vec4<f32>(metallic, roughness, 0.04, 1.0);

    // モーションベクトル (current uv - previous uv)
    let curr_ndc = in.curr_clip.xy / max(in.curr_clip.w, 1e-5);
    let prev_ndc = in.prev_clip.xy / max(in.prev_clip.w, 1e-5);
    let curr_uv = vec2<f32>(curr_ndc.x * 0.5 + 0.5, -curr_ndc.y * 0.5 + 0.5);
    let prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, -prev_ndc.y * 0.5 + 0.5);
    out.motion = curr_uv - prev_uv;

    return out;
}
