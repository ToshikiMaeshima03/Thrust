// Volumetric Light Shafts (God Rays) — Round 7
//
// 太陽方向のスクリーンスペース ray-march で、深度バッファに遮蔽されていない
// 太陽光線を画面下から放射状に蓄積する。Crytek スタイルの簡易実装。

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

struct VolumetricUniform {
    /// xyz = sun world direction, w = enabled
    sun_dir: vec4<f32>,
    /// rgb = sun color, a = intensity
    sun_color: vec4<f32>,
    /// x = density, y = decay, z = weight, w = exposure
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> vol: VolumetricUniform;
@group(0) @binding(2) var t_depth: texture_depth_2d;
@group(0) @binding(3) var t_hdr: texture_2d<f32>;
@group(0) @binding(4) var s_linear: sampler;

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

fn uv_to_pixel(uv: vec2<f32>) -> vec2<i32> {
    let dim = vec2<f32>(camera.viewport.xy);
    let p = clamp(uv * dim, vec2<f32>(0.0), dim - vec2<f32>(1.0));
    return vec2<i32>(p);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if vol.sun_dir.w < 0.5 {
        return vec4<f32>(0.0);
    }

    // 太陽方向をスクリーン空間に投影
    let sun_world_pos = camera.camera_position - normalize(vol.sun_dir.xyz) * 200.0;
    let sun_clip = camera.view_proj * vec4<f32>(sun_world_pos, 1.0);
    if sun_clip.w <= 0.0 {
        return vec4<f32>(0.0);
    }
    let sun_ndc = sun_clip.xyz / sun_clip.w;
    let sun_uv = vec2<f32>(sun_ndc.x * 0.5 + 0.5, -sun_ndc.y * 0.5 + 0.5);

    // 太陽が画面外に大きく出ていればフェード
    let off_screen_dist = max(
        max(-sun_uv.x, sun_uv.x - 1.0),
        max(-sun_uv.y, sun_uv.y - 1.0),
    );
    let off_screen_fade = 1.0 - smoothstep(0.0, 0.5, off_screen_dist);

    let density = vol.params.x;
    let decay = vol.params.y;
    let weight = vol.params.z;
    let exposure = vol.params.w;

    let num_samples: i32 = 48;
    var coord = in.uv;
    let delta = (in.uv - sun_uv) / f32(num_samples) * density;

    var color = vec3<f32>(0.0);
    var illumination_decay = 1.0;
    for (var i: i32 = 0; i < num_samples; i = i + 1) {
        coord = coord - delta;
        let depth = textureLoad(t_depth, uv_to_pixel(coord), 0);
        // 深度がほぼ遠平面 (= 空) なら太陽光線が通過
        var sample = 0.0;
        if depth >= 0.999 {
            sample = 1.0;
        }
        sample = sample * illumination_decay * weight;
        color = color + vec3<f32>(sample);
        illumination_decay = illumination_decay * decay;
    }

    let sun_color = vol.sun_color.rgb * vol.sun_color.a;
    let result = color * sun_color * exposure * off_screen_fade;
    return vec4<f32>(result, 1.0);
}
