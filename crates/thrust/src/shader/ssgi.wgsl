// Screen-Space Global Illumination (Round 9)
//
// Lumen 風の簡易 SSGI: depth + normal + HDR をサンプリングし、
// 各ピクセルから半球方向に ray-march して 1 バウンス間接光を計算する。
// 完全な GI には voxel cone tracing や RTX が必要だが、ここでは
// スクリーン空間のみの近似で十分な視覚効果を得る。

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

struct SsgiUniform {
    /// x = max_distance, y = num_steps, z = num_rays, w = strength
    params: vec4<f32>,
    /// x = bounce_intensity, y = thickness, z = noise_scale, w = enabled
    extra: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> ssgi_params: SsgiUniform;
@group(0) @binding(2) var t_normal: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var t_hdr: texture_2d<f32>;
@group(0) @binding(5) var s_linear: sampler;

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

fn uv_to_pixel(uv: vec2<f32>) -> vec2<i32> {
    let dim = vec2<f32>(camera.viewport.xy);
    let p = clamp(uv * dim, vec2<f32>(0.0), dim - vec2<f32>(1.0));
    return vec2<i32>(p);
}

fn load_depth(uv: vec2<f32>) -> f32 {
    return textureLoad(t_depth, uv_to_pixel(uv), 0);
}

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

fn project_view_to_uv(view_pos: vec3<f32>) -> vec3<f32> {
    let clip = camera.proj * vec4<f32>(view_pos, 1.0);
    if clip.w <= 0.0 {
        return vec3<f32>(-1.0);
    }
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    return vec3<f32>(uv, ndc.z);
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5);
}

fn hemisphere_dir(i: u32, total: u32, normal: vec3<f32>, rotation: vec2<f32>) -> vec3<f32> {
    let golden = 2.399963229728653;
    let idx = f32(i);
    let phi = idx * golden;
    let cos_theta = 1.0 - (idx + 0.5) / f32(total);
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
    if ssgi_params.extra.w < 0.5 {
        return vec4<f32>(0.0);
    }

    let depth = load_depth(in.uv);
    if depth >= 0.9999 {
        return vec4<f32>(0.0);
    }

    let view_pos = reconstruct_view_position(in.uv, depth);
    let view_normal = fetch_view_normal(in.uv);

    let max_dist = ssgi_params.params.x;
    let num_steps = i32(ssgi_params.params.y);
    let num_rays = u32(ssgi_params.params.z);
    let strength = ssgi_params.params.w;
    let bounce_intensity = ssgi_params.extra.x;
    let thickness = ssgi_params.extra.y;

    var indirect = vec3<f32>(0.0);
    let noise_rot = hash21(in.uv * camera.viewport.xy + camera.time_params.x) * 6.28318;
    let rotation = vec2<f32>(cos(noise_rot), sin(noise_rot));

    for (var ri: u32 = 0u; ri < num_rays; ri = ri + 1u) {
        let ray_dir = hemisphere_dir(ri, num_rays, view_normal, rotation);
        let step_size = max_dist / f32(num_steps);
        var t = 0.05;
        var hit = false;
        var hit_uv = vec2<f32>(-1.0);

        for (var si: i32 = 0; si < num_steps; si = si + 1) {
            let sample_pos = view_pos + ray_dir * t;
            let projected = project_view_to_uv(sample_pos);
            if projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 {
                break;
            }
            let scene_depth = load_depth(projected.xy);
            let scene_view_pos = reconstruct_view_position(projected.xy, scene_depth);

            let z_diff = sample_pos.z - scene_view_pos.z;
            if z_diff < 0.0 && z_diff > -thickness {
                hit = true;
                hit_uv = projected.xy;
                break;
            }
            t = t + step_size;
        }

        if hit {
            let hit_color = textureSampleLevel(t_hdr, s_linear, hit_uv, 0.0).rgb;
            let hit_normal = fetch_view_normal(hit_uv);
            // Lambert: 自身の法線と ray 方向の cos
            let n_dot_l = max(dot(view_normal, ray_dir), 0.0);
            // ヒット面の法線とライト (反対方向) の cos で背面チェック
            let back = max(dot(hit_normal, -ray_dir), 0.0);
            indirect = indirect + hit_color * n_dot_l * back * bounce_intensity;
        }
    }

    indirect = indirect / f32(num_rays) * strength;
    return vec4<f32>(indirect, 1.0);
}
