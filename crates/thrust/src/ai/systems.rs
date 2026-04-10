//! AI システム: AgentMover (Round 5)
//!
//! `AgentMover` コンポーネントを持つエンティティが waypoint に向かって移動する。
//! `find_path` でパスを設定すると、`agent_movement_system` が毎フレーム Transform を進める。

use glam::Vec3;
use hecs::World;

use crate::scene::transform::Transform;

/// 移動エージェント
pub struct AgentMover {
    /// 現在の経路 (ワールド座標)
    pub path: Vec<Vec3>,
    /// 現在の経路上の次のウェイポイントインデックス
    pub current_waypoint: usize,
    /// 移動速度 (m/s)
    pub speed: f32,
    /// ウェイポイント到達判定の半径
    pub waypoint_radius: f32,
}

impl AgentMover {
    pub fn new(speed: f32) -> Self {
        Self {
            path: Vec::new(),
            current_waypoint: 0,
            speed,
            waypoint_radius: 0.3,
        }
    }

    /// 新しい経路を設定する (current_waypoint を 0 にリセット)
    pub fn set_path(&mut self, path: Vec<Vec3>) {
        self.path = path;
        self.current_waypoint = 0;
    }

    /// 経路の終点に到達済みか
    pub fn is_at_destination(&self) -> bool {
        self.path.is_empty() || self.current_waypoint >= self.path.len()
    }

    /// 現在のターゲットウェイポイント
    pub fn current_target(&self) -> Option<Vec3> {
        self.path.get(self.current_waypoint).copied()
    }
}

/// AgentMover を持つ全エンティティを移動させる
pub fn agent_movement_system(world: &mut World, dt: f32) {
    for (transform, agent) in world.query_mut::<(&mut Transform, &mut AgentMover)>() {
        if agent.is_at_destination() {
            continue;
        }

        let mut remaining = agent.speed * dt;
        // 1 フレームで複数ウェイポイントを通過可能 (高速エージェント対応)
        while remaining > 0.0 && !agent.is_at_destination() {
            let Some(target) = agent.current_target() else {
                break;
            };
            let to_target = target - transform.translation;
            let to_target_xz = Vec3::new(to_target.x, 0.0, to_target.z);
            let dist_xz = to_target_xz.length();

            if dist_xz < agent.waypoint_radius {
                // 到達 → 次のウェイポイントへ
                agent.current_waypoint += 1;
                continue;
            }

            let dir = to_target_xz / dist_xz.max(1e-5);
            if remaining >= dist_xz {
                // この step でウェイポイントに到達 → snap して次へ
                transform.translation = Vec3::new(target.x, transform.translation.y, target.z);
                remaining -= dist_xz;
                agent.current_waypoint += 1;
            } else {
                transform.translation += dir * remaining;
                remaining = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mover_new() {
        let m = AgentMover::new(5.0);
        assert!((m.speed - 5.0).abs() < 1e-5);
        assert!(m.path.is_empty());
        assert_eq!(m.current_waypoint, 0);
        assert!(m.is_at_destination());
    }

    #[test]
    fn test_agent_mover_set_path() {
        let mut m = AgentMover::new(5.0);
        m.set_path(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ]);
        assert_eq!(m.path.len(), 3);
        assert_eq!(m.current_waypoint, 0);
        assert!(!m.is_at_destination());
    }

    #[test]
    fn test_agent_movement_simple() {
        let mut world = World::new();
        let mut agent = AgentMover::new(2.0);
        agent.set_path(vec![Vec3::new(10.0, 0.0, 0.0)]);
        let entity = world.spawn((Transform::from_translation(Vec3::ZERO), agent));

        // 1 秒進める
        agent_movement_system(&mut world, 1.0);

        let t = world.get::<&Transform>(entity).unwrap();
        // 2 m/s で 1 秒 → x = 2.0
        assert!((t.translation.x - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_agent_arrival() {
        let mut world = World::new();
        let mut agent = AgentMover::new(10.0);
        agent.set_path(vec![Vec3::new(1.0, 0.0, 0.0)]);
        let entity = world.spawn((Transform::from_translation(Vec3::ZERO), agent));

        // 1 秒進める → 1 m 動いて到達
        agent_movement_system(&mut world, 1.0);

        let agent = world.get::<&AgentMover>(entity).unwrap();
        // current_waypoint = 1 で is_at_destination
        assert!(agent.is_at_destination());
    }

    #[test]
    fn test_agent_multi_waypoint() {
        let mut world = World::new();
        let mut agent = AgentMover::new(5.0);
        agent.set_path(vec![Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 2.0)]);
        let entity = world.spawn((Transform::from_translation(Vec3::ZERO), agent));

        for _ in 0..20 {
            agent_movement_system(&mut world, 0.1);
        }

        let t = world.get::<&Transform>(entity).unwrap();
        // 終点 (2, 0, 2) に到達するはず
        assert!((t.translation - Vec3::new(2.0, 0.0, 2.0)).length() < 1.0);
    }

    #[test]
    fn test_agent_no_path() {
        let mut world = World::new();
        let agent = AgentMover::new(5.0);
        let entity = world.spawn((Transform::from_translation(Vec3::ZERO), agent));

        agent_movement_system(&mut world, 1.0);

        let t = world.get::<&Transform>(entity).unwrap();
        // 動かない
        assert!(t.translation.length() < 1e-5);
    }
}
