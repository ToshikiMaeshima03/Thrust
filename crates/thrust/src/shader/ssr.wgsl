// Screen-Space Reflections (Round 7)
//
// Linear ray-marching を view 空間で行い、反射ベクトルが当たるピクセルから
// HDR カラーを取得する。Roughness によってぼかし強度を変える。
// 結果は半透明 RGBA で main HDR に加算する想定。

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

struct SsrParams {
    /// x = max_distance, y = thickness, z = max_steps, w = strength
    params: vec4<f32>,
    /// x = roughness_cutoff, y = fade_distance, z = jitter_strength, w = _
    extra: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> ssr_params: SsrParams;
@group(0) @binding(2) var t_normal: texture_2d<f32>;
@group(0) @binding(3) var t_material: texture_2d<f32>;
@group(0) @binding(4) var t_depth: texture_depth_2d;
@group(0) @binding(5) var t_hdr: texture_2d<f32>;
@group(0) @binding(6) var s_linear: sampler;

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

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let depth = load_depth(in.uv);
    if depth >= 0.9999 {
        return vec4<f32>(0.0);
    }

    let mat = textureSampleLevel(t_material, s_linear, in.uv, 0.0);
    let metallic = mat.r;
    let roughness = mat.g;

    // ラフネスが高いか非金属で反射弱いものはスキップ
    let roughness_cutoff = ssr_params.extra.x;
    if roughness > roughness_cutoff {
        return vec4<f32>(0.0);
    }

    // ビュー空間で反射ベクトル
    let view_pos = reconstruct_view_position(in.uv, depth);
    let view_normal = fetch_view_normal(in.uv);
    let view_dir = normalize(view_pos);
    let reflect_dir = normalize(reflect(view_dir, view_normal));

    // 後方反射 (カメラから離れる方向) は意味がないのでスキップ
    if reflect_dir.z > 0.0 {
        return vec4<f32>(0.0);
    }

    let max_dist = ssr_params.params.x;
    let thickness = ssr_params.params.y;
    let max_steps = i32(ssr_params.params.z);

    // ジッター: ノイズで開始オフセットを散らす
    let jitter = hash(in.uv * camera.viewport.xy + camera.time_params.x) * ssr_params.extra.z;

    var t = 0.05 + jitter * 0.05;
    let step = max_dist / f32(max_steps);
    var hit_uv = vec2<f32>(-1.0);
    var fade = 0.0;

    for (var i: i32 = 0; i < max_steps; i = i + 1) {
        let sample_view = view_pos + reflect_dir * t;
        let projected = project_view_to_uv(sample_view);
        if projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 {
            break;
        }
        let scene_depth = load_depth(projected.xy);
        let scene_view_pos = reconstruct_view_position(projected.xy, scene_depth);

        let z_diff = sample_view.z - scene_view_pos.z;
        if z_diff < 0.0 && z_diff > -thickness {
            // ヒット
            hit_uv = projected.xy;
            // 画面端でフェード
            let edge_x = min(hit_uv.x, 1.0 - hit_uv.x);
            let edge_y = min(hit_uv.y, 1.0 - hit_uv.y);
            let edge_fade = smoothstep(0.0, 0.1, min(edge_x, edge_y));
            // 距離フェード
            let dist_fade = 1.0 - smoothstep(ssr_params.extra.y * 0.5, ssr_params.extra.y, t);
            fade = edge_fade * dist_fade;
            break;
        }
        t = t + step;
    }

    if hit_uv.x < 0.0 {
        return vec4<f32>(0.0);
    }

    let reflection_color = textureSampleLevel(t_hdr, s_linear, hit_uv, 0.0).rgb;
    let fresnel = pow(1.0 - max(dot(-view_dir, view_normal), 0.0), 4.0);
    let strength = ssr_params.params.w * (metallic * 0.6 + fresnel * 0.4) * (1.0 - roughness);

    return vec4<f32>(reflection_color * strength * fade, fade);
}
