// Skybox シェーダー (Round 4)
// 全画面三角形 + ビュー方向から cubemap サンプル + プロシージャル sky フォールバック

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

struct SkyboxUniform {
    /// xyz = 太陽方向, w = enabled (0 = procedural sky, 1 = cubemap)
    sun_dir: vec4<f32>,
    /// rgb = 地平線色, a = HDR 強度
    horizon: vec4<f32>,
    /// rgb = 天頂色, a = _
    zenith: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> sky: SkyboxUniform;
@group(0) @binding(2) var t_cubemap: texture_cube<f32>;
@group(0) @binding(3) var s_cubemap: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_dir: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    // 全画面三角形
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    let clip_pos = vec4<f32>(x, y, 1.0, 1.0);
    out.clip_position = clip_pos;

    // クリップ → ワールド方向 (view-projection の逆行列を介する)
    let inv_vp = transpose(mat4x4<f32>(
        camera.view_proj[0],
        camera.view_proj[1],
        camera.view_proj[2],
        camera.view_proj[3],
    ));
    // 簡易: ndc を view 空間方向にして、view 行列の inverse rotation を適用
    let view_inv = transpose(mat4x4<f32>(
        camera.view[0],
        camera.view[1],
        camera.view[2],
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    ));
    // クリップ空間の x,y を view 空間に変換 (z = 1 = 遠平面)
    let view_dir = normalize(vec3<f32>(x, y, -1.0));
    let world_dir = (view_inv * vec4<f32>(view_dir, 0.0)).xyz;
    out.world_dir = normalize(world_dir);
    return out;
}

// ===== Round 8: Atmospheric Scattering (Hillaire 簡易版) =====

const PI: f32 = 3.14159265358979;
const RAYLEIGH_BETA: vec3<f32> = vec3<f32>(5.8e-3, 13.5e-3, 33.1e-3); // wavelength dependent
const MIE_BETA: f32 = 21e-3;

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return (3.0 / (16.0 * PI)) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5);
    return (3.0 / (8.0 * PI)) * ((1.0 - g2) * (1.0 + cos_theta * cos_theta)) / ((2.0 + g2) * denom);
}

/// 簡易大気散乱: 視線方向と太陽方向から空色を計算する
/// dir: ビュー方向 (正規化)、sun: 太陽方向 (正規化)
fn atmospheric_sky(dir: vec3<f32>, sun: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(sun);
    let cos_theta = dot(dir, -sun_dir);

    // 高度に応じた密度減衰 (上を見ると薄くなる)
    let h = clamp(dir.y, -0.1, 1.0);
    let density = exp(-h * 3.0);

    // Rayleigh: 青空
    let rayleigh = rayleigh_phase(cos_theta) * RAYLEIGH_BETA * density * 30.0;

    // Mie: 太陽周りのハロ
    let mie = mie_phase(cos_theta, 0.76) * vec3<f32>(MIE_BETA) * density * 8.0;

    // 太陽の高さによる時刻シミュレーション
    let sun_height = max(-sun_dir.y, 0.0);
    let day_factor = clamp(sun_height * 4.0, 0.0, 1.0);
    let dawn_color = vec3<f32>(0.95, 0.45, 0.15);
    let day_color = vec3<f32>(0.5, 0.7, 1.0);
    let sky_tint = mix(dawn_color, day_color, day_factor);

    return (rayleigh + mie) * sky_tint;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_dir);

    if sky.sun_dir.w > 0.5 {
        // Cubemap モード
        let color = textureSample(t_cubemap, s_cubemap, dir).rgb * sky.horizon.a;
        return vec4<f32>(color, 1.0);
    }

    // Round 8: 大気散乱ベースの空
    let sun_dir = normalize(sky.sun_dir.xyz);
    var color = atmospheric_sky(dir, sun_dir);

    // 既存の地平線/天頂グラデーションをブレンド (アーティスト調整用)
    let t = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    let gradient = mix(sky.horizon.rgb, sky.zenith.rgb, smoothstep(0.0, 1.0, t));
    color = color * 0.7 + gradient * 0.3;

    // 太陽ディスク
    let sun_dot = max(dot(dir, -sun_dir), 0.0);
    let sun_disk = pow(sun_dot, 512.0);
    color = color + vec3<f32>(1.0, 0.95, 0.8) * sun_disk * 8.0;

    return vec4<f32>(color * sky.horizon.a, 1.0);
}
