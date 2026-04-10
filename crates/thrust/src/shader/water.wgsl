// Water Material Shader (Round 8)
//
// 頂点シェーダーで Gerstner 波を加算して水面を変形、
// フラグメントシェーダーで深度ベース屈折 + フレネル + 反射を合成。

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

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct WaterUniform {
    /// xyz = shallow color, w = transparency
    shallow_color: vec4<f32>,
    /// xyz = deep color, w = depth_range
    deep_color: vec4<f32>,
    /// x = wave_amplitude, y = wave_frequency, z = wave_speed, w = num_waves
    wave_params: vec4<f32>,
    /// xy = wind direction (normalized), z = fresnel_power, w = reflectivity
    extra: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model_data: ModelUniform;
@group(2) @binding(0) var<uniform> water: WaterUniform;
@group(2) @binding(1) var t_normal: texture_2d<f32>;
@group(2) @binding(2) var s_water: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

/// Gerstner 波 1 つを追加する (位置オフセット + 法線変更)
fn gerstner_wave(pos: vec2<f32>, dir: vec2<f32>, amp: f32, freq: f32, speed: f32, time: f32) -> vec3<f32> {
    let phase = freq * dot(dir, pos) + speed * time;
    let qx = dir.x * amp * cos(phase);
    let qz = dir.y * amp * cos(phase);
    let qy = amp * sin(phase);
    return vec3<f32>(qx, qy, qz);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world4 = model_data.model * vec4<f32>(in.position, 1.0);
    var world = world4.xyz;

    // 4 方向の Gerstner 波を加算
    let time = camera.time_params.x;
    let amp = water.wave_params.x;
    let freq = water.wave_params.y;
    let speed = water.wave_params.z;

    let wind = water.extra.xy;
    let d1 = wind;
    let d2 = vec2<f32>(wind.y, -wind.x);
    let d3 = normalize(wind + vec2<f32>(0.5, 0.7));
    let d4 = normalize(wind - vec2<f32>(0.7, 0.3));

    let w1 = gerstner_wave(world.xz, d1, amp, freq, speed, time);
    let w2 = gerstner_wave(world.xz, d2, amp * 0.6, freq * 1.7, speed * 1.2, time);
    let w3 = gerstner_wave(world.xz, d3, amp * 0.4, freq * 2.3, speed * 0.8, time);
    let w4 = gerstner_wave(world.xz, d4, amp * 0.3, freq * 3.1, speed * 1.5, time);

    world = world + w1 + w2 + w3 + w4;
    out.world_pos = world;
    out.clip_position = camera.view_proj * vec4<f32>(world, 1.0);

    // 法線は近似的に頂点シェーダーで再計算
    let normal_perturb = (w1 + w2 + w3 + w4) * 0.5;
    out.world_normal = normalize(vec3<f32>(-normal_perturb.x, 1.0, -normal_perturb.z));
    out.tex_coords = in.tex_coords + vec2<f32>(time * 0.05, time * 0.03);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n_tex = textureSample(t_normal, s_water, in.tex_coords).xyz * 2.0 - 1.0;
    let normal = normalize(in.world_normal + n_tex * 0.3);

    let view_dir = normalize(camera.camera_position - in.world_pos);
    let n_dot_v = max(dot(normal, view_dir), 0.0);

    // フレネル
    let fresnel_pow = water.extra.z;
    let fresnel = pow(1.0 - n_dot_v, fresnel_pow);

    // 浅瀬と深瀬のカラーブレンド (高さベース)
    let depth_factor = clamp(in.world_pos.y * 0.1 + 0.5, 0.0, 1.0);
    let water_color = mix(water.deep_color.rgb, water.shallow_color.rgb, depth_factor);

    // 簡易反射 (空色)
    let sky_color = vec3<f32>(0.5, 0.7, 0.95);
    let reflectivity = water.extra.w;

    let surface = mix(water_color, sky_color, fresnel * reflectivity);
    let alpha = mix(water.shallow_color.w, 1.0, fresnel);
    return vec4<f32>(surface, alpha);
}
