use glam::Vec3;
use hecs::{Entity, World};

use crate::math::Aabb;
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

/// コリジョン形状
#[derive(Debug, Clone)]
pub enum ColliderShape {
    /// 軸平行バウンディングボックス（ローカル空間）
    Aabb(Aabb),
    /// 球（ローカル空間）
    Sphere { center: Vec3, radius: f32 },
}

/// コライダーコンポーネント
pub struct Collider {
    pub shape: ColliderShape,
    /// トリガーの場合、物理応答なし（重なり検出のみ）
    pub is_trigger: bool,
}

/// 速度コンポーネント
pub struct Velocity {
    pub linear: Vec3,
}

impl Default for Velocity {
    fn default() -> Self {
        Self { linear: Vec3::ZERO }
    }
}

/// コリジョンペア（衝突した2つのエンティティ）
#[derive(Debug, Clone, Copy)]
pub struct CollisionPair {
    pub entity_a: Entity,
    pub entity_b: Entity,
}

/// コリジョンイベント（イベントシステム経由で配信）
#[derive(Debug, Clone, Copy)]
pub struct CollisionEvent {
    pub entity_a: Entity,
    pub entity_b: Entity,
}

/// 速度システム: Velocity を Transform に適用する
pub fn velocity_system(world: &mut World, dt: f32) {
    for (transform, velocity) in world.query_mut::<(&mut Transform, &Velocity)>() {
        transform.translation += velocity.linear * dt;
    }
}

/// コリジョン検出システム: AABB/Sphere の重なりを検出し、CollisionEvent を発行する
pub fn collision_system(world: &World, events: &mut crate::event::Events) {
    // ワールド空間の AABB を計算してエンティティとペアで収集
    let mut colliders: Vec<(Entity, Aabb)> = Vec::new();

    for (entity, collider, transform, global_transform) in world
        .query::<(Entity, &Collider, &Transform, Option<&GlobalTransform>)>()
        .iter()
    {
        let matrix = match global_transform {
            Some(gt) => gt.0,
            None => transform.to_matrix(),
        };

        let world_aabb = match &collider.shape {
            ColliderShape::Aabb(aabb) => aabb.transformed(&matrix),
            ColliderShape::Sphere { center, radius } => {
                let world_center = matrix.transform_point3(*center);
                // スケールを考慮した近似半径
                let scale = crate::math::extract_max_scale(&matrix);
                let world_radius = radius * scale;
                Aabb::new(
                    world_center - Vec3::splat(world_radius),
                    world_center + Vec3::splat(world_radius),
                )
            }
        };

        colliders.push((entity, world_aabb));
    }

    // ブルートフォース O(n^2) 交差判定
    for i in 0..colliders.len() {
        for j in (i + 1)..colliders.len() {
            if colliders[i].1.intersects(&colliders[j].1) {
                events.send(CollisionEvent {
                    entity_a: colliders[i].0,
                    entity_b: colliders[j].0,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Events;

    #[test]
    fn test_velocity_system_basic() {
        let mut world = World::new();
        let entity = world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Velocity {
                linear: Vec3::new(1.0, 0.0, 0.0),
            },
        ));

        velocity_system(&mut world, 0.5);

        let t = world.get::<&Transform>(entity).unwrap();
        assert!(
            (t.translation.x - 0.5).abs() < 1e-5,
            "0.5秒後の位置は 0.5 であるべき: {}",
            t.translation.x
        );
    }

    #[test]
    fn test_velocity_system_zero_dt() {
        let mut world = World::new();
        let entity = world.spawn((
            Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
            Velocity {
                linear: Vec3::new(10.0, 0.0, 0.0),
            },
        ));

        velocity_system(&mut world, 0.0);

        let t = world.get::<&Transform>(entity).unwrap();
        assert!(
            (t.translation.x - 5.0).abs() < 1e-5,
            "dt=0 では位置が変わらないべき"
        );
    }

    #[test]
    fn test_collision_system_aabb_overlap() {
        let mut world = World::new();
        let mut events = Events::new();

        // 重なる2つの AABB コライダー
        world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: false,
            },
        ));
        world.spawn((
            Transform::from_translation(Vec3::new(0.5, 0.0, 0.0)),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: false,
            },
        ));

        collision_system(&world, &mut events);

        let collisions = events.read::<CollisionEvent>();
        assert_eq!(
            collisions.len(),
            1,
            "重なるAABBで1件の衝突イベントが発行されるべき"
        );
    }

    #[test]
    fn test_collision_system_no_overlap() {
        let mut world = World::new();
        let mut events = Events::new();

        // 重ならない2つの AABB コライダー
        world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: false,
            },
        ));
        world.spawn((
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: false,
            },
        ));

        collision_system(&world, &mut events);

        let collisions = events.read::<CollisionEvent>();
        assert!(collisions.is_empty(), "離れたAABBでは衝突しないべき");
    }

    #[test]
    fn test_collision_system_sphere() {
        let mut world = World::new();
        let mut events = Events::new();

        world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Collider {
                shape: ColliderShape::Sphere {
                    center: Vec3::ZERO,
                    radius: 1.0,
                },
                is_trigger: false,
            },
        ));
        world.spawn((
            Transform::from_translation(Vec3::new(1.5, 0.0, 0.0)),
            Collider {
                shape: ColliderShape::Sphere {
                    center: Vec3::ZERO,
                    radius: 1.0,
                },
                is_trigger: false,
            },
        ));

        collision_system(&world, &mut events);

        // 球のAABB近似で重なる（中心距離 1.5 < 半径合計 2.0）
        let collisions = events.read::<CollisionEvent>();
        assert_eq!(
            collisions.len(),
            1,
            "重なる球で衝突イベントが発行されるべき"
        );
    }

    #[test]
    fn test_collision_system_three_bodies() {
        let mut world = World::new();
        let mut events = Events::new();

        // 3つ全て重なる配置
        for i in 0..3 {
            world.spawn((
                Transform::from_translation(Vec3::new(i as f32 * 0.5, 0.0, 0.0)),
                Collider {
                    shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                    is_trigger: false,
                },
            ));
        }

        collision_system(&world, &mut events);

        let collisions = events.read::<CollisionEvent>();
        assert_eq!(
            collisions.len(),
            3,
            "3体が全て重なる場合、3ペアの衝突が発生すべき"
        );
    }

    #[test]
    fn test_velocity_default() {
        let v = Velocity::default();
        assert_eq!(v.linear, Vec3::ZERO);
    }
}
