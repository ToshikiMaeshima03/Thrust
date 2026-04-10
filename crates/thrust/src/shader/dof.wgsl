// Depth of Field (Round 7)
//
// 深度バッファから circle of confusion (CoC) を計算し、
// 6 タップのリングサンプリングでぼかしを生成する。シンプルなボケ近似。

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

struct DofUniform {
    /// x = focus_distance, y = focus_range, z = max_blur_radius_px, w = enabled
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> dof: DofUniform;
@group(0) @binding(2) var t_color: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
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

fn linearize_depth(d: f32) -> f32 {
    let near = camera.camera_params.x;
    let far = camera.camera_params.y;
    return near * far / max(far - d * (far - near), 1e-5);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let center_color = textureSampleLevel(t_color, s_linear, in.uv, 0.0);
    if dof.params.w < 0.5 {
        return center_color;
    }

    let dim = vec2<f32>(camera.viewport.xy);
    let pixel = clamp(in.uv * dim, vec2<f32>(0.0), dim - vec2<f32>(1.0));
    let depth_raw = textureLoad(t_depth, vec2<i32>(pixel), 0);
    let lin_depth = linearize_depth(depth_raw);
    let focus_dist = dof.params.x;
    let focus_range = max(dof.params.y, 1e-3);
    let coc = clamp(abs(lin_depth - focus_dist) / focus_range, 0.0, 1.0);
    let max_radius_px = dof.params.z;
    let radius = coc * max_radius_px;
    let texel = camera.viewport.zw;

    if radius < 0.5 {
        return center_color;
    }

    // 6 タップ + 中心 + 12 タップの 2 リングサンプリング
    let taps = array<vec2<f32>, 18>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.5, 0.0),
        vec2<f32>(0.25, 0.433),
        vec2<f32>(-0.25, 0.433),
        vec2<f32>(-0.5, 0.0),
        vec2<f32>(-0.25, -0.433),
        vec2<f32>(0.25, -0.433),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.866, 0.5),
        vec2<f32>(0.5, 0.866),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(-0.5, 0.866),
        vec2<f32>(-0.866, 0.5),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(-0.866, -0.5),
        vec2<f32>(-0.5, -0.866),
        vec2<f32>(0.5, -0.866),
        vec2<f32>(0.866, -0.5),
    );

    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i: i32 = 0; i < 18; i = i + 1) {
        let off = taps[i] * radius * texel;
        let s = textureSampleLevel(t_color, s_linear, in.uv + off, 0.0);
        sum = sum + s;
        weight_sum = weight_sum + 1.0;
    }
    return sum / weight_sum;
}
