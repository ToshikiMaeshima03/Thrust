// GPU インスタンスドメッシュシェーダー (Round 5)
// foliage / 大量のオブジェクト用、PBR ライティング統合

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

struct LightsHeader {
    ambient: vec4<f32>,
    counts: vec4<u32>,
};

struct GpuLight {
    position_or_dir: vec4<f32>,
    color_intensity: vec4<f32>,
    params: vec4<f32>,
    spot_dir: vec4<f32>,
};

struct CsmUniform {
    matrices: array<mat4x4<f32>, 3>,
    splits: vec4<f32>,
};

struct FogUniform {
    color_density: vec4<f32>,
    params: vec4<f32>,
};

struct MaterialUniform {
    base_color_factor: vec4<f32>,
    mr_no: vec4<f32>,
    emissive: vec4<f32>,
    texture_flags: vec4<u32>,
    // Round 8: 拡張フィールド (instanced シェーダーは参照しないが layout 一致が必要)
    extended: vec4<f32>,
    aniso_dir: vec4<f32>,
    subsurface_color: vec4<f32>,
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

@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var t_base_color: texture_2d<f32>;
@group(2) @binding(2) var s_base_color: sampler;
@group(2) @binding(3) var t_metallic_roughness: texture_2d<f32>;
@group(2) @binding(4) var t_normal: texture_2d<f32>;
@group(2) @binding(5) var t_occlusion: texture_2d<f32>;
@group(2) @binding(6) var t_emissive: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    // インスタンスデータ (mat4x4 = 4 vec4)
    @location(6) instance_row0: vec4<f32>,
    @location(7) instance_row1: vec4<f32>,
    @location(8) instance_row2: vec4<f32>,
    @location(9) instance_row3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
    @location(3) view_space_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let instance_mat = mat4x4<f32>(
        in.instance_row0,
        in.instance_row1,
        in.instance_row2,
        in.instance_row3,
    );
    let world_pos = instance_mat * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;
    // 簡易: 一様スケール仮定で normal_matrix = upper-3x3
    out.world_normal = normalize((instance_mat * vec4<f32>(in.normal, 0.0)).xyz);
    out.tex_coords = in.tex_coords;
    out.view_space_position = (camera.view * world_pos).xyz;
    return out;
}

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

fn select_cascade(view_z: f32) -> i32 {
    let abs_z = abs(view_z);
    if abs_z < csm.splits.x { return 0; }
    if abs_z < csm.splits.y { return 1; }
    return 2;
}

fn sample_shadow_csm(world_position: vec3<f32>, view_z: f32) -> f32 {
    if csm.splits.w < 0.5 { return 1.0; }
    let cascade = select_cascade(view_z);
    let light_space_pos = csm.matrices[cascade] * vec4<f32>(world_position, 1.0);
    let proj = light_space_pos.xyz / max(light_space_pos.w, 1e-5);
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, -proj.y * 0.5 + 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 { return 1.0; }
    if proj.z > 1.0 || proj.z < 0.0 { return 1.0; }
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

fn apply_fog(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let density = fog.color_density.w;
    if density <= 0.001 { return color; }
    let to_pos = world_position - camera.camera_position;
    let dist = length(to_pos);
    let avg_height = (camera.camera_position.y + world_position.y) * 0.5;
    let height_factor = exp(-fog.params.x * (avg_height - fog.params.y));
    let optical_depth = density * dist * height_factor;
    let transmittance = exp(-optical_depth);
    return mix(fog.color_density.rgb, color, transmittance);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = material.texture_flags.x;
    var base_color = material.base_color_factor;
    if (flags & 1u) != 0u {
        base_color = base_color * textureSample(t_base_color, s_base_color, in.tex_coords);
    }

    var metallic = material.mr_no.x;
    var roughness = material.mr_no.y;
    if (flags & 2u) != 0u {
        let mr = textureSample(t_metallic_roughness, s_base_color, in.tex_coords);
        roughness = roughness * mr.g;
        metallic = metallic * mr.b;
    }
    roughness = clamp(roughness, 0.04, 1.0);

    let n = normalize(in.world_normal);
    let v = normalize(camera.camera_position - in.world_position);
    let f0 = mix(vec3<f32>(0.04), base_color.rgb, metallic);

    let shadow = sample_shadow_csm(in.world_position, in.view_space_position.z);

    var lo = vec3<f32>(0.0);
    let total = lights_header.counts.w;
    var first_dir = false;
    for (var i: u32 = 0u; i < total; i = i + 1u) {
        let light = lights[i];
        let type_tag = bitcast<u32>(light.position_or_dir.w);
        var l: vec3<f32>;
        var radiance: vec3<f32>;
        if type_tag == 0u {
            l = -normalize(light.position_or_dir.xyz);
            radiance = light.color_intensity.rgb * light.color_intensity.w;
            if !first_dir {
                radiance = radiance * shadow;
                first_dir = true;
            }
        } else if type_tag == 1u {
            let to_light = light.position_or_dir.xyz - in.world_position;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-5);
            let d = dist / max(light.params.x, 1e-3);
            let factor = clamp(1.0 - d * d * d * d, 0.0, 1.0);
            let att = factor * factor / (dist * dist + 1e-3);
            radiance = light.color_intensity.rgb * light.color_intensity.w * att;
        } else {
            let to_light = light.position_or_dir.xyz - in.world_position;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-5);
            let d = dist / max(light.params.x, 1e-3);
            let factor = clamp(1.0 - d * d * d * d, 0.0, 1.0);
            let att = factor * factor / (dist * dist + 1e-3);
            let cos_a = dot(-l, normalize(light.spot_dir.xyz));
            let spot = clamp((cos_a - light.params.z) / max(light.params.y - light.params.z, 1e-5), 0.0, 1.0);
            radiance = light.color_intensity.rgb * light.color_intensity.w * att * spot;
        }

        let h = normalize(v + l);
        let n_dot_l = max(dot(n, l), 0.0);
        if n_dot_l > 0.0 {
            let ndf = distribution_ggx(n, h, roughness);
            let g = geometry_smith(n, v, l, roughness);
            let f = fresnel_schlick(max(dot(h, v), 0.0), f0);
            let specular = (ndf * g * f) / (4.0 * max(dot(n, v), 0.0) * n_dot_l + 1e-5);
            let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);
            lo = lo + (kd * base_color.rgb / PI + specular) * radiance * n_dot_l;
        }
    }

    let irradiance = textureSample(t_irradiance, s_ibl, n).rgb;
    let ambient = (lights_header.ambient.rgb * lights_header.ambient.w + irradiance * 0.5) * base_color.rgb;

    var color = ambient + lo + material.emissive.rgb;
    color = apply_fog(color, in.world_position);
    return vec4<f32>(color, base_color.a);
}
