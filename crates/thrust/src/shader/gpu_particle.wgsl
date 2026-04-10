// GPU Particle compute shader (Round 8)
//
// パーティクルを compute で更新する。各 invocation が 1 パーティクルを処理。
// パーティクルは Storage Buffer に格納され、頂点シェーダーから read で描画される。

struct GpuParticle {
    /// xyz = position, w = lifetime
    pos_life: vec4<f32>,
    /// xyz = velocity, w = age
    vel_age: vec4<f32>,
    /// rgba = color
    color: vec4<f32>,
    /// x = size, y = active flag, z = _, w = _
    misc: vec4<f32>,
};

struct SimParams {
    /// xyz = gravity, w = dt
    gravity_dt: vec4<f32>,
    /// xyz = wind force, w = drag
    wind_drag: vec4<f32>,
    /// x = num_particles, y = seed, z = emit_per_step, w = _
    counts: vec4<f32>,
};

@group(0) @binding(0) var<storage, read_write> particles: array<GpuParticle>;
@group(0) @binding(1) var<uniform> params: SimParams;

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5);
}

@compute @workgroup_size(64)
fn cs_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total = u32(params.counts.x);
    if idx >= total {
        return;
    }

    var p = particles[idx];

    if p.misc.y < 0.5 {
        // 非アクティブ → 確率的に再生成
        let h = hash21(vec2<f32>(f32(idx) * 0.013, params.counts.y));
        if h > 0.95 {
            // 中心から放射方向に発射
            let phi = h * 6.28318;
            let theta = hash21(vec2<f32>(f32(idx) * 0.029, params.counts.y + 1.0)) * 3.14159;
            let dir = vec3<f32>(
                sin(theta) * cos(phi),
                cos(theta) * 0.5 + 0.5,
                sin(theta) * sin(phi),
            );
            p.pos_life = vec4<f32>(0.0, 0.0, 0.0, 2.0);
            p.vel_age = vec4<f32>(dir * 5.0, 0.0);
            p.color = vec4<f32>(1.0, 0.6, 0.2, 1.0);
            p.misc = vec4<f32>(0.3, 1.0, 0.0, 0.0);
        }
    } else {
        // アクティブ → 物理更新
        let dt = params.gravity_dt.w;
        var vel = p.vel_age.xyz + params.gravity_dt.xyz * dt + params.wind_drag.xyz * dt;
        vel = vel * (1.0 - params.wind_drag.w * dt);
        let pos = p.pos_life.xyz + vel * dt;
        let age = p.vel_age.w + dt;
        let lifetime = p.pos_life.w;

        if age >= lifetime {
            // 寿命切れ
            p.misc.y = 0.0;
        } else {
            p.pos_life = vec4<f32>(pos, lifetime);
            p.vel_age = vec4<f32>(vel, age);
            // alpha フェード
            let life_t = age / lifetime;
            p.color.a = 1.0 - life_t;
        }
    }

    particles[idx] = p;
}
