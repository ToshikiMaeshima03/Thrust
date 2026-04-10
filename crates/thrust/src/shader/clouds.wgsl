// Volumetric Clouds (Round 8)
//
// 簡易ボリュメトリッククラウド: ray-march で 3D ノイズをサンプリング、
// Henyey-Greenstein 散乱で太陽光の透過を計算する。

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

struct CloudUniform {
    /// xyz = sun direction, w = enabled
    sun_dir: vec4<f32>,
    /// rgb = sun color, a = intensity
    sun_color: vec4<f32>,
    /// x = base_height, y = top_height, z = density, w = coverage
    params: vec4<f32>,
    /// x = scale, y = wind_speed, z = scattering_g, w = step_size
    params2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> cloud: CloudUniform;
@group(0) @binding(2) var t_depth: texture_depth_2d;
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

// 簡易 3D Hash ノイズ
fn hash3(p: vec3<f32>) -> f32 {
    let pp = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(dot(pp, vec3<f32>(43.5, 78.2, 91.4))) * 43758.5);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let n000 = hash3(i + vec3<f32>(0.0, 0.0, 0.0));
    let n100 = hash3(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash3(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash3(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash3(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash3(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash3(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash3(i + vec3<f32>(1.0, 1.0, 1.0));

    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: i32 = 0; i < 4; i = i + 1) {
        v = v + amp * noise3(p * freq);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return v;
}

fn cloud_density(p: vec3<f32>) -> f32 {
    let base_h = cloud.params.x;
    let top_h = cloud.params.y;
    if p.y < base_h || p.y > top_h {
        return 0.0;
    }
    let height_grad = clamp((p.y - base_h) / (top_h - base_h), 0.0, 1.0);
    let edge = smoothstep(0.0, 0.2, height_grad) * (1.0 - smoothstep(0.8, 1.0, height_grad));

    let scale = cloud.params2.x;
    let wind = vec3<f32>(cloud.params2.y * camera.time_params.x, 0.0, 0.0);
    let n = fbm((p + wind) * scale);
    let coverage = cloud.params.w;
    let density = clamp(n - (1.0 - coverage), 0.0, 1.0) * cloud.params.z;
    return density * edge;
}

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if cloud.sun_dir.w < 0.5 {
        return vec4<f32>(0.0);
    }

    // 視線方向を再構築 (UV → world ray)
    let ndc = vec3<f32>(in.uv.x * 2.0 - 1.0, -(in.uv.y * 2.0 - 1.0), 0.5);
    let world4 = camera.inv_view_proj * vec4<f32>(ndc, 1.0);
    let world_pos = world4.xyz / max(world4.w, 1e-5);
    let ray_dir = normalize(world_pos - camera.camera_position);

    // 上向きでなければスキップ
    if ray_dir.y < 0.05 {
        return vec4<f32>(0.0);
    }

    let sun_dir = normalize(cloud.sun_dir.xyz);
    let cos_sun = dot(ray_dir, -sun_dir);
    let phase = henyey_greenstein(cos_sun, cloud.params2.z);

    let base_h = cloud.params.x;
    let top_h = cloud.params.y;

    // ray-march の開始/終了高度
    let ray_origin = camera.camera_position;
    let t_start = (base_h - ray_origin.y) / max(ray_dir.y, 1e-3);
    let t_end = (top_h - ray_origin.y) / max(ray_dir.y, 1e-3);
    if t_end < 0.0 {
        return vec4<f32>(0.0);
    }
    let t_min = max(t_start, 0.0);
    let t_max = max(t_end, 0.0);

    let num_steps: i32 = 32;
    let step = (t_max - t_min) / f32(num_steps);

    var transmittance = 1.0;
    var color = vec3<f32>(0.0);
    var t = t_min;
    for (var i: i32 = 0; i < num_steps; i = i + 1) {
        let p = ray_origin + ray_dir * t;
        let d = cloud_density(p);
        if d > 0.001 {
            // ライトに向かう短い ray-march で太陽光透過率を計算
            var light_t = 1.0;
            for (var j: i32 = 0; j < 4; j = j + 1) {
                let lp = p + (-sun_dir) * f32(j) * 50.0;
                light_t = light_t * exp(-cloud_density(lp) * 50.0);
            }
            let scatter = cloud.sun_color.rgb * cloud.sun_color.a * phase * light_t;
            color = color + scatter * d * step * transmittance;
            transmittance = transmittance * exp(-d * step);
            if transmittance < 0.01 {
                break;
            }
        }
        t = t + step;
    }

    let alpha = 1.0 - transmittance;
    return vec4<f32>(color, alpha);
}
