// Bloom シェーダー (Round 4)
// Threshold extract + downsample (13-tap) + upsample (3x3 tent) チェーン

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_linear: sampler;

struct BloomUniform {
    /// x = threshold, y = soft_knee, z = filter_radius, w = _
    params: vec4<f32>,
};

@group(0) @binding(2) var<uniform> bloom: BloomUniform;

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

fn karis_average(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + max(max(c.r, c.g), c.b));
}

/// 閾値抽出 (前段)
@fragment
fn fs_threshold(in: VsOut) -> @location(0) vec4<f32> {
    let color = textureSample(t_input, s_linear, in.uv).rgb;
    let brightness = max(max(color.r, color.g), color.b);
    let threshold = bloom.params.x;
    let knee = bloom.params.y * threshold + 1e-5;

    var soft = brightness - threshold + knee;
    soft = clamp(soft, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee + 1e-5);
    let contribution = max(soft, brightness - threshold) / max(brightness, 1e-5);

    return vec4<f32>(color * contribution, 1.0);
}

/// 13 tap downsample (Call of Duty Advanced Warfare 風)
@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(1.0, 1.0) / vec2<f32>(textureDimensions(t_input));
    let x = texel.x;
    let y = texel.y;

    let a = textureSample(t_input, s_linear, in.uv + vec2<f32>(-2.0 * x, 2.0 * y)).rgb;
    let b = textureSample(t_input, s_linear, in.uv + vec2<f32>(0.0, 2.0 * y)).rgb;
    let c = textureSample(t_input, s_linear, in.uv + vec2<f32>(2.0 * x, 2.0 * y)).rgb;
    let d = textureSample(t_input, s_linear, in.uv + vec2<f32>(-2.0 * x, 0.0)).rgb;
    let e = textureSample(t_input, s_linear, in.uv).rgb;
    let f = textureSample(t_input, s_linear, in.uv + vec2<f32>(2.0 * x, 0.0)).rgb;
    let g = textureSample(t_input, s_linear, in.uv + vec2<f32>(-2.0 * x, -2.0 * y)).rgb;
    let h = textureSample(t_input, s_linear, in.uv + vec2<f32>(0.0, -2.0 * y)).rgb;
    let i = textureSample(t_input, s_linear, in.uv + vec2<f32>(2.0 * x, -2.0 * y)).rgb;
    let j = textureSample(t_input, s_linear, in.uv + vec2<f32>(-x, y)).rgb;
    let k = textureSample(t_input, s_linear, in.uv + vec2<f32>(x, y)).rgb;
    let l = textureSample(t_input, s_linear, in.uv + vec2<f32>(-x, -y)).rgb;
    let m = textureSample(t_input, s_linear, in.uv + vec2<f32>(x, -y)).rgb;

    var result = e * 0.125;
    result = result + (a + c + g + i) * 0.03125;
    result = result + (b + d + f + h) * 0.0625;
    result = result + (j + k + l + m) * 0.125;

    return vec4<f32>(result, 1.0);
}

/// 3x3 tent upsample
@fragment
fn fs_upsample(in: VsOut) -> @location(0) vec4<f32> {
    let radius = bloom.params.z;
    let texel = vec2<f32>(1.0, 1.0) / vec2<f32>(textureDimensions(t_input));
    let x = texel.x * radius;
    let y = texel.y * radius;

    let a = textureSample(t_input, s_linear, in.uv + vec2<f32>(-x, y)).rgb;
    let b = textureSample(t_input, s_linear, in.uv + vec2<f32>(0.0, y)).rgb;
    let c = textureSample(t_input, s_linear, in.uv + vec2<f32>(x, y)).rgb;
    let d = textureSample(t_input, s_linear, in.uv + vec2<f32>(-x, 0.0)).rgb;
    let e = textureSample(t_input, s_linear, in.uv).rgb;
    let f = textureSample(t_input, s_linear, in.uv + vec2<f32>(x, 0.0)).rgb;
    let g = textureSample(t_input, s_linear, in.uv + vec2<f32>(-x, -y)).rgb;
    let h = textureSample(t_input, s_linear, in.uv + vec2<f32>(0.0, -y)).rgb;
    let i = textureSample(t_input, s_linear, in.uv + vec2<f32>(x, -y)).rgb;

    var result = e * 4.0;
    result = result + (b + d + f + h) * 2.0;
    result = result + (a + c + g + i);
    return vec4<f32>(result / 16.0, 1.0);
}
