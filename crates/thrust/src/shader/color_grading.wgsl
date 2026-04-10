// Color Grading + Vignette + Chromatic Aberration (Round 7)
//
// LDR 入力に対して以下を順に適用:
// 1. Lift / Gamma / Gain (LGG color grading)
// 2. 彩度・コントラスト・露出
// 3. ヴィネット
// 4. 軽微な色収差 (chromatic aberration)

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

struct GradingUniform {
    /// rgb = lift, w = enabled
    lift: vec4<f32>,
    /// rgb = gamma, w = _
    gamma: vec4<f32>,
    /// rgb = gain, w = _
    gain: vec4<f32>,
    /// x = saturation, y = contrast, z = exposure_offset, w = vignette_strength
    misc: vec4<f32>,
    /// x = vignette_radius, y = chromatic_aberration_strength, z = _, w = _
    misc2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> grading: GradingUniform;
@group(0) @binding(2) var t_color: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

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

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var color: vec3<f32>;

    // 色収差 (chromatic aberration)
    let ca = grading.misc2.y;
    if ca > 0.001 {
        let center_offset = in.uv - vec2<f32>(0.5);
        let dist = length(center_offset);
        let dir = center_offset / max(dist, 1e-5);
        let off = dir * ca * dist * 0.01;
        let r = textureSampleLevel(t_color, s_linear, in.uv + off, 0.0).r;
        let g = textureSampleLevel(t_color, s_linear, in.uv, 0.0).g;
        let b = textureSampleLevel(t_color, s_linear, in.uv - off, 0.0).b;
        color = vec3<f32>(r, g, b);
    } else {
        color = textureSampleLevel(t_color, s_linear, in.uv, 0.0).rgb;
    }

    if grading.lift.w < 0.5 {
        // 無効: vignette のみ適用
        let center_offset = in.uv - vec2<f32>(0.5);
        let dist = length(center_offset) * 1.4142;
        let vignette = 1.0 - smoothstep(grading.misc2.x, 1.0, dist) * grading.misc.w;
        return vec4<f32>(color * vignette, 1.0);
    }

    // 露出補正
    color = color * exp2(grading.misc.z);

    // Lift Gamma Gain (Slope Offset Power)
    color = color + grading.lift.rgb;
    color = pow(max(color, vec3<f32>(0.0)), grading.gamma.rgb);
    color = color * grading.gain.rgb;

    // 彩度
    let lum = vec3<f32>(luminance(color));
    color = mix(lum, color, grading.misc.x);

    // コントラスト
    color = (color - vec3<f32>(0.5)) * grading.misc.y + vec3<f32>(0.5);
    color = max(color, vec3<f32>(0.0));

    // ヴィネット
    let center_offset2 = in.uv - vec2<f32>(0.5);
    let dist2 = length(center_offset2) * 1.4142;
    let vignette = 1.0 - smoothstep(grading.misc2.x, 1.0, dist2) * grading.misc.w;
    color = color * vignette;

    return vec4<f32>(color, 1.0);
}
