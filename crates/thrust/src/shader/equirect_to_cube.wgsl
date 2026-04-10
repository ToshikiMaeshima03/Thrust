// Equirectangular HDR → Cubemap 変換 (Round 4 IBL)

@group(0) @binding(0) var t_equirect: texture_2d<f32>;
@group(0) @binding(1) var s_linear: sampler;

struct FaceUniform {
    /// 0..5 = +X, -X, +Y, -Y, +Z, -Z
    face: vec4<u32>,
};

@group(0) @binding(2) var<uniform> face_uniform: FaceUniform;

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

fn uv_to_dir(uv: vec2<f32>, face: u32) -> vec3<f32> {
    // [0, 1] → [-1, 1]
    let n = uv * 2.0 - 1.0;
    var d: vec3<f32>;
    switch face {
        case 0u: { d = vec3<f32>(1.0, -n.y, -n.x); }   // +X
        case 1u: { d = vec3<f32>(-1.0, -n.y, n.x); }   // -X
        case 2u: { d = vec3<f32>(n.x, 1.0, n.y); }     // +Y
        case 3u: { d = vec3<f32>(n.x, -1.0, -n.y); }   // -Y
        case 4u: { d = vec3<f32>(n.x, -n.y, 1.0); }    // +Z
        default: { d = vec3<f32>(-n.x, -n.y, -1.0); }  // -Z
    }
    return normalize(d);
}

fn dir_to_equirect(d: vec3<f32>) -> vec2<f32> {
    let phi = atan2(d.z, d.x);
    let theta = asin(clamp(d.y, -1.0, 1.0));
    return vec2<f32>(phi / (2.0 * PI) + 0.5, theta / PI + 0.5);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dir = uv_to_dir(in.uv, face_uniform.face.x);
    let equirect_uv = dir_to_equirect(dir);
    let color = textureSample(t_equirect, s_linear, equirect_uv).rgb;
    return vec4<f32>(color, 1.0);
}
