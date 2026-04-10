// Deferred Decal Shader (Round 7)
//
// 各デカールはボックスボリューム (1m³) として描画され、フラグメントごとに
// 深度バッファを再構築してデカール空間に変換、テクスチャを投影する。

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

struct DecalUniform {
    /// world → decal local の変換 (inverse model)
    inv_model: mat4x4<f32>,
    /// world での model 変換 (頂点位置計算用)
    model: mat4x4<f32>,
    /// xyz = decal color tint, w = opacity
    tint: vec4<f32>,
    /// x = max_normal_angle (cos), y = fade_distance, z = _, w = _
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> decal: DecalUniform;
@group(1) @binding(1) var t_decal: texture_2d<f32>;
@group(1) @binding(2) var s_decal: sampler;
@group(1) @binding(3) var t_normal: texture_2d<f32>;
@group(1) @binding(4) var t_depth: texture_depth_2d;

struct VertexInput {
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) screen_uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = decal.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    let ndc = out.clip_position.xy / max(out.clip_position.w, 1e-5);
    out.screen_uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
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

fn fetch_view_normal(uv: vec2<f32>) -> vec3<f32> {
    let raw = textureLoad(t_normal, uv_to_pixel(uv), 0).xyz;
    return normalize(raw * 2.0 - 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let depth = load_depth(in.screen_uv);
    if depth >= 0.9999 {
        discard;
    }

    // 深度から world 空間位置を再構築
    let ndc = vec3<f32>(
        in.screen_uv.x * 2.0 - 1.0,
        -(in.screen_uv.y * 2.0 - 1.0),
        depth,
    );
    let world4 = camera.inv_view_proj * vec4<f32>(ndc, 1.0);
    let world_pos = world4.xyz / max(world4.w, 1e-5);

    // decal local 空間に変換 (-0.5..0.5 範囲)
    let local = decal.inv_model * vec4<f32>(world_pos, 1.0);
    let l = local.xyz;
    if abs(l.x) > 0.5 || abs(l.y) > 0.5 || abs(l.z) > 0.5 {
        discard;
    }

    // ジオメトリ法線とデカール投影方向の角度チェック
    let view_normal = fetch_view_normal(in.screen_uv);
    let view_world_inv_t = mat3x3<f32>(
        camera.inv_view[0].xyz,
        camera.inv_view[1].xyz,
        camera.inv_view[2].xyz,
    );
    let world_normal = normalize(view_world_inv_t * view_normal);
    let decal_dir_world = normalize((decal.model * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz);
    let cos_angle = dot(world_normal, decal_dir_world);
    let max_cos = decal.params.x;
    if cos_angle < max_cos {
        discard;
    }

    let uv = vec2<f32>(l.x + 0.5, 0.5 - l.y);
    let tex = textureSample(t_decal, s_decal, uv);
    if tex.a < 0.01 {
        discard;
    }

    // ボリュームエッジでフェード
    let edge_fade_x = smoothstep(0.5, 0.4, abs(l.x));
    let edge_fade_y = smoothstep(0.5, 0.4, abs(l.y));
    let edge_fade_z = smoothstep(0.5, 0.4, abs(l.z));
    let fade = edge_fade_x * edge_fade_y * edge_fade_z;

    let color = tex.rgb * decal.tint.rgb;
    let alpha = tex.a * decal.tint.a * fade;
    return vec4<f32>(color * alpha, alpha);
}
