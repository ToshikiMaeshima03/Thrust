// FXAA 3.11 簡易移植 (Round 4)
// NVIDIA FXAA を簡略化したエッジアンチエイリアス

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_linear: sampler;

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

const FXAA_SPAN_MAX: f32 = 8.0;
const FXAA_REDUCE_MUL: f32 = 1.0 / 8.0;
const FXAA_REDUCE_MIN: f32 = 1.0 / 128.0;

fn rgb2luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(1.0, 1.0) / vec2<f32>(textureDimensions(t_input));

    let rgb_nw = textureSample(t_input, s_linear, in.uv + vec2<f32>(-1.0, -1.0) * texel).rgb;
    let rgb_ne = textureSample(t_input, s_linear, in.uv + vec2<f32>(1.0, -1.0) * texel).rgb;
    let rgb_sw = textureSample(t_input, s_linear, in.uv + vec2<f32>(-1.0, 1.0) * texel).rgb;
    let rgb_se = textureSample(t_input, s_linear, in.uv + vec2<f32>(1.0, 1.0) * texel).rgb;
    let rgb_m = textureSample(t_input, s_linear, in.uv).rgb;

    let luma_nw = rgb2luma(rgb_nw);
    let luma_ne = rgb2luma(rgb_ne);
    let luma_sw = rgb2luma(rgb_sw);
    let luma_se = rgb2luma(rgb_se);
    let luma_m = rgb2luma(rgb_m);

    let luma_min = min(luma_m, min(min(luma_nw, luma_ne), min(luma_sw, luma_se)));
    let luma_max = max(luma_m, max(max(luma_nw, luma_ne), max(luma_sw, luma_se)));

    var dir = vec2<f32>(0.0);
    dir.x = -((luma_nw + luma_ne) - (luma_sw + luma_se));
    dir.y = ((luma_nw + luma_sw) - (luma_ne + luma_se));

    let dir_reduce = max((luma_nw + luma_ne + luma_sw + luma_se) * (0.25 * FXAA_REDUCE_MUL), FXAA_REDUCE_MIN);
    let rcp_dir_min = 1.0 / (min(abs(dir.x), abs(dir.y)) + dir_reduce);
    dir = clamp(dir * rcp_dir_min, vec2<f32>(-FXAA_SPAN_MAX), vec2<f32>(FXAA_SPAN_MAX)) * texel;

    let rgb_a = 0.5 * (
        textureSample(t_input, s_linear, in.uv + dir * (1.0 / 3.0 - 0.5)).rgb +
        textureSample(t_input, s_linear, in.uv + dir * (2.0 / 3.0 - 0.5)).rgb
    );
    let rgb_b = rgb_a * 0.5 + 0.25 * (
        textureSample(t_input, s_linear, in.uv + dir * -0.5).rgb +
        textureSample(t_input, s_linear, in.uv + dir * 0.5).rgb
    );

    let luma_b = rgb2luma(rgb_b);
    if luma_b < luma_min || luma_b > luma_max {
        return vec4<f32>(rgb_a, 1.0);
    }
    return vec4<f32>(rgb_b, 1.0);
}
