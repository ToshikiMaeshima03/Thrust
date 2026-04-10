//! リプリケーション (Round 9)
//!
//! ECS の Transform を NetworkSnapshot に変換するヘルパー、および
//! snapshot を ECS に適用するヘルパー。
//!
//! 簡易版: NetworkId コンポーネントが付いている全エンティティを同期する。

use hecs::World;

use crate::network::protocol::{ServerSnapshot, SnapshotEntity};
use crate::scene::transform::Transform;

/// レプリケーションモード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// サーバーのみが書き込み、クライアントは受信専用
    ServerAuthoritative,
    /// クライアント側で予測 + サーバーが補正
    ClientPredicted,
}

/// ネットワーク ID コンポーネント (ECS)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkId(pub u64);

/// 全 NetworkId + Transform を ServerSnapshot に変換する
pub fn replicate_transforms(world: &World, tick: u64, timestamp: f64) -> ServerSnapshot {
    let mut entities = Vec::new();
    for (id, t) in world.query::<(&NetworkId, &Transform)>().iter() {
        entities.push(SnapshotEntity {
            network_id: id.0,
            position: t.translation.to_array(),
            rotation: t.rotation.to_array(),
            scale: t.scale.to_array(),
        });
    }
    ServerSnapshot {
        tick,
        timestamp,
        entities,
    }
}

/// snapshot を世界に適用する (補間なし、即時上書き)
pub fn apply_snapshot(world: &mut World, snapshot: &ServerSnapshot) {
    use std::collections::HashMap;
    let by_id: HashMap<u64, &SnapshotEntity> = snapshot
        .entities
        .iter()
        .map(|e| (e.network_id, e))
        .collect();

    let to_update: Vec<(hecs::Entity, u64)> = world
        .query::<(hecs::Entity, &NetworkId)>()
        .iter()
        .map(|(e, id)| (e, id.0))
        .collect();

    for (entity, id) in to_update {
        let Some(snap) = by_id.get(&id) else {
            continue;
        };
        if let Ok(mut t) = world.get::<&mut Transform>(entity) {
            t.translation = glam::Vec3::from(snap.position);
            t.rotation = glam::Quat::from_array(snap.rotation);
            t.scale = glam::Vec3::from(snap.scale);
        }
    }
}

/// クライアント補間: 前後 2 つの snapshot 間で alpha (0..1) で線形補間
pub fn interpolate_snapshots(
    world: &mut World,
    prev: &ServerSnapshot,
    next: &ServerSnapshot,
    alpha: f32,
) {
    use std::collections::HashMap;
    let prev_by_id: HashMap<u64, &SnapshotEntity> =
        prev.entities.iter().map(|e| (e.network_id, e)).collect();
    let next_by_id: HashMap<u64, &SnapshotEntity> =
        next.entities.iter().map(|e| (e.network_id, e)).collect();

    let to_update: Vec<(hecs::Entity, u64)> = world
        .query::<(hecs::Entity, &NetworkId)>()
        .iter()
        .map(|(e, id)| (e, id.0))
        .collect();

    let alpha = alpha.clamp(0.0, 1.0);
    for (entity, id) in to_update {
        let (Some(p), Some(n)) = (prev_by_id.get(&id), next_by_id.get(&id)) else {
            continue;
        };
        if let Ok(mut t) = world.get::<&mut Transform>(entity) {
            let pp = glam::Vec3::from(p.position);
            let np = glam::Vec3::from(n.position);
            t.translation = pp.lerp(np, alpha);
            let pr = glam::Quat::from_array(p.rotation);
            let nr = glam::Quat::from_array(n.rotation);
            t.rotation = pr.slerp(nr, alpha);
            let ps = glam::Vec3::from(p.scale);
            let ns = glam::Vec3::from(n.scale);
            t.scale = ps.lerp(ns, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_mode() {
        assert_ne!(
            ReplicationMode::ServerAuthoritative,
            ReplicationMode::ClientPredicted
        );
    }

    #[test]
    fn test_replicate_empty_world() {
        let world = World::new();
        let snap = replicate_transforms(&world, 1, 0.0);
        assert!(snap.entities.is_empty());
    }

    #[test]
    fn test_replicate_one_entity() {
        let mut world = World::new();
        world.spawn((
            NetworkId(42),
            Transform {
                translation: glam::Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
        ));
        let snap = replicate_transforms(&world, 1, 0.0);
        assert_eq!(snap.entities.len(), 1);
        assert_eq!(snap.entities[0].network_id, 42);
        assert_eq!(snap.entities[0].position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_apply_snapshot() {
        let mut world = World::new();
        let entity = world.spawn((NetworkId(7), Transform::default()));
        let snap = ServerSnapshot {
            tick: 1,
            timestamp: 0.0,
            entities: vec![SnapshotEntity {
                network_id: 7,
                position: [10.0, 20.0, 30.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [2.0; 3],
            }],
        };
        apply_snapshot(&mut world, &snap);
        let t = world.get::<&Transform>(entity).unwrap();
        assert_eq!(t.translation, glam::Vec3::new(10.0, 20.0, 30.0));
        assert_eq!(t.scale, glam::Vec3::splat(2.0));
    }

    #[test]
    fn test_interpolate_snapshots() {
        let mut world = World::new();
        let entity = world.spawn((NetworkId(1), Transform::default()));
        let prev = ServerSnapshot {
            tick: 1,
            timestamp: 0.0,
            entities: vec![SnapshotEntity {
                network_id: 1,
                position: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            }],
        };
        let next = ServerSnapshot {
            tick: 2,
            timestamp: 0.1,
            entities: vec![SnapshotEntity {
                network_id: 1,
                position: [10.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            }],
        };
        interpolate_snapshots(&mut world, &prev, &next, 0.5);
        let t = world.get::<&Transform>(entity).unwrap();
        assert!((t.translation.x - 5.0).abs() < 1e-5);
    }
}
