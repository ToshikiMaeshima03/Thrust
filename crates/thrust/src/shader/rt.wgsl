// Software Ray Tracing (Round 9)
//
// Compute shader で簡易レイトレを行う。シーンは球の集合で表現し、
// 各ピクセルから 1 本のレイを発射 → 最も近い球と交差判定 → 単純シェーディング。
// BVH なしの brute-force だが、球数 ~256 まで実用的。
// 影/AO/反射の追加サンプリング用に拡張可能。

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

struct RtSphere {
    /// xyz = center, w = radius
    center_radius: vec4<f32>,
    /// rgba = albedo (a = unused)
    albedo: vec4<f32>,
    /// x = metallic, y = roughness, z = emission, w = _
    material: vec4<f32>,
};

struct RtParams {
    /// xyz = sun direction, w = num_spheres
    sun_count: vec4<f32>,
    /// rgb = sun color, a = sky intensity
    sun_sky: vec4<f32>,
    /// x = max_bounces, y = max_t, z = enabled, w = _
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> rt_params: RtParams;
@group(0) @binding(2) var<storage, read> spheres: array<RtSphere>;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba16float, write>;

struct Ray {
    origin: vec3<f32>,
    direction: vec3<f32>,
};

struct Hit {
    t: f32,
    position: vec3<f32>,
    normal: vec3<f32>,
    sphere_idx: i32,
};

fn no_hit() -> Hit {
    return Hit(1e30, vec3<f32>(0.0), vec3<f32>(0.0, 1.0, 0.0), -1);
}

fn intersect_sphere(ray: Ray, sphere: RtSphere) -> f32 {
    let oc = ray.origin - sphere.center_radius.xyz;
    let a = dot(ray.direction, ray.direction);
    let b = 2.0 * dot(oc, ray.direction);
    let c = dot(oc, oc) - sphere.center_radius.w * sphere.center_radius.w;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return -1.0;
    }
    let sq = sqrt(disc);
    let t0 = (-b - sq) / (2.0 * a);
    let t1 = (-b + sq) / (2.0 * a);
    if t0 > 0.001 {
        return t0;
    }
    if t1 > 0.001 {
        return t1;
    }
    return -1.0;
}

fn trace_scene(ray: Ray) -> Hit {
    var best = no_hit();
    let n = u32(rt_params.sun_count.w);
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let sphere = spheres[i];
        let t = intersect_sphere(ray, sphere);
        if t > 0.0 && t < best.t {
            best.t = t;
            best.position = ray.origin + ray.direction * t;
            best.normal = normalize(best.position - sphere.center_radius.xyz);
            best.sphere_idx = i32(i);
        }
    }
    return best;
}

fn shade(hit: Hit, ray_dir: vec3<f32>) -> vec3<f32> {
    if hit.sphere_idx < 0 {
        // 空: ダウンウェルダラインスカイ
        let t = max(ray_dir.y, 0.0);
        let sky = mix(vec3<f32>(0.6, 0.8, 1.0), vec3<f32>(0.3, 0.5, 0.9), t);
        return sky * rt_params.sun_sky.a;
    }
    let sphere = spheres[hit.sphere_idx];
    let albedo = sphere.albedo.rgb;
    let emission = sphere.material.z;

    let sun_dir = normalize(rt_params.sun_count.xyz);
    // 影レイ
    let shadow_ray = Ray(hit.position + hit.normal * 0.001, -sun_dir);
    let shadow_hit = trace_scene(shadow_ray);
    let in_shadow = shadow_hit.sphere_idx >= 0;

    let n_dot_l = max(dot(hit.normal, -sun_dir), 0.0);
    var color = albedo * 0.15; // ambient
    if !in_shadow {
        color = color + albedo * rt_params.sun_sky.rgb * n_dot_l;
    }
    color = color + albedo * emission;
    return color;
}

@compute @workgroup_size(8, 8)
fn cs_trace(@builtin(global_invocation_id) gid: vec3<u32>) {
    if rt_params.misc.z < 0.5 {
        return;
    }
    let dim = vec2<u32>(camera.viewport.xy);
    if gid.x >= dim.x || gid.y >= dim.y {
        return;
    }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(dim);
    // UV → world ray
    let ndc = vec3<f32>(uv.x * 2.0 - 1.0, -(uv.y * 2.0 - 1.0), 1.0);
    let world4 = camera.inv_view_proj * vec4<f32>(ndc, 1.0);
    let world_pos = world4.xyz / max(world4.w, 1e-5);
    let ray_dir = normalize(world_pos - camera.camera_position);
    let ray = Ray(camera.camera_position, ray_dir);

    let hit = trace_scene(ray);
    var color = shade(hit, ray_dir);

    // 1 バウンス反射 (粗いの除外)
    if hit.sphere_idx >= 0 {
        let sphere = spheres[hit.sphere_idx];
        if sphere.material.x > 0.5 && sphere.material.y < 0.3 {
            // 金属で滑らか: 反射
            let reflect_dir = reflect(ray_dir, hit.normal);
            let reflect_ray = Ray(hit.position + hit.normal * 0.001, reflect_dir);
            let reflect_hit = trace_scene(reflect_ray);
            let reflect_color = shade(reflect_hit, reflect_dir);
            color = mix(color, reflect_color, sphere.material.x * 0.5);
        }
    }

    textureStore(output_tex, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}
