// Screen-Space Ambient Occlusion (Round 7 — Geometry prepass 統合版)
//
// 1× 非 MSAA の depth + view-space normal を G-buffer prepass から読み取る。
// 16 サンプル半球 + ハッシュ回転で軽量だが効果的。
//
// 深度は textureLoad で読み込み、サンプラーを共有しない。

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

struct SsaoParams {
    /// x = radius, y = bias, z = intensity, w = _
    params: vec4<f32>,
    /// x = noise_scale, y = max_distance, z = _, w = _
    extra: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> ssao_params: SsaoParams;
@group(0) @binding(2) var t_normal: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var s_linear: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

/// uv -> 整数ピクセル座標
fn uv_to_pixel(uv: vec2<f32>) -> vec2<i32> {
    let dim = vec2<f32>(camera.viewport.xy);
    let p = clamp(uv * dim, vec2<f32>(0.0), dim - vec2<f32>(1.0));
    return vec2<i32>(p);
}

fn load_depth(uv: vec2<f32>) -> f32 {
    let p = uv_to_pixel(uv);
    return textureLoad(t_depth, p, 0);
}

/// UV + depth → view 空間位置 (inverse projection)
fn reconstruct_view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec3<f32>(uv.x * 2.0 - 1.0, -(uv.y * 2.0 - 1.0), depth);
    let clip = vec4<f32>(ndc, 1.0);
    let view4 = camera.inv_proj * clip;
    return view4.xyz / max(view4.w, 1e-5);
}

fn fetch_view_normal(uv: vec2<f32>) -> vec3<f32> {
    let raw = textureSampleLevel(t_normal, s_linear, uv, 0.0).xyz;
    return normalize(raw * 2.0 - 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

const SSAO_SAMPLES: u32 = 16u;

fn hemisphere_sample(i: u32, normal: vec3<f32>, rotation: vec2<f32>) -> vec3<f32> {
    let golden = 2.399963229728653;
    let idx = f32(i);
    let phi = idx * golden;
    let cos_theta = 1.0 - (idx + 0.5) / f32(SSAO_SAMPLES);
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let dir_local = vec3<f32>(
        cos(phi) * sin_theta * rotation.x - sin(phi) * sin_theta * rotation.y,
        sin(phi) * sin_theta * rotation.x + cos(phi) * sin_theta * rotation.y,
        cos_theta,
    );

    var up = vec3<f32>(0.0, 0.0, 1.0);
    if abs(normal.z) > 0.999 {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(up, normal));
    let bitangent = cross(normal, tangent);
    return tangent * dir_local.x + bitangent * dir_local.y + normal * dir_local.z;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let depth = load_depth(in.uv);
    if depth >= 0.9999 {
        return vec4<f32>(1.0);
    }

    let view_pos = reconstruct_view_position(in.uv, depth);
    let view_normal = fetch_view_normal(in.uv);

    let radius = ssao_params.params.x;
    let bias = ssao_params.params.y;
    let intensity = ssao_params.params.z;

    var occlusion = 0.0;
    let noise_rot = hash(in.uv * camera.viewport.xy) * 6.28318;
    let rotation = vec2<f32>(cos(noise_rot), sin(noise_rot));

    for (var i: u32 = 0u; i < SSAO_SAMPLES; i = i + 1u) {
        let sample_dir = hemisphere_sample(i, view_normal, rotation);
        let sample_pos = view_pos + sample_dir * radius;

        let clip = camera.proj * vec4<f32>(sample_pos, 1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let proj_xy = clip.xy / clip.w;
        let sample_uv = vec2<f32>(proj_xy.x * 0.5 + 0.5, -proj_xy.y * 0.5 + 0.5);

        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            continue;
        }

        let sample_depth = load_depth(sample_uv);
        let sample_view_pos = reconstruct_view_position(sample_uv, sample_depth);

        let z_diff = view_pos.z - sample_view_pos.z;
        let range_check = smoothstep(0.0, 1.0, radius / max(abs(z_diff), 1e-5));
        if sample_view_pos.z > sample_pos.z + bias {
            occlusion = occlusion + range_check;
        }
    }

    let ao = 1.0 - (occlusion / f32(SSAO_SAMPLES)) * intensity;
    return vec4<f32>(clamp(ao, 0.0, 1.0));
}
