use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use glam::{Vec3, Vec4};
use hecs::World;

use crate::math::SimpleRng;
use crate::renderer::texture::ThrustTexture;
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

/// エミッター生成ごとにインクリメントされるグローバルカウンター（一意シード生成用）
static EMITTER_COUNTER: AtomicU32 = AtomicU32::new(0);

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
    /// パーティクルテクスチャ（None = 円形フェードフォールバック）
    pub texture: Option<Arc<ThrustTexture>>,

    // 内部状態
    pub(crate) particles: Vec<Particle>,
    pub(crate) emit_accumulator: f32,
    pub(crate) rng: SimpleRng,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        // 各エミッターに一意のシードを割り当て、同一パターンの発生を防止
        let seed = EMITTER_COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(2654435761);
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
            texture: None,
            particles: Vec::new(),
            emit_accumulator: 0.0,
            rng: SimpleRng::new(seed),
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

/// エミッターカウンターをリセットする（テスト用）
#[cfg(test)]
pub(crate) fn reset_emitter_counter() {
    EMITTER_COUNTER.store(0, Ordering::Relaxed);
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
                    emitter.rng.range(
                        emitter.initial_velocity_min.x,
                        emitter.initial_velocity_max.x,
                    ),
                    emitter.rng.range(
                        emitter.initial_velocity_min.y,
                        emitter.initial_velocity_max.y,
                    ),
                    emitter.rng.range(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_is_alive() {
        let p = Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 2.0,
            age: 0.0,
        };
        assert!(p.is_alive());

        let dead = Particle { age: 2.0, ..p };
        assert!(!dead.is_alive());
    }

    #[test]
    fn test_particle_normalized_age() {
        let p = Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 4.0,
            age: 2.0,
        };
        assert!((p.normalized_age() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_normalized_age_clamped() {
        let p = Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 1.0,
            age: 5.0,
        };
        assert!((p.normalized_age() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_emitter_unique_seeds() {
        reset_emitter_counter();
        let e1 = ParticleEmitter::default();
        let e2 = ParticleEmitter::default();
        assert_ne!(
            e1.rng.seed(),
            e2.rng.seed(),
            "各エミッターは異なるシードを持つべき"
        );
    }

    #[test]
    fn test_emitter_alive_count() {
        let mut emitter = ParticleEmitter::default();
        emitter.particles.push(Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 2.0,
            age: 0.0,
        });
        emitter.particles.push(Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 2.0,
            age: 3.0, // 死亡
        });
        assert_eq!(emitter.alive_count(), 1);
    }

    #[test]
    fn test_particle_system_emits() {
        let mut world = World::new();
        let emitter = ParticleEmitter {
            emission_rate: 100.0,
            ..Default::default()
        };
        world.spawn((Transform::default(), emitter));

        // 1秒分のステップ → 約100パーティクルが生成されるはず
        particle_system(&mut world, 1.0);

        for e in world.query::<&ParticleEmitter>().iter() {
            assert!(
                e.alive_count() > 50,
                "1秒で十分なパーティクルが生成されるべき: {}",
                e.alive_count()
            );
        }
    }

    #[test]
    fn test_particle_system_respects_max() {
        let mut world = World::new();
        let emitter = ParticleEmitter {
            emission_rate: 10000.0,
            max_particles: 10,
            ..Default::default()
        };
        world.spawn((Transform::default(), emitter));

        particle_system(&mut world, 1.0);

        for e in world.query::<&ParticleEmitter>().iter() {
            assert!(
                e.particles.len() <= 10,
                "max_particles を超えないべき: {}",
                e.particles.len()
            );
        }
    }

    #[test]
    fn test_particle_system_inactive() {
        let mut world = World::new();
        let emitter = ParticleEmitter {
            active: false,
            ..Default::default()
        };
        world.spawn((Transform::default(), emitter));

        particle_system(&mut world, 1.0);

        for e in world.query::<&ParticleEmitter>().iter() {
            assert_eq!(
                e.alive_count(),
                0,
                "非アクティブエミッターはパーティクルを生成しないべき"
            );
        }
    }

    #[test]
    fn test_particle_system_removes_dead() {
        let mut world = World::new();
        let mut emitter = ParticleEmitter {
            active: false,
            ..Default::default()
        };
        emitter.particles.push(Particle {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 0.5,
            age: 0.0,
        });
        world.spawn((Transform::default(), emitter));

        // 1秒経過 → パーティクルは寿命を超えて死亡
        particle_system(&mut world, 1.0);

        for e in world.query::<&ParticleEmitter>().iter() {
            assert_eq!(
                e.particles.len(),
                0,
                "寿命を超えたパーティクルは除去されるべき"
            );
        }
    }

    #[test]
    fn test_particle_gravity_applied() {
        let mut world = World::new();
        let mut emitter = ParticleEmitter {
            active: false,
            gravity: Vec3::new(0.0, -10.0, 0.0),
            ..Default::default()
        };
        emitter.particles.push(Particle {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 5.0,
            age: 0.0,
        });
        world.spawn((Transform::default(), emitter));

        particle_system(&mut world, 1.0);

        for e in world.query::<&ParticleEmitter>().iter() {
            let p = &e.particles[0];
            assert!(
                p.position.y < 10.0,
                "重力で下降するべき: y={}",
                p.position.y
            );
            assert!(
                p.velocity.y < 0.0,
                "重力で速度が下向きになるべき: vy={}",
                p.velocity.y
            );
        }
    }
}
