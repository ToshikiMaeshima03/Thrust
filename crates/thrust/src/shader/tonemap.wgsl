// HDR → LDR トーンマップ + ガンマ補正 (Round 4)
// ACES Filmic (Narkowicz 近似) + Bloom 加算

@group(0) @binding(0) var t_hdr: texture_2d<f32>;
@group(0) @binding(1) var s_linear: sampler;
@group(0) @binding(2) var t_bloom: texture_2d<f32>;

struct PostUniform {
    /// x = exposure, y = bloom_strength, z = enable_bloom (0/1), w = _
    params: vec4<f32>,
};

@group(0) @binding(3) var<uniform> post: PostUniform;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// 全画面三角形 (頂点 3 つ)
@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

// ACES Filmic (Narkowicz 2015 近似)
fn aces_tonemap(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var hdr_color = textureSample(t_hdr, s_linear, in.uv).rgb;

    // Bloom 加算
    if post.params.z > 0.5 {
        let bloom = textureSample(t_bloom, s_linear, in.uv).rgb;
        hdr_color = hdr_color + bloom * post.params.y;
    }

    // 露光
    hdr_color = hdr_color * post.params.x;

    // ACES Filmic
    var ldr = aces_tonemap(hdr_color);

    // sRGB ガンマ補正
    ldr = pow(ldr, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(ldr, 1.0);
}
