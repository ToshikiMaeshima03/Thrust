//! 車両物理 (Round 8)
//!
//! rapier3d の `DynamicRayCastVehicleController` のラッパー。
//! シャシー (RigidBody) + 4 輪サスペンション + ステアリング + エンジン.

use glam::Vec3;
use nalgebra::{Point3, Vector3};
use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::prelude::*;

use crate::physics::PhysicsHandle;
use crate::physics::PhysicsWorld;

/// 車両 ECS コンポーネント
pub struct Vehicle {
    /// rapier の車両コントローラ (lazy 初期化)
    pub controller: Option<DynamicRayCastVehicleController>,
    /// シャシー RigidBody (PhysicsHandle 経由で取得)
    pub initialized: bool,
    /// 各輪の接続点 (chassis local)
    pub wheel_positions: [Vec3; 4],
    /// サスペンション長 (m)
    pub suspension_rest_length: f32,
    /// タイヤ半径 (m)
    pub wheel_radius: f32,
    /// エンジン力 (各輪に適用される N)
    pub engine_force: f32,
    /// ブレーキ力 (N)
    pub brake_force: f32,
    /// ステアリング角 (rad、フロントタイヤ)
    pub steering: f32,
}

impl Default for Vehicle {
    fn default() -> Self {
        Self {
            controller: None,
            initialized: false,
            wheel_positions: [
                Vec3::new(0.8, -0.3, 1.2),   // FL
                Vec3::new(-0.8, -0.3, 1.2),  // FR
                Vec3::new(0.8, -0.3, -1.2),  // RL
                Vec3::new(-0.8, -0.3, -1.2), // RR
            ],
            suspension_rest_length: 0.4,
            wheel_radius: 0.3,
            engine_force: 0.0,
            brake_force: 0.0,
            steering: 0.0,
        }
    }
}

impl Vehicle {
    /// 車両コントローラと 4 輪を初期化する
    pub fn ensure_initialized(&mut self, chassis: RigidBodyHandle) {
        if self.initialized {
            return;
        }
        let mut ctrl = DynamicRayCastVehicleController::new(chassis);
        let tuning = WheelTuning::default();
        for pos in &self.wheel_positions {
            ctrl.add_wheel(
                Point3::new(pos.x, pos.y, pos.z),
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(-1.0, 0.0, 0.0),
                self.suspension_rest_length,
                self.wheel_radius,
                &tuning,
            );
        }
        self.controller = Some(ctrl);
        self.initialized = true;
    }

    /// ステアリング/エンジン/ブレーキを各輪に適用
    pub fn apply_inputs(&mut self) {
        let Some(ctrl) = self.controller.as_mut() else {
            return;
        };
        let wheels = ctrl.wheels_mut();
        if wheels.len() >= 2 {
            wheels[0].steering = self.steering;
            wheels[1].steering = self.steering;
        }
        if wheels.len() >= 4 {
            wheels[2].engine_force = self.engine_force;
            wheels[3].engine_force = self.engine_force;
        }
        for w in wheels.iter_mut() {
            w.brake = self.brake_force;
        }
    }
}

/// 車両物理の初期化と更新を行うシステム
pub fn vehicle_system(world: &mut hecs::World, physics: &mut PhysicsWorld, dt: f32) {
    // (entity, body handle) を列挙
    let pending: Vec<(hecs::Entity, RigidBodyHandle)> = world
        .query::<(hecs::Entity, &Vehicle, &PhysicsHandle)>()
        .iter()
        .map(|(e, _, h)| (e, h.body))
        .collect();

    for (entity, body) in pending {
        let Ok(mut v) = world.get::<&mut Vehicle>(entity) else {
            continue;
        };
        v.ensure_initialized(body);
        v.apply_inputs();
        if let Some(ctrl) = v.controller.as_mut() {
            ctrl.update_vehicle(
                dt,
                &mut physics.bodies,
                &physics.colliders,
                &physics.query_pipeline,
                QueryFilter::default(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_vehicle() {
        let v = Vehicle::default();
        assert!(!v.initialized);
        assert_eq!(v.wheel_positions.len(), 4);
        assert!(v.wheel_radius > 0.0);
    }

    #[test]
    fn test_default_inputs_zero() {
        let v = Vehicle::default();
        assert_eq!(v.engine_force, 0.0);
        assert_eq!(v.brake_force, 0.0);
        assert_eq!(v.steering, 0.0);
    }
}
