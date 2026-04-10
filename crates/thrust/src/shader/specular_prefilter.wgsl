// 鏡面プリフィルタ cubemap 生成 (Round 4 IBL)
// 各 mip レベルで roughness に応じた GGX 重要度サンプリング

@group(0) @binding(0) var t_env: texture_cube<f32>;
@group(0) @binding(1) var s_linear: sampler;

struct PrefilterUniform {
    /// x = face, y = roughness, z = _, w = _
    params: vec4<f32>,
};

@group(0) @binding(2) var<uniform> prefilter: PrefilterUniform;

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

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 256u;

fn uv_to_dir(uv: vec2<f32>, face: u32) -> vec3<f32> {
    let n = uv * 2.0 - 1.0;
    var d: vec3<f32>;
    switch face {
        case 0u: { d = vec3<f32>(1.0, -n.y, -n.x); }
        case 1u: { d = vec3<f32>(-1.0, -n.y, n.x); }
        case 2u: { d = vec3<f32>(n.x, 1.0, n.y); }
        case 3u: { d = vec3<f32>(n.x, -1.0, -n.y); }
        case 4u: { d = vec3<f32>(n.x, -n.y, 1.0); }
        default: { d = vec3<f32>(-n.x, -n.y, -1.0); }
    }
    return normalize(d);
}

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, n: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(n), radical_inverse_vdc(i));
}

fn importance_sample_ggx(xi: vec2<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);

    let h = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);

    var up: vec3<f32>;
    if abs(n.z) < 0.999 {
        up = vec3<f32>(0.0, 0.0, 1.0);
    } else {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return normalize(tangent * h.x + bitangent * h.y + n * h.z);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let normal = uv_to_dir(in.uv, u32(prefilter.params.x));
    let roughness = prefilter.params.y;
    let n = normal;
    let r = n;
    let v = r;

    var prefiltered_color = vec3<f32>(0.0);
    var total_weight = 0.0;

    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let xi = hammersley(i, SAMPLE_COUNT);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = normalize(2.0 * dot(v, h) * h - v);

        let n_dot_l = max(dot(n, l), 0.0);
        if n_dot_l > 0.0 {
            prefiltered_color = prefiltered_color + textureSampleLevel(t_env, s_linear, l, 0.0).rgb * n_dot_l;
            total_weight = total_weight + n_dot_l;
        }
    }
    prefiltered_color = prefiltered_color / max(total_weight, 0.001);
    return vec4<f32>(prefiltered_color, 1.0);
}
