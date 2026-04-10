// Motion Blur (Round 7)
//
// G-buffer から取得したモーションベクトルを使い、現在のピクセルからベクトル方向に
// サンプルを蓄積する単純なリニアモーションブラー。

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

struct MbUniform {
    /// x = strength, y = max_pixel_offset, z = enabled, w = _
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> mb: MbUniform;
@group(0) @binding(2) var t_color: texture_2d<f32>;
@group(0) @binding(3) var t_motion: texture_2d<f32>;
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let center = textureSampleLevel(t_color, s_linear, in.uv, 0.0);
    if mb.params.z < 0.5 {
        return center;
    }
    var motion = textureSampleLevel(t_motion, s_linear, in.uv, 0.0).xy;
    motion = motion * mb.params.x;
    let max_off = mb.params.y;
    let dim = vec2<f32>(camera.viewport.xy);
    let max_off_uv = max_off / dim;
    let len = length(motion);
    if len > max_off_uv.x {
        motion = motion * (max_off_uv.x / max(len, 1e-5));
    }

    let num_samples: i32 = 8;
    var sum = vec4<f32>(0.0);
    for (var i: i32 = 0; i < num_samples; i = i + 1) {
        let t = f32(i) / f32(num_samples - 1) - 0.5;
        let off = motion * t;
        sum = sum + textureSampleLevel(t_color, s_linear, in.uv + off, 0.0);
    }
    return sum / f32(num_samples);
}
