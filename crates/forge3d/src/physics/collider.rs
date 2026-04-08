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
                let scale = matrix
                    .transform_vector3(Vec3::X)
                    .length()
                    .max(matrix.transform_vector3(Vec3::Y).length())
                    .max(matrix.transform_vector3(Vec3::Z).length());
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
