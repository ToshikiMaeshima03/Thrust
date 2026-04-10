//! キャラクターコントローラー (Round 7)
//!
//! rapier3d の `KinematicCharacterController` ラッパー。重力 + 滑り + 段差登りに対応する
//! シンプルな移動コントローラを提供する。
//!
//! ECS で `CharacterController` コンポーネントを持つエンティティに対して、
//! `character_controller_system` が `desired_velocity` を入力として実際の移動量を計算し、
//! `Transform` を更新する。

use glam::Vec3;
use hecs::World;
use nalgebra::vector;
use rapier3d::control::{CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::physics::PhysicsHandle;
use crate::physics::PhysicsWorld;
use crate::scene::transform::Transform;

/// キャラクターコントローラー定義 (ECS コンポーネント)
#[derive(Debug, Clone)]
pub struct CharacterController {
    /// プレイヤー入力で決まる目標速度 (m/s)。Y は重力で上書きされる
    pub desired_velocity: Vec3,
    /// 重力によって貯まる垂直速度
    pub vertical_velocity: f32,
    /// 段差を上れる最大高さ (m)
    pub max_step_height: f32,
    /// スロープの最大角度 (rad)
    pub max_slope_angle: f32,
    /// 床にいるかどうか (毎フレーム更新される)
    pub grounded: bool,
    /// ジャンプ要求 (true なら次のステップでジャンプ)
    pub jump_request: bool,
    /// ジャンプ初速度
    pub jump_speed: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            desired_velocity: Vec3::ZERO,
            vertical_velocity: 0.0,
            max_step_height: 0.3,
            max_slope_angle: 45.0_f32.to_radians(),
            grounded: false,
            jump_request: false,
            jump_speed: 5.0,
        }
    }
}

/// キャラクターコントローラーシステム
///
/// `CharacterController + PhysicsHandle + Transform` を持つエンティティを対象とする。
/// rapier の `KinematicCharacterController` を使って衝突しないように移動量を補正する。
pub fn character_controller_system(world: &mut World, physics: &mut PhysicsWorld, dt: f32) {
    let entities: Vec<hecs::Entity> = world
        .query::<(
            hecs::Entity,
            &CharacterController,
            &PhysicsHandle,
            &Transform,
        )>()
        .iter()
        .map(|(e, _, _, _)| e)
        .collect();

    for entity in entities {
        let Ok(handle_ref) = world.get::<&PhysicsHandle>(entity) else {
            continue;
        };
        let collider_handle = handle_ref.collider;
        drop(handle_ref);

        // CharacterController を可変借用
        let Ok(mut cc) = world.get::<&mut CharacterController>(entity) else {
            continue;
        };

        // 重力 + ジャンプ
        cc.vertical_velocity += physics.gravity.y * dt;
        if cc.jump_request && cc.grounded {
            cc.vertical_velocity = cc.jump_speed;
            cc.jump_request = false;
            cc.grounded = false;
        }

        let desired_xz = vector![cc.desired_velocity.x, 0.0, cc.desired_velocity.z];
        let desired = vector![desired_xz.x, cc.vertical_velocity, desired_xz.z] * dt;

        let controller = KinematicCharacterController {
            up: Vector::y_axis(),
            offset: CharacterLength::Absolute(0.01),
            slide: true,
            autostep: Some(rapier3d::control::CharacterAutostep {
                max_height: CharacterLength::Absolute(cc.max_step_height),
                min_width: CharacterLength::Absolute(0.05),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: cc.max_slope_angle,
            min_slope_slide_angle: cc.max_slope_angle * 0.6,
            snap_to_ground: Some(CharacterLength::Absolute(0.1)),
            ..Default::default()
        };

        // collider を取得
        let collider = match physics.colliders.get(collider_handle) {
            Some(c) => c,
            None => continue,
        };
        let collider_shape = collider.shared_shape().clone();
        let collider_pos = *collider.position();

        let movement = controller.move_shape(
            dt,
            &physics.bodies,
            &physics.colliders,
            &physics.query_pipeline,
            collider_shape.as_ref(),
            &collider_pos,
            desired,
            QueryFilter::default().exclude_collider(collider_handle),
            |_| {},
        );

        let translation = movement.translation;
        cc.grounded = movement.grounded;
        if movement.grounded {
            cc.vertical_velocity = 0.0;
        }
        drop(cc);

        // body の位置を更新
        if let Ok(handle_ref) = world.get::<&PhysicsHandle>(entity)
            && let Some(body) = physics.bodies.get_mut(handle_ref.body)
        {
            let cur_pos = *body.translation();
            let new_pos = cur_pos + translation;
            body.set_next_kinematic_translation(new_pos);
        }

        // ECS Transform を即時更新 (キネマティック)
        if let Ok(mut t) = world.get::<&mut Transform>(entity) {
            t.translation += Vec3::new(translation.x, translation.y, translation.z);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_character_controller() {
        let cc = CharacterController::default();
        assert!((cc.max_step_height - 0.3).abs() < 1e-5);
        assert!(!cc.grounded);
        assert!(cc.jump_speed > 0.0);
    }

    #[test]
    fn test_max_slope_angle_default() {
        let cc = CharacterController::default();
        assert!(cc.max_slope_angle > 0.0 && cc.max_slope_angle < std::f32::consts::PI);
    }
}
