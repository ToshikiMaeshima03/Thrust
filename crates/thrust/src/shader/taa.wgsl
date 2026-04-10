// Temporal Anti-Aliasing (Round 8)
//
// 前フレームの色を motion vector で再投影、neighborhood clamp で artifact を抑える。
// FXAA より高品質、特に時間サブピクセルジッタリングと組み合わせると効果大。

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

struct TaaUniform {
    /// x = blend_factor, y = clamp_strength, z = enabled, w = _
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> taa: TaaUniform;
@group(0) @binding(2) var t_current: texture_2d<f32>;
@group(0) @binding(3) var t_history: texture_2d<f32>;
@group(0) @binding(4) var t_motion: texture_2d<f32>;
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

/// 3x3 近傍からの最小/最大色を取得
fn neighborhood_min_max(uv: vec2<f32>) -> array<vec3<f32>, 2> {
    let texel = camera.viewport.zw;
    var mn = vec3<f32>(1e10);
    var mx = vec3<f32>(-1e10);
    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let off = vec2<f32>(f32(x), f32(y)) * texel;
            let c = textureSampleLevel(t_current, s_linear, uv + off, 0.0).rgb;
            mn = min(mn, c);
            mx = max(mx, c);
        }
    }
    return array<vec3<f32>, 2>(mn, mx);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let current = textureSampleLevel(t_current, s_linear, in.uv, 0.0).rgb;
    if taa.params.z < 0.5 {
        return vec4<f32>(current, 1.0);
    }

    let motion = textureSampleLevel(t_motion, s_linear, in.uv, 0.0).xy;
    let prev_uv = in.uv - motion;

    // 画面外なら history を使わず current だけ
    if prev_uv.x < 0.0 || prev_uv.x > 1.0 || prev_uv.y < 0.0 || prev_uv.y > 1.0 {
        return vec4<f32>(current, 1.0);
    }

    let history = textureSampleLevel(t_history, s_linear, prev_uv, 0.0).rgb;

    // Neighborhood clamp で history の値を current の近傍範囲にクリップ
    let min_max = neighborhood_min_max(in.uv);
    let mn = min_max[0];
    let mx = min_max[1];
    let clamp_strength = taa.params.y;
    let mn_expanded = mn - (mx - mn) * (1.0 - clamp_strength);
    let mx_expanded = mx + (mx - mn) * (1.0 - clamp_strength);
    let clamped_history = clamp(history, mn_expanded, mx_expanded);

    // ブレンド
    let blend = taa.params.x;
    let result = mix(current, clamped_history, blend);
    return vec4<f32>(result, 1.0);
}
