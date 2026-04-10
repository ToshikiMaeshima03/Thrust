// GPU 駆動カリング (Round 9)
//
// Compute shader でクラスタ/メッシュインスタンスのフラスタムカリングを行い、
// 可視のものだけ DrawIndirect 引数に書き込む。Nanite 風のクラスタカリング
// の最も基本的な部分を実装。

struct InstanceBound {
    /// xyz = center, w = radius (sphere bound)
    center_radius: vec4<f32>,
    /// xyz = aabb min, w = _
    aabb_min: vec4<f32>,
    /// xyz = aabb max, w = _
    aabb_max: vec4<f32>,
    /// x = mesh_id, y = first_index, z = index_count, w = base_vertex
    draw_info: vec4<u32>,
};

struct DrawIndirectArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
};

struct CullingParams {
    /// 6 個のフラスタム平面 (Hessian 形式 ax+by+cz+d=0、d は -plane.dot(normal))
    planes: array<vec4<f32>, 6>,
    /// x = total_instances, y = _, z = _, w = _
    counts: vec4<u32>,
    /// xyz = camera_position, w = _
    camera: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> instances: array<InstanceBound>;
@group(0) @binding(1) var<storage, read_write> draw_args: array<DrawIndirectArgs>;
@group(0) @binding(2) var<storage, read_write> visible_count: atomic<u32>;
@group(0) @binding(3) var<uniform> params: CullingParams;

/// Sphere vs frustum 6 planes test
fn sphere_in_frustum(center: vec3<f32>, radius: f32) -> bool {
    for (var i: u32 = 0u; i < 6u; i = i + 1u) {
        let plane = params.planes[i];
        let d = dot(plane.xyz, center) + plane.w;
        if d < -radius {
            return false;
        }
    }
    return true;
}

@compute @workgroup_size(64)
fn cs_cull(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total = params.counts.x;
    if idx >= total {
        return;
    }

    let inst = instances[idx];
    let center = inst.center_radius.xyz;
    let radius = inst.center_radius.w;

    if sphere_in_frustum(center, radius) {
        let out_idx = atomicAdd(&visible_count, 1u);
        var args: DrawIndirectArgs;
        args.index_count = inst.draw_info.z;
        args.instance_count = 1u;
        args.first_index = inst.draw_info.y;
        args.base_vertex = i32(inst.draw_info.w);
        args.first_instance = idx;
        draw_args[out_idx] = args;
    }
}
