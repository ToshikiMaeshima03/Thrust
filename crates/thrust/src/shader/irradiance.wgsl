// 拡散イラディアンス cubemap 生成 (Round 4 IBL)
// 環境マップを半球積分してイラディアンスマップを生成する

@group(0) @binding(0) var t_env: texture_cube<f32>;
@group(0) @binding(1) var s_linear: sampler;

struct FaceUniform {
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let normal = uv_to_dir(in.uv, face_uniform.face.x);

    var up = vec3<f32>(0.0, 1.0, 0.0);
    if abs(normal.y) > 0.999 {
        up = vec3<f32>(0.0, 0.0, 1.0);
    }
    let right = normalize(cross(up, normal));
    let new_up = cross(normal, right);

    var irradiance = vec3<f32>(0.0);
    let sample_delta = 0.05;
    var nr_samples = 0.0;

    var phi: f32 = 0.0;
    loop {
        if phi >= 2.0 * PI { break; }
        var theta: f32 = 0.0;
        loop {
            if theta >= 0.5 * PI { break; }
            let tangent_sample = vec3<f32>(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
            let sample_vec = right * tangent_sample.x + new_up * tangent_sample.y + normal * tangent_sample.z;
            irradiance = irradiance + textureSampleLevel(t_env, s_linear, sample_vec, 0.0).rgb * cos(theta) * sin(theta);
            nr_samples = nr_samples + 1.0;
            theta = theta + sample_delta;
        }
        phi = phi + sample_delta;
    }
    irradiance = PI * irradiance / nr_samples;
    return vec4<f32>(irradiance, 1.0);
}
