//! rapier3d とのブリッジシステム (Round 4)

use glam::Vec3;
use hecs::{Entity, World};
use rapier3d::prelude::*;

use crate::physics::collider::{Collider, ColliderShape, Velocity};
use crate::physics::rapier_world::{PhysicsHandle, PhysicsWorld, RigidBody, RigidBodyType};
use crate::scene::transform::Transform;

/// 新規エンティティに `RigidBody` + `Collider` がある場合、rapier ハンドルを生成する
///
/// 既存の `Collider` のみのエンティティ (RigidBody なし) は legacy collision_system が処理する。
pub fn physics_init_system(world: &mut World, physics: &mut PhysicsWorld) {
    // 初期化が必要なエンティティを収集
    let needs_init: Vec<Entity> = world
        .query::<(Entity, &RigidBody, &Collider, &Transform)>()
        .without::<&PhysicsHandle>()
        .iter()
        .map(|(e, _rb, _c, _t)| e)
        .collect();

    for entity in needs_init {
        // 全てのデータを抽出してから borrow を解放する
        let extracted = {
            let Ok(entity_ref) = world.entity(entity) else {
                continue;
            };
            let Some(rb) = entity_ref.get::<&RigidBody>() else {
                continue;
            };
            let Some(coll) = entity_ref.get::<&Collider>() else {
                continue;
            };
            let Some(t) = entity_ref.get::<&Transform>() else {
                continue;
            };
            let initial_vel = entity_ref
                .get::<&Velocity>()
                .map(|v| v.linear)
                .unwrap_or(rb.initial_velocity);

            (
                rb.body_type,
                rb.linear_damping,
                rb.angular_damping,
                t.translation,
                t.rotation,
                coll.shape.clone(),
                coll.is_trigger,
                initial_vel,
            )
        };

        let (
            body_type,
            linear_damping,
            angular_damping,
            translation,
            rotation,
            shape,
            is_trigger,
            initial_vel,
        ) = extracted;

        let euler = rotation.to_euler(glam::EulerRot::XYZ);
        let body_builder = match body_type {
            RigidBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            RigidBodyType::Fixed => RigidBodyBuilder::fixed(),
            RigidBodyType::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
        }
        .translation(vector![translation.x, translation.y, translation.z])
        .rotation(vector![euler.0, euler.1, euler.2])
        .linvel(vector![initial_vel.x, initial_vel.y, initial_vel.z])
        .linear_damping(linear_damping)
        .angular_damping(angular_damping);

        let body = body_builder.build();
        let body_handle = physics.bodies.insert(body);

        let collider_builder = match &shape {
            ColliderShape::Aabb(aabb) => {
                let half = (aabb.max - aabb.min) * 0.5;
                ColliderBuilder::cuboid(half.x.abs(), half.y.abs(), half.z.abs())
            }
            ColliderShape::Sphere { radius, .. } => ColliderBuilder::ball(*radius),
        };
        let collider_builder = if is_trigger {
            collider_builder.sensor(true)
        } else {
            collider_builder
        };

        let collider_handle = physics.colliders.insert_with_parent(
            collider_builder.build(),
            body_handle,
            &mut physics.bodies,
        );

        let _ = world.insert_one(
            entity,
            PhysicsHandle {
                body: body_handle,
                collider: collider_handle,
            },
        );
    }
}

/// rapier の物理ステップを進める
pub fn physics_step_system(physics: &mut PhysicsWorld, dt: f32) {
    if dt <= 0.0 || dt > 1.0 {
        // スパイクや一時停止時はスキップ
        return;
    }
    physics.step(dt);
}

/// rapier の Dynamic ボディの位置・回転を ECS Transform へ反映する
pub fn physics_sync_from_system(world: &mut World, physics: &PhysicsWorld) {
    for (handle, transform, rb) in world.query_mut::<(&PhysicsHandle, &mut Transform, &RigidBody)>()
    {
        if rb.body_type == RigidBodyType::Fixed {
            continue;
        }
        if let Some(body) = physics.bodies.get(handle.body) {
            let pos = body.translation();
            transform.translation = Vec3::new(pos.x, pos.y, pos.z);
            let rot = body.rotation();
            transform.rotation = glam::Quat::from_xyzw(rot.i, rot.j, rot.k, rot.w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Aabb;
    use crate::physics::collider::ColliderShape;

    #[test]
    fn test_physics_init_creates_handle() {
        let mut world = World::new();
        let mut physics = PhysicsWorld::new();
        let entity = world.spawn((
            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)),
            RigidBody::dynamic(),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5))),
                is_trigger: false,
            },
        ));

        physics_init_system(&mut world, &mut physics);

        assert!(world.get::<&PhysicsHandle>(entity).is_ok());
        assert_eq!(physics.bodies.len(), 1);
    }

    #[test]
    fn test_dynamic_body_syncs_back_after_fall() {
        let mut world = World::new();
        let mut physics = PhysicsWorld::new();
        let entity = world.spawn((
            Transform::from_translation(Vec3::new(0.0, 10.0, 0.0)),
            RigidBody::dynamic(),
            Collider {
                shape: ColliderShape::Sphere {
                    center: Vec3::ZERO,
                    radius: 0.5,
                },
                is_trigger: false,
            },
        ));

        physics_init_system(&mut world, &mut physics);

        // 30 ステップ (0.5 秒) 進める
        for _ in 0..30 {
            physics_step_system(&mut physics, 1.0 / 60.0);
        }
        physics_sync_from_system(&mut world, &physics);

        let t = world.get::<&Transform>(entity).unwrap();
        // 0.5 秒で約 1.225 m 落下 → 8.775 付近
        assert!(
            t.translation.y < 10.0 && t.translation.y > 7.0,
            "落下していない: y={}",
            t.translation.y
        );
    }
}
