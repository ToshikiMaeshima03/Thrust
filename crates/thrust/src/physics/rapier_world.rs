//! rapier3d ベースの物理ワールド (Round 4)
//!
//! 既存の `Collider` / `Velocity` コンポーネントは互換シムとして
//! `physics_init_system` が rapier ハンドルを生成することで動作する。

use glam::Vec3;
use rapier3d::prelude::*;

/// 物理世界全体を保持するリソース
pub struct PhysicsWorld {
    pub gravity: Vector<Real>,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub joints_impulse: ImpulseJointSet,
    pub joints_multibody: MultibodyJointSet,
    pub island_manager: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,
    pub physics_pipeline: PhysicsPipeline,
    pub integration_parameters: IntegrationParameters,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    /// 重力 (0, -9.81, 0) でデフォルトの物理世界を構築する
    pub fn new() -> Self {
        Self {
            gravity: vector![0.0, -9.81, 0.0],
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            joints_impulse: ImpulseJointSet::new(),
            joints_multibody: MultibodyJointSet::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            physics_pipeline: PhysicsPipeline::new(),
            integration_parameters: IntegrationParameters::default(),
        }
    }

    /// 1 ステップ物理シミュレーションを進める
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        let physics_hooks = ();
        let event_handler = ();
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints_impulse,
            &mut self.joints_multibody,
            &mut self.ccd_solver,
            None,
            &physics_hooks,
            &event_handler,
        );
        self.query_pipeline.update(&self.colliders);
    }
}

/// 剛体タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBodyType {
    /// 動的: 力・重力・衝突に応答
    Dynamic,
    /// 静的: 動かない
    Fixed,
    /// キネマティック: ユーザーが Transform で制御
    KinematicPositionBased,
}

impl From<RigidBodyType> for rapier3d::dynamics::RigidBodyType {
    fn from(t: RigidBodyType) -> Self {
        match t {
            RigidBodyType::Dynamic => rapier3d::dynamics::RigidBodyType::Dynamic,
            RigidBodyType::Fixed => rapier3d::dynamics::RigidBodyType::Fixed,
            RigidBodyType::KinematicPositionBased => {
                rapier3d::dynamics::RigidBodyType::KinematicPositionBased
            }
        }
    }
}

/// rapier ハンドルを保持するコンポーネント (`physics_init_system` が挿入)
pub struct PhysicsHandle {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
}

/// 剛体定義コンポーネント (Round 4)
///
/// `RigidBody` をエンティティに付けると `physics_init_system` が次フレームに
/// rapier ハンドルを生成し `PhysicsHandle` を挿入する。
pub struct RigidBody {
    pub body_type: RigidBodyType,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub initial_velocity: Vec3,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            body_type: RigidBodyType::Dynamic,
            linear_damping: 0.0,
            angular_damping: 0.0,
            initial_velocity: Vec3::ZERO,
        }
    }
}

impl RigidBody {
    pub fn dynamic() -> Self {
        Self {
            body_type: RigidBodyType::Dynamic,
            ..Default::default()
        }
    }

    pub fn fixed() -> Self {
        Self {
            body_type: RigidBodyType::Fixed,
            ..Default::default()
        }
    }

    pub fn kinematic() -> Self {
        Self {
            body_type: RigidBodyType::KinematicPositionBased,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_world_new() {
        let w = PhysicsWorld::new();
        assert!((w.gravity.y + 9.81).abs() < 1e-5);
        assert_eq!(w.bodies.len(), 0);
    }

    #[test]
    fn test_dynamic_body_falls_under_gravity() {
        let mut w = PhysicsWorld::new();
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 10.0, 0.0])
            .build();
        let handle = w.bodies.insert(body);
        let collider = ColliderBuilder::ball(0.5).build();
        w.colliders
            .insert_with_parent(collider, handle, &mut w.bodies);

        // 1 秒シミュレート (60 ステップ)
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }

        let pos = w.bodies[handle].translation();
        // 自由落下: 約 -4.9 + 10 = 5.1 付近
        assert!(
            pos.y < 9.0 && pos.y > 4.0,
            "1 秒後の y は約 5.1 付近、実際: {}",
            pos.y
        );
    }

    #[test]
    fn test_kinematic_body_doesnt_fall() {
        let mut w = PhysicsWorld::new();
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(vector![0.0, 10.0, 0.0])
            .build();
        let handle = w.bodies.insert(body);

        for _ in 0..30 {
            w.step(1.0 / 60.0);
        }

        let pos = w.bodies[handle].translation();
        assert!((pos.y - 10.0).abs() < 1e-3, "kinematic は動かないべき");
    }

    #[test]
    fn test_rigid_body_default() {
        let rb = RigidBody::default();
        assert_eq!(rb.body_type, RigidBodyType::Dynamic);
    }

    #[test]
    fn test_rigid_body_helpers() {
        assert_eq!(RigidBody::fixed().body_type, RigidBodyType::Fixed);
        assert_eq!(
            RigidBody::kinematic().body_type,
            RigidBodyType::KinematicPositionBased
        );
    }
}
