// SSAO ブラー (Round 6)
// 4x4 box blur で SSAO ノイズを除去

@group(0) @binding(0) var t_ssao: texture_2d<f32>;
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(t_ssao));
    var sum = 0.0;
    var count = 0.0;
    for (var y: i32 = -2; y <= 1; y = y + 1) {
        for (var x: i32 = -2; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            sum = sum + textureSampleLevel(t_ssao, s_linear, in.uv + offset, 0.0).r;
            count = count + 1.0;
        }
    }
    let blurred = sum / count;
    return vec4<f32>(blurred, 0.0, 0.0, 1.0);
}
