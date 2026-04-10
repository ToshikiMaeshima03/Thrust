// Thrust PBR シェーダー (Cook-Torrance BRDF + CSM + ボリュメトリックフォグ)
// Round 5: 単一カスケードシャドウ → 3 カスケード CSM、ボリュメトリックフォグ統合

// ===== ユニフォーム / ストレージ =====

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
    viewport: vec4<f32>,        // xy = size, zw = 1/size
    camera_params: vec4<f32>,   // near, far, aspect, fov_y_rad
    time_params: vec4<f32>,     // time, dt, frame, _
};

struct LightsHeader {
    ambient: vec4<f32>,        // rgb + intensity
    counts: vec4<u32>,         // dir, point, spot, total
};

struct GpuLight {
    position_or_dir: vec4<f32>,  // xyz + type tag in w
    color_intensity: vec4<f32>,  // rgb + intensity
    params: vec4<f32>,           // range, inner_cos, outer_cos, _
    spot_dir: vec4<f32>,         // xyz spot direction
};

struct CsmUniform {
    matrices: array<mat4x4<f32>, 3>,
    splits: vec4<f32>,    // x,y,z = カスケード境界 (view-space far), w = enabled
};

/// ボリュメトリックフォグ uniform (Round 5)
struct FogUniform {
    /// xyz = フォグ色, w = 密度 (0 で無効)
    color_density: vec4<f32>,
    /// x = 高さ減衰係数, y = 基準高さ, z = 散乱強度, w = max distance
    params: vec4<f32>,
};

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct MaterialUniform {
    base_color_factor: vec4<f32>,
    mr_no: vec4<f32>,            // metallic, roughness, normal_scale, occlusion_strength
    emissive: vec4<f32>,
    texture_flags: vec4<u32>,    // bitflags: 0=base, 1=mr, 2=normal, 3=ao, 4=emissive
    // Round 8: 拡張フィールド
    extended: vec4<f32>,         // clearcoat, clearcoat_roughness, anisotropy, subsurface
    aniso_dir: vec4<f32>,        // xy = anisotropy direction (tangent space)
    subsurface_color: vec4<f32>, // rgb = sss color
    _padding2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> lights_header: LightsHeader;
@group(0) @binding(2) var<storage, read> lights: array<GpuLight>;
@group(0) @binding(3) var t_shadow: texture_depth_2d_array;
@group(0) @binding(4) var s_shadow: sampler_comparison;
@group(0) @binding(5) var<uniform> csm: CsmUniform;
@group(0) @binding(6) var t_irradiance: texture_cube<f32>;
@group(0) @binding(7) var t_prefilter: texture_cube<f32>;
@group(0) @binding(8) var t_brdf_lut: texture_2d<f32>;
@group(0) @binding(9) var s_ibl: sampler;
@group(0) @binding(10) var<uniform> fog: FogUniform;

@group(1) @binding(0) var<uniform> model_data: ModelUniform;
@group(1) @binding(1) var<storage, read> joint_matrices: array<mat4x4<f32>>;

@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var t_base_color: texture_2d<f32>;
@group(2) @binding(2) var s_base_color: sampler;
@group(2) @binding(3) var t_metallic_roughness: texture_2d<f32>;
@group(2) @binding(4) var t_normal: texture_2d<f32>;
@group(2) @binding(5) var t_occlusion: texture_2d<f32>;
@group(2) @binding(6) var t_emissive: texture_2d<f32>;

// ===== Round 7: 点光源/スポット用シャドウアトラス =====

struct PointShadowVp {
    face_vp: array<mat4x4<f32>, 6>,
    world_pos: vec4<f32>,
    far_active: vec4<f32>,
};

struct ShadowAtlasUniform {
    point_shadows: array<PointShadowVp, 4>,
    spot_vp: array<mat4x4<f32>, 4>,
    spot_pos: array<vec4<f32>, 4>,
    counts: vec4<u32>,
};

@group(3) @binding(0) var t_point_shadow: texture_depth_cube_array;
@group(3) @binding(1) var t_spot_shadow: texture_depth_2d_array;
@group(3) @binding(2) var s_atlas: sampler_comparison;
@group(3) @binding(3) var<uniform> atlas: ShadowAtlasUniform;

// ===== 頂点シェーダー =====

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
    @location(0) world_normal: vec3<f32>,
    @location(1) world_tangent: vec3<f32>,
    @location(2) world_bitangent: vec3<f32>,
    @location(3) world_position: vec3<f32>,
    @location(4) tex_coords: vec2<f32>,
    @location(5) view_space_position: vec3<f32>,
};

/// スキニング行列を計算する
/// weights.x + .y + .z + .w > 0 ならスキンメッシュ、それ以外は単位行列を返す
fn compute_skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let total = weights.x + weights.y + weights.z + weights.w;
    if total < 0.001 {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }
    let m0 = joint_matrices[joints.x] * weights.x;
    let m1 = joint_matrices[joints.y] * weights.y;
    let m2 = joint_matrices[joints.z] * weights.z;
    let m3 = joint_matrices[joints.w] * weights.w;
    return m0 + m1 + m2 + m3;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let skin_mat = compute_skin_matrix(in.joints, in.weights);
    let skinned_pos = (skin_mat * vec4<f32>(in.position, 1.0)).xyz;
    let skinned_normal = (skin_mat * vec4<f32>(in.normal, 0.0)).xyz;
    let skinned_tangent = (skin_mat * vec4<f32>(in.tangent.xyz, 0.0)).xyz;

    let world_pos = model_data.model * vec4<f32>(skinned_pos, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;
    out.world_normal = normalize((model_data.normal_matrix * vec4<f32>(skinned_normal, 0.0)).xyz);
    let t_world = normalize((model_data.model * vec4<f32>(skinned_tangent, 0.0)).xyz);
    out.world_tangent = t_world;
    out.world_bitangent = cross(out.world_normal, t_world) * in.tangent.w;
    out.tex_coords = in.tex_coords;
    // CSM 用にビュー空間 z を渡す
    out.view_space_position = (camera.view * world_pos).xyz;
    return out;
}

// ===== PBR ヘルパー関数 =====

const PI: f32 = 3.14159265359;

fn distribution_ggx(n: vec3<f32>, h: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let n_dot_h = max(dot(n, h), 0.0);
    let n_dot_h2 = n_dot_h * n_dot_h;
    let denom = n_dot_h2 * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 1e-5);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k + 1e-5);
}

fn geometry_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32) -> f32 {
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_l = max(dot(n, l), 0.0);
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

/// 1 灯ぶんの BRDF を評価する
fn brdf_one_light(
    n: vec3<f32>,
    v: vec3<f32>,
    l: vec3<f32>,
    radiance: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
    f0: vec3<f32>,
) -> vec3<f32> {
    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    if n_dot_l <= 0.0 {
        return vec3<f32>(0.0);
    }

    let ndf = distribution_ggx(n, h, roughness);
    let g = geometry_smith(n, v, l, roughness);
    let f = fresnel_schlick(max(dot(h, v), 0.0), f0);

    let numerator = ndf * g * f;
    let denominator = 4.0 * max(dot(n, v), 0.0) * n_dot_l + 1e-5;
    let specular = numerator / denominator;

    let ks = f;
    let kd = (vec3<f32>(1.0) - ks) * (1.0 - metallic);

    return (kd * base_color / PI + specular) * radiance * n_dot_l;
}

/// 距離減衰 (range ベース)
fn attenuation(distance: f32, range: f32) -> f32 {
    if range <= 0.0 {
        // 無限遠 (directional 等)
        return 1.0;
    }
    let d = distance / range;
    let factor = clamp(1.0 - d * d * d * d, 0.0, 1.0);
    return factor * factor / (distance * distance + 1e-3);
}

/// CSM カスケードを view-space 深度から選択する
fn select_cascade(view_z: f32) -> i32 {
    let abs_z = abs(view_z);
    if abs_z < csm.splits.x {
        return 0;
    }
    if abs_z < csm.splits.y {
        return 1;
    }
    return 2;
}

/// 点光源 cube シャドウをサンプルする (light_idx は atlas 内の点光源インデックス)
fn sample_point_shadow(world_position: vec3<f32>, light_idx: i32) -> f32 {
    if light_idx < 0 || light_idx >= 4 {
        return 1.0;
    }
    let shadow = atlas.point_shadows[light_idx];
    if shadow.far_active.y < 0.5 {
        return 1.0;
    }
    let light_pos = shadow.world_pos.xyz;
    let to_frag = world_position - light_pos;
    let dist = length(to_frag);
    let far = shadow.far_active.x;
    if dist > far {
        return 1.0;
    }
    // 距離を [0,1] に正規化して depth ref として使う
    let depth_ref = clamp(dist / far, 0.0, 1.0) - 0.005;
    let dir = normalize(to_frag);
    return textureSampleCompareLevel(t_point_shadow, s_atlas, dir, light_idx, depth_ref);
}

/// スポットライトシャドウをサンプルする
fn sample_spot_shadow(world_position: vec3<f32>, light_idx: i32) -> f32 {
    if light_idx < 0 || light_idx >= 4 {
        return 1.0;
    }
    if atlas.spot_pos[light_idx].w < 0.5 {
        return 1.0;
    }
    let light_space = atlas.spot_vp[light_idx] * vec4<f32>(world_position, 1.0);
    let proj = light_space.xyz / max(light_space.w, 1e-5);
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    if proj.z > 1.0 || proj.z < 0.0 {
        return 1.0;
    }
    let depth_ref = proj.z - 0.002;
    return textureSampleCompareLevel(t_spot_shadow, s_atlas, uv, light_idx, depth_ref);
}

/// CSM テクスチャ配列から 3x3 PCF でサンプルする
fn sample_shadow_csm(world_position: vec3<f32>, view_z: f32) -> f32 {
    if csm.splits.w < 0.5 {
        return 1.0;
    }
    let cascade = select_cascade(view_z);
    let light_space_pos = csm.matrices[cascade] * vec4<f32>(world_position, 1.0);
    let proj = light_space_pos.xyz / max(light_space_pos.w, 1e-5);
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    if proj.z > 1.0 || proj.z < 0.0 {
        return 1.0;
    }
    let depth_ref = proj.z - 0.002;
    let texel = 1.0 / 2048.0;
    var sum = 0.0;
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(-texel, -texel), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(0.0, -texel), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(texel, -texel), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(-texel, 0.0), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv, cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(texel, 0.0), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(-texel, texel), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(0.0, texel), cascade, depth_ref);
    sum = sum + textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2<f32>(texel, texel), cascade, depth_ref);
    return sum / 9.0;
}

/// ボリュメトリックフォグを適用する (Round 5)
///
/// 指数高さフォグ: density(h) = density_0 * exp(-falloff * (h - h_ref))
/// 散乱項: 太陽方向への散乱
fn apply_fog(color: vec3<f32>, world_position: vec3<f32>, view_dir_to_camera: vec3<f32>) -> vec3<f32> {
    let density = fog.color_density.w;
    if density <= 0.001 {
        return color;
    }
    let camera_pos = camera.camera_position;
    let to_pos = world_position - camera_pos;
    let dist = length(to_pos);
    let max_dist = fog.params.w;
    if max_dist > 0.0 && dist > max_dist {
        // クランプ
    }
    let falloff = fog.params.x;
    let h_ref = fog.params.y;

    // 高さ減衰積分: ray が camera から world_pos に進む間の累積密度
    let avg_height = (camera_pos.y + world_position.y) * 0.5;
    let height_factor = exp(-falloff * (avg_height - h_ref));
    let optical_depth = density * dist * height_factor;
    let transmittance = exp(-optical_depth);

    // 散乱色 (フォグ色 + 太陽方向の bias)
    var scatter_color = fog.color_density.rgb;

    // 簡易光散乱: 太陽方向との角度で色を変える
    if lights_header.counts.x > 0u {
        let sun_dir = -normalize(lights[0].position_or_dir.xyz);
        let view_dot_sun = max(dot(-view_dir_to_camera, sun_dir), 0.0);
        let scatter_strength = fog.params.z;
        let halo = pow(view_dot_sun, 8.0) * scatter_strength;
        scatter_color = scatter_color + lights[0].color_intensity.rgb * halo;
    }

    return mix(scatter_color, color, transmittance);
}

// ===== フラグメントシェーダー =====

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = material.texture_flags.x;

    // ベースカラー
    var base_color = material.base_color_factor;
    if (flags & 1u) != 0u {
        base_color = base_color * textureSample(t_base_color, s_base_color, in.tex_coords);
    }

    // メタリック・ラフネス
    var metallic = material.mr_no.x;
    var roughness = material.mr_no.y;
    if (flags & 2u) != 0u {
        let mr = textureSample(t_metallic_roughness, s_base_color, in.tex_coords);
        roughness = roughness * mr.g;
        metallic = metallic * mr.b;
    }
    roughness = clamp(roughness, 0.04, 1.0);

    // ノーマルマップ
    var n = normalize(in.world_normal);
    if (flags & 4u) != 0u {
        let tangent_normal = textureSample(t_normal, s_base_color, in.tex_coords).xyz * 2.0 - 1.0;
        let scaled = vec3<f32>(
            tangent_normal.x * material.mr_no.z,
            tangent_normal.y * material.mr_no.z,
            tangent_normal.z,
        );
        let t = normalize(in.world_tangent);
        let b = normalize(in.world_bitangent);
        let tbn = mat3x3<f32>(t, b, n);
        n = normalize(tbn * scaled);
    }

    // アンビエントオクルージョン
    var ao = 1.0;
    if (flags & 8u) != 0u {
        let occlusion = textureSample(t_occlusion, s_base_color, in.tex_coords).r;
        ao = mix(1.0, occlusion, material.mr_no.w);
    }

    // エミッシブ
    var emissive = material.emissive.rgb;
    if (flags & 16u) != 0u {
        emissive = emissive * textureSample(t_emissive, s_base_color, in.tex_coords).rgb;
    }

    let v = normalize(camera.camera_position - in.world_position);
    let f0 = mix(vec3<f32>(0.04), base_color.rgb, metallic);

    // Round 8: 拡張パラメータを取得
    let clearcoat = material.extended.x;
    let clearcoat_roughness = clamp(material.extended.y, 0.04, 1.0);
    let anisotropy = material.extended.z;
    let subsurface_strength = material.extended.w;
    let sss_color = material.subsurface_color.rgb;

    // CSM シャドウ係数 (1 つ目の directional light のみ)
    let shadow = sample_shadow_csm(in.world_position, in.view_space_position.z);

    var lo = vec3<f32>(0.0);
    let total = lights_header.counts.w;
    var first_dir_light_processed = false;
    var point_shadow_idx: i32 = 0;
    var spot_shadow_idx: i32 = 0;
    let max_point_shadows = i32(atlas.counts.x);
    let max_spot_shadows = i32(atlas.counts.y);

    for (var i: u32 = 0u; i < total; i = i + 1u) {
        let light = lights[i];
        let type_tag = bitcast<u32>(light.position_or_dir.w);

        var l: vec3<f32>;
        var radiance: vec3<f32>;

        if type_tag == 0u {
            // Directional
            l = -normalize(light.position_or_dir.xyz);
            radiance = light.color_intensity.rgb * light.color_intensity.w;
            // 1 つ目の方向光にのみシャドウ適用
            if !first_dir_light_processed {
                radiance = radiance * shadow;
                first_dir_light_processed = true;
            }
        } else if type_tag == 1u {
            // Point
            let to_light = light.position_or_dir.xyz - in.world_position;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-5);
            let att = attenuation(dist, light.params.x);
            radiance = light.color_intensity.rgb * light.color_intensity.w * att;
            // 点光源シャドウ (atlas 範囲内のみ)
            if point_shadow_idx < max_point_shadows {
                let s = sample_point_shadow(in.world_position, point_shadow_idx);
                radiance = radiance * s;
                point_shadow_idx = point_shadow_idx + 1;
            }
        } else {
            // Spot
            let to_light = light.position_or_dir.xyz - in.world_position;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-5);
            let att = attenuation(dist, light.params.x);
            let spot_dir = normalize(light.spot_dir.xyz);
            let cos_angle = dot(-l, spot_dir);
            let inner_cos = light.params.y;
            let outer_cos = light.params.z;
            let spot_factor = clamp((cos_angle - outer_cos) / max(inner_cos - outer_cos, 1e-5), 0.0, 1.0);
            radiance = light.color_intensity.rgb * light.color_intensity.w * att * spot_factor;
            // スポットライトシャドウ
            if spot_shadow_idx < max_spot_shadows {
                let s = sample_spot_shadow(in.world_position, spot_shadow_idx);
                radiance = radiance * s;
                spot_shadow_idx = spot_shadow_idx + 1;
            }
        }

        // ベース BRDF
        var light_contrib = brdf_one_light(n, v, l, radiance, base_color.rgb, metallic, roughness, f0);

        // Round 8: 異方性補正 (Ward モデル簡易版)
        if abs(anisotropy) > 0.01 {
            let tangent = normalize(in.world_tangent);
            let bitangent = normalize(in.world_bitangent);
            let h = normalize(v + l);
            let t_dot_h = dot(tangent, h);
            let b_dot_h = dot(bitangent, h);
            let alpha_t = roughness * (1.0 + anisotropy);
            let alpha_b = roughness * (1.0 - anisotropy);
            let aniso_d = 1.0 / (3.14159 * alpha_t * alpha_b) *
                exp(-((t_dot_h * t_dot_h) / (alpha_t * alpha_t) + (b_dot_h * b_dot_h) / (alpha_b * alpha_b)));
            let n_dot_l = max(dot(n, l), 0.0);
            light_contrib = light_contrib * 0.7 + vec3<f32>(aniso_d * n_dot_l * 0.3) * radiance;
        }

        // Round 8: Subsurface scattering (back-light の緩やかな透過)
        if subsurface_strength > 0.01 {
            let sss_wrap = pow(max(dot(-n, l), 0.0), 2.0) + max(dot(n, l), 0.0) * 0.3;
            light_contrib = light_contrib + sss_color * sss_wrap * radiance * subsurface_strength;
        }

        // Round 8: Clearcoat 層 (追加の specular lobe)
        if clearcoat > 0.01 {
            let h = normalize(v + l);
            let cc_f0 = vec3<f32>(0.04);
            let cc_ndf = distribution_ggx(n, h, clearcoat_roughness);
            let cc_g = geometry_smith(n, v, l, clearcoat_roughness);
            let cc_f = fresnel_schlick(max(dot(h, v), 0.0), cc_f0);
            let cc_spec = (cc_ndf * cc_g * cc_f) /
                (4.0 * max(dot(n, v), 0.0) * max(dot(n, l), 0.0) + 1e-5);
            light_contrib = light_contrib * (1.0 - clearcoat * cc_f.x) +
                cc_spec * radiance * max(dot(n, l), 0.0) * clearcoat;
        }

        lo = lo + light_contrib;
    }

    // ===== IBL (Round 4 後半) =====
    let n_dot_v = max(dot(n, v), 0.001);
    let f_roughness = fresnel_schlick_roughness(n_dot_v, f0, roughness);
    let ks_ibl = f_roughness;
    let kd_ibl = (vec3<f32>(1.0) - ks_ibl) * (1.0 - metallic);

    // 拡散項: irradiance cubemap
    let irradiance = textureSample(t_irradiance, s_ibl, n).rgb;
    let diffuse_ibl = irradiance * base_color.rgb;

    // 鏡面項: prefiltered cubemap (mip = roughness * MAX_REFLECTION_LOD) + BRDF LUT
    let r_dir = reflect(-v, n);
    let max_reflection_lod = 4.0;
    let prefiltered_color = textureSampleLevel(
        t_prefilter,
        s_ibl,
        r_dir,
        roughness * max_reflection_lod,
    ).rgb;
    let env_brdf = textureSample(t_brdf_lut, s_ibl, vec2<f32>(n_dot_v, roughness)).rg;
    let specular_ibl = prefiltered_color * (f_roughness * env_brdf.x + env_brdf.y);

    let ibl_ambient = (kd_ibl * diffuse_ibl + specular_ibl) * ao;

    // ambient_rgb は手動アンビエント (IBL の補完として)
    let ambient_rgb = lights_header.ambient.rgb * lights_header.ambient.w * base_color.rgb * ao;

    var color = ambient_rgb + ibl_ambient + lo + emissive;
    // Round 5: ボリュメトリックフォグを適用
    color = apply_fog(color, in.world_position, v);
    // HDR ターゲットへ書き込む。トーンマップとガンマ補正は post-process で行う
    return vec4<f32>(color, base_color.a);
}

/// Roughness 込みの Fresnel-Schlick (IBL 用)
fn fresnel_schlick_roughness(cos_theta: f32, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let one_minus_rough = vec3<f32>(1.0 - roughness);
    return f0 + (max(one_minus_rough, f0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}
