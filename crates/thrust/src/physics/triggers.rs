//! トリガーボリューム + ゲームイベント (Round 6)
//!
//! UE 風の Trigger Volume: エンティティ A が B の領域に入った/出た/留まっている状態を検出し、
//! `TriggerEnter` / `TriggerStay` / `TriggerExit` イベントを発行する。
//!
//! 既存の collision_system が発行する `CollisionEvent` を解釈してトリガーイベントを生成する。
//! 毎フレームの overlap 集合を比較して Enter/Exit を検出する。

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::event::Events;
use crate::physics::collider::{Collider, CollisionEvent};

/// トリガー Enter イベント (Volume, Other)
#[derive(Debug, Clone, Copy)]
pub struct TriggerEnter {
    pub volume: Entity,
    pub other: Entity,
}

/// トリガー Stay イベント
#[derive(Debug, Clone, Copy)]
pub struct TriggerStay {
    pub volume: Entity,
    pub other: Entity,
}

/// トリガー Exit イベント
#[derive(Debug, Clone, Copy)]
pub struct TriggerExit {
    pub volume: Entity,
    pub other: Entity,
}

/// トリガーボリュームコンポーネント
///
/// 前フレームの overlap 集合を保持し、差分から Enter/Exit を判定する。
#[derive(Default)]
pub struct TriggerVolume {
    /// 前フレームに重なっていたエンティティ
    pub previous_overlaps: HashSet<Entity>,
}

impl TriggerVolume {
    pub fn new() -> Self {
        Self::default()
    }
}

/// トリガーシステム: collision_system 出力から Enter/Stay/Exit イベントを生成する
///
/// `collision_system` より**後に**実行する必要がある。
pub fn trigger_system(world: &mut World, events: &mut Events) {
    // 今フレームの全 collision を volume ごとに集計
    let collisions: Vec<CollisionEvent> = events.read::<CollisionEvent>().to_vec();

    // volume entity ごとの current overlaps を構築
    let mut current: std::collections::HashMap<Entity, HashSet<Entity>> =
        std::collections::HashMap::new();

    for col in &collisions {
        // 両方が is_trigger かチェック
        let a_trigger = world
            .get::<&Collider>(col.entity_a)
            .map(|c| c.is_trigger)
            .unwrap_or(false);
        let b_trigger = world
            .get::<&Collider>(col.entity_b)
            .map(|c| c.is_trigger)
            .unwrap_or(false);

        if a_trigger {
            current
                .entry(col.entity_a)
                .or_default()
                .insert(col.entity_b);
        }
        if b_trigger {
            current
                .entry(col.entity_b)
                .or_default()
                .insert(col.entity_a);
        }
    }

    // TriggerVolume を持つ全エンティティを処理
    let volume_entities: Vec<Entity> = world
        .query::<(Entity, &TriggerVolume)>()
        .iter()
        .map(|(e, _tv)| e)
        .collect();

    let mut new_events: Vec<(
        Option<TriggerEnter>,
        Option<TriggerStay>,
        Option<TriggerExit>,
    )> = Vec::new();

    for volume in volume_entities {
        let prev = world
            .get::<&TriggerVolume>(volume)
            .map(|tv| tv.previous_overlaps.clone())
            .unwrap_or_default();

        let empty = HashSet::new();
        let cur = current.get(&volume).unwrap_or(&empty);

        // Enter: current - previous
        for &e in cur.iter() {
            if !prev.contains(&e) {
                new_events.push((Some(TriggerEnter { volume, other: e }), None, None));
            } else {
                new_events.push((None, Some(TriggerStay { volume, other: e }), None));
            }
        }
        // Exit: previous - current
        for &e in prev.iter() {
            if !cur.contains(&e) {
                new_events.push((None, None, Some(TriggerExit { volume, other: e })));
            }
        }

        // 前フレーム集合を更新
        if let Ok(mut tv) = world.get::<&mut TriggerVolume>(volume) {
            tv.previous_overlaps = cur.clone();
        }
    }

    for (enter, stay, exit) in new_events {
        if let Some(e) = enter {
            events.send(e);
        }
        if let Some(s) = stay {
            events.send(s);
        }
        if let Some(x) = exit {
            events.send(x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Aabb;
    use crate::physics::collider::{Collider, ColliderShape};
    use crate::scene::transform::Transform;
    use glam::Vec3;

    fn setup_trigger_world() -> (World, Events) {
        let mut world = World::new();
        let _volume = world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: true,
            },
            TriggerVolume::new(),
        ));
        (world, Events::new())
    }

    #[test]
    fn test_trigger_enter_detected() {
        let (mut world, mut events) = setup_trigger_world();

        // volume を見つけて overlap 対象を spawn
        let volume = world.iter().next().unwrap().entity();
        let other = world.spawn((
            Transform::from_translation(Vec3::ZERO),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5))),
                is_trigger: false,
            },
        ));

        // collision_system がこれらを重なりとして検出したと想定
        events.send(CollisionEvent {
            entity_a: volume,
            entity_b: other,
        });
        trigger_system(&mut world, &mut events);

        let enters = events.read::<TriggerEnter>();
        assert_eq!(enters.len(), 1);
        assert_eq!(enters[0].other, other);
    }

    #[test]
    fn test_trigger_stay_on_second_frame() {
        let (mut world, mut events) = setup_trigger_world();
        let volume = world.iter().next().unwrap().entity();
        let other = world.spawn((
            Transform::default(),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5))),
                is_trigger: false,
            },
        ));

        // フレーム 1: Enter
        events.send(CollisionEvent {
            entity_a: volume,
            entity_b: other,
        });
        trigger_system(&mut world, &mut events);
        events.clear();

        // フレーム 2: Stay
        events.send(CollisionEvent {
            entity_a: volume,
            entity_b: other,
        });
        trigger_system(&mut world, &mut events);

        let stays = events.read::<TriggerStay>();
        assert_eq!(stays.len(), 1);
        let enters = events.read::<TriggerEnter>();
        assert_eq!(enters.len(), 0);
    }

    #[test]
    fn test_trigger_exit_when_collision_stops() {
        let (mut world, mut events) = setup_trigger_world();
        let volume = world.iter().next().unwrap().entity();
        let other = world.spawn((
            Transform::default(),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5))),
                is_trigger: false,
            },
        ));

        // フレーム 1: Enter
        events.send(CollisionEvent {
            entity_a: volume,
            entity_b: other,
        });
        trigger_system(&mut world, &mut events);
        events.clear();

        // フレーム 2: 衝突なし → Exit
        trigger_system(&mut world, &mut events);

        let exits = events.read::<TriggerExit>();
        assert_eq!(exits.len(), 1);
        assert_eq!(exits[0].other, other);
    }

    #[test]
    fn test_non_trigger_not_affected() {
        let mut world = World::new();
        let mut events = Events::new();
        let volume = world.spawn((
            Transform::default(),
            Collider {
                shape: ColliderShape::Aabb(Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))),
                is_trigger: false, // ← トリガーではない
            },
            TriggerVolume::new(),
        ));
        let other = world.spawn((Transform::default(),));
        events.send(CollisionEvent {
            entity_a: volume,
            entity_b: other,
        });
        trigger_system(&mut world, &mut events);

        // is_trigger=false なので TriggerVolume があっても発行されない
        let enters = events.read::<TriggerEnter>();
        assert_eq!(enters.len(), 0);
    }
}
