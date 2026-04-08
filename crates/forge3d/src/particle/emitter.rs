use glam::{Vec3, Vec4};
use hecs::World;

use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

/// 個別のパーティクル（ECS エンティティではなく、プレーンデータ）
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: Vec4,
    pub size: f32,
    pub lifetime: f32,
    pub age: f32,
}

impl Particle {
    /// パーティクルが生存中か判定
    pub fn is_alive(&self) -> bool {
        self.age < self.lifetime
    }

    /// 正規化された年齢 (0.0 = 生成直後, 1.0 = 寿命終了)
    pub fn normalized_age(&self) -> f32 {
        (self.age / self.lifetime).min(1.0)
    }
}

/// パーティクルエミッターコンポーネント
///
/// Transform を持つエンティティにアタッチする。
/// 内部にパーティクルプールを保持し、`particle_system` で更新される。
pub struct ParticleEmitter {
    /// パーティクル放出レート（パーティクル/秒）
    pub emission_rate: f32,
    /// パーティクルの寿命（秒）
    pub particle_lifetime: f32,
    /// 最小初速
    pub initial_velocity_min: Vec3,
    /// 最大初速
    pub initial_velocity_max: Vec3,
    /// 初期色（RGBA、A はフェードに使用）
    pub initial_color: Vec4,
    /// 初期サイズ
    pub initial_size: f32,
    /// 寿命終了時のサイズ倍率 (1.0 = 変化なし, 0.0 = 消滅)
    pub size_over_lifetime: f32,
    /// アルファフェードアウト
    pub fade_out: bool,
    /// 重力加速度
    pub gravity: Vec3,
    /// プール最大サイズ
    pub max_particles: usize,
    /// 放出中か否か
    pub active: bool,

    // 内部状態
    pub(crate) particles: Vec<Particle>,
    pub(crate) emit_accumulator: f32,
    pub(crate) rng_seed: u32,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            emission_rate: 50.0,
            particle_lifetime: 2.0,
            initial_velocity_min: Vec3::new(-0.5, 1.0, -0.5),
            initial_velocity_max: Vec3::new(0.5, 3.0, 0.5),
            initial_color: Vec4::new(1.0, 0.8, 0.2, 1.0),
            initial_size: 0.1,
            size_over_lifetime: 0.0,
            fade_out: true,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            max_particles: 1000,
            active: true,
            particles: Vec::new(),
            emit_accumulator: 0.0,
            rng_seed: 42,
        }
    }
}

impl ParticleEmitter {
    /// 現在の生存パーティクル数
    pub fn alive_count(&self) -> usize {
        self.particles.iter().filter(|p| p.is_alive()).count()
    }

    /// 生存中パーティクルのスライスを取得
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }
}

/// 簡易擬似乱数（LCG、外部クレート不使用）
fn simple_rand(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    ((*seed >> 16) & 0x7FFF) as f32 / 32767.0
}

fn rand_range(seed: &mut u32, min: f32, max: f32) -> f32 {
    min + simple_rand(seed) * (max - min)
}

/// パーティクルシステム: 全エミッターのパーティクルを更新する
pub fn particle_system(world: &mut World, dt: f32) {
    for (emitter, transform, global_transform) in
        world.query_mut::<(&mut ParticleEmitter, &Transform, Option<&GlobalTransform>)>()
    {
        let world_pos = match global_transform {
            Some(gt) => gt.0.transform_point3(Vec3::ZERO),
            None => transform.translation,
        };

        // 1. 既存パーティクルを更新
        for particle in &mut emitter.particles {
            if !particle.is_alive() {
                continue;
            }
            particle.velocity += emitter.gravity * dt;
            particle.position += particle.velocity * dt;
            particle.age += dt;

            // サイズ補間
            let t = particle.normalized_age();
            particle.size = emitter.initial_size * (1.0 - t + t * emitter.size_over_lifetime);

            // フェードアウト
            if emitter.fade_out {
                particle.color.w = emitter.initial_color.w * (1.0 - t);
            }
        }

        // 2. 死んだパーティクルを除去
        emitter.particles.retain(|p| p.is_alive());

        // 3. 新しいパーティクルを放出
        if emitter.active {
            emitter.emit_accumulator += emitter.emission_rate * dt;
            let to_emit = emitter.emit_accumulator as usize;
            emitter.emit_accumulator -= to_emit as f32;

            for _ in 0..to_emit {
                if emitter.particles.len() >= emitter.max_particles {
                    break;
                }

                let velocity = Vec3::new(
                    rand_range(
                        &mut emitter.rng_seed,
                        emitter.initial_velocity_min.x,
                        emitter.initial_velocity_max.x,
                    ),
                    rand_range(
                        &mut emitter.rng_seed,
                        emitter.initial_velocity_min.y,
                        emitter.initial_velocity_max.y,
                    ),
                    rand_range(
                        &mut emitter.rng_seed,
                        emitter.initial_velocity_min.z,
                        emitter.initial_velocity_max.z,
                    ),
                );

                emitter.particles.push(Particle {
                    position: world_pos,
                    velocity,
                    color: emitter.initial_color,
                    size: emitter.initial_size,
                    lifetime: emitter.particle_lifetime,
                    age: 0.0,
                });
            }
        }
    }
}
