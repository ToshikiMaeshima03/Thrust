//! ビヘイビアツリー (Round 6)
//!
//! UE 風の AI 判断木:
//! - **Composite**: Sequence (全子が成功するまで進む) / Selector (成功した子で終わる)
//! - **Decorator**: Inverter / Repeater / UntilSuccess
//! - **Leaf**: Action (クロージャ) / Condition (bool 関数)
//!
//! 各ノードは `tick` で `Status` を返す。
//! `BehaviorTree` コンポーネントをエンティティに付けて `behavior_tree_system` で駆動する。

use hecs::{Entity, World};
use std::collections::HashMap;

use crate::ecs::resources::Resources;

/// ノードの実行結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 実行成功
    Success,
    /// 実行失敗
    Failure,
    /// 実行中 (次フレームも継続)
    Running,
}

/// ビヘイビアノード
pub enum BtNode {
    /// Sequence: 子を順に実行、Failure か Running で止まる
    Sequence(Vec<BtNode>),
    /// Selector: 子を順に実行、Success か Running で止まる
    Selector(Vec<BtNode>),
    /// Inverter: 子の Success ⇔ Failure を反転
    Inverter(Box<BtNode>),
    /// Repeater: 子を N 回繰り返す (0 なら無限)
    Repeater { child: Box<BtNode>, count: u32 },
    /// UntilSuccess: Success になるまで繰り返す (最大試行数)
    UntilSuccess { child: Box<BtNode>, max_tries: u32 },
    /// アクション (クロージャ)
    Action(Box<dyn Fn(&mut BtContext) -> Status + Send + Sync>),
    /// 条件判定 (bool → Success/Failure)
    Condition(Box<dyn Fn(&mut BtContext) -> bool + Send + Sync>),
    /// 成功を常に返す
    AlwaysSuccess,
    /// 失敗を常に返す
    AlwaysFailure,
}

impl std::fmt::Debug for BtNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BtNode::Sequence(c) => write!(f, "Sequence({})", c.len()),
            BtNode::Selector(c) => write!(f, "Selector({})", c.len()),
            BtNode::Inverter(_) => write!(f, "Inverter"),
            BtNode::Repeater { count, .. } => write!(f, "Repeater({count})"),
            BtNode::UntilSuccess { max_tries, .. } => write!(f, "UntilSuccess({max_tries})"),
            BtNode::Action(_) => write!(f, "Action"),
            BtNode::Condition(_) => write!(f, "Condition"),
            BtNode::AlwaysSuccess => write!(f, "AlwaysSuccess"),
            BtNode::AlwaysFailure => write!(f, "AlwaysFailure"),
        }
    }
}

/// アクション/条件から参照できるコンテキスト
///
/// `blackboard` はキー/値ストア、`entity` は自エンティティ、
/// `world_ptr` と `res_ptr` は一時的な unsafe ポインタ (BT 実行中のみ有効)。
///
/// # Safety
/// `world_ptr` と `res_ptr` は `behavior_tree_system` の実行中のみ生存する。
/// コンテキスト外へ持ち出してはならない。
pub struct BtContext<'a> {
    pub entity: Entity,
    pub blackboard: &'a mut Blackboard,
    pub dt: f32,
}

/// キー/値ストア (BT 内の状態共有)
#[derive(Default)]
pub struct Blackboard {
    pub floats: HashMap<String, f32>,
    pub bools: HashMap<String, bool>,
    pub ints: HashMap<String, i32>,
    pub vecs: HashMap<String, glam::Vec3>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_float(&mut self, key: impl Into<String>, value: f32) {
        self.floats.insert(key.into(), value);
    }
    pub fn get_float(&self, key: &str) -> f32 {
        self.floats.get(key).copied().unwrap_or(0.0)
    }
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.bools.insert(key.into(), value);
    }
    pub fn get_bool(&self, key: &str) -> bool {
        self.bools.get(key).copied().unwrap_or(false)
    }
    pub fn set_int(&mut self, key: impl Into<String>, value: i32) {
        self.ints.insert(key.into(), value);
    }
    pub fn get_int(&self, key: &str) -> i32 {
        self.ints.get(key).copied().unwrap_or(0)
    }
    pub fn set_vec(&mut self, key: impl Into<String>, value: glam::Vec3) {
        self.vecs.insert(key.into(), value);
    }
    pub fn get_vec(&self, key: &str) -> glam::Vec3 {
        self.vecs.get(key).copied().unwrap_or(glam::Vec3::ZERO)
    }
}

impl BtNode {
    /// このノードを実行する
    pub fn tick(&self, ctx: &mut BtContext) -> Status {
        match self {
            BtNode::Sequence(children) => {
                for child in children {
                    match child.tick(ctx) {
                        Status::Success => continue,
                        Status::Failure => return Status::Failure,
                        Status::Running => return Status::Running,
                    }
                }
                Status::Success
            }
            BtNode::Selector(children) => {
                for child in children {
                    match child.tick(ctx) {
                        Status::Success => return Status::Success,
                        Status::Failure => continue,
                        Status::Running => return Status::Running,
                    }
                }
                Status::Failure
            }
            BtNode::Inverter(child) => match child.tick(ctx) {
                Status::Success => Status::Failure,
                Status::Failure => Status::Success,
                Status::Running => Status::Running,
            },
            BtNode::Repeater { child, count } => {
                if *count == 0 {
                    // 無限ループは Running を返して 1 回だけ子を tick する
                    let _ = child.tick(ctx);
                    return Status::Running;
                }
                for _ in 0..*count {
                    match child.tick(ctx) {
                        Status::Running => return Status::Running,
                        _ => continue,
                    }
                }
                Status::Success
            }
            BtNode::UntilSuccess { child, max_tries } => {
                for _ in 0..*max_tries {
                    match child.tick(ctx) {
                        Status::Success => return Status::Success,
                        Status::Running => return Status::Running,
                        Status::Failure => continue,
                    }
                }
                Status::Failure
            }
            BtNode::Action(f) => f(ctx),
            BtNode::Condition(f) => {
                if f(ctx) {
                    Status::Success
                } else {
                    Status::Failure
                }
            }
            BtNode::AlwaysSuccess => Status::Success,
            BtNode::AlwaysFailure => Status::Failure,
        }
    }
}

/// ヘルパー: アクションノード作成
pub fn action<F: Fn(&mut BtContext) -> Status + Send + Sync + 'static>(f: F) -> BtNode {
    BtNode::Action(Box::new(f))
}

/// ヘルパー: 条件ノード作成
pub fn condition<F: Fn(&mut BtContext) -> bool + Send + Sync + 'static>(f: F) -> BtNode {
    BtNode::Condition(Box::new(f))
}

/// ヘルパー: Sequence ショートカット
pub fn sequence(children: Vec<BtNode>) -> BtNode {
    BtNode::Sequence(children)
}

/// ヘルパー: Selector ショートカット
pub fn selector(children: Vec<BtNode>) -> BtNode {
    BtNode::Selector(children)
}

/// ビヘイビアツリーコンポーネント
pub struct BehaviorTree {
    pub root: BtNode,
    pub blackboard: Blackboard,
    pub last_status: Status,
}

impl BehaviorTree {
    pub fn new(root: BtNode) -> Self {
        Self {
            root,
            blackboard: Blackboard::new(),
            last_status: Status::Running,
        }
    }
}

/// 各エンティティのビヘイビアツリーを 1 tick 進めるシステム
///
/// 注意: BT のアクション内部から world/res にアクセスする必要がある場合は、
/// blackboard に情報を詰めてから呼ぶか、より高度な実装 (BtContext を拡張) が必要。
/// この実装は blackboard と dt のみのシンプル版。
pub fn behavior_tree_system(world: &mut World, _res: &mut Resources, dt: f32) {
    // Entity ID を先に収集 (BehaviorTree の借用を避けるため)
    let entities: Vec<Entity> = world
        .query::<(Entity, &BehaviorTree)>()
        .iter()
        .map(|(e, _bt)| e)
        .collect();

    for entity in entities {
        // BT を一時的に取り出して実行 (自エンティティを安全に扱える)
        let mut bt = match world.remove_one::<BehaviorTree>(entity) {
            Ok(bt) => bt,
            Err(_) => continue,
        };
        let mut ctx = BtContext {
            entity,
            blackboard: &mut bt.blackboard,
            dt,
        };
        let status = bt.root.tick(&mut ctx);
        bt.last_status = status;
        let _ = world.insert_one(entity, bt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entity() -> Entity {
        Entity::from_bits(0x100000001).unwrap()
    }

    fn tick_with(root: BtNode, bb: &mut Blackboard) -> Status {
        let mut ctx = BtContext {
            entity: dummy_entity(),
            blackboard: bb,
            dt: 0.016,
        };
        root.tick(&mut ctx)
    }

    #[test]
    fn test_always_success() {
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(BtNode::AlwaysSuccess, &mut bb), Status::Success);
    }

    #[test]
    fn test_always_failure() {
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(BtNode::AlwaysFailure, &mut bb), Status::Failure);
    }

    #[test]
    fn test_sequence_all_success() {
        let tree = sequence(vec![BtNode::AlwaysSuccess, BtNode::AlwaysSuccess]);
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Success);
    }

    #[test]
    fn test_sequence_short_circuit() {
        let tree = sequence(vec![BtNode::AlwaysSuccess, BtNode::AlwaysFailure]);
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Failure);
    }

    #[test]
    fn test_selector_first_success() {
        let tree = selector(vec![BtNode::AlwaysFailure, BtNode::AlwaysSuccess]);
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Success);
    }

    #[test]
    fn test_selector_all_failure() {
        let tree = selector(vec![BtNode::AlwaysFailure, BtNode::AlwaysFailure]);
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Failure);
    }

    #[test]
    fn test_inverter() {
        let tree = BtNode::Inverter(Box::new(BtNode::AlwaysSuccess));
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Failure);
    }

    #[test]
    fn test_condition_blackboard() {
        let tree = condition(|ctx| ctx.blackboard.get_float("health") > 0.5);
        let mut bb = Blackboard::new();
        bb.set_float("health", 1.0);
        assert_eq!(tick_with(tree, &mut bb), Status::Success);
    }

    #[test]
    fn test_action_modifies_blackboard() {
        let tree = action(|ctx| {
            ctx.blackboard
                .set_int("counter", ctx.blackboard.get_int("counter") + 1);
            Status::Success
        });
        let mut bb = Blackboard::new();
        let _ = tick_with(tree, &mut bb);
        assert_eq!(bb.get_int("counter"), 1);
    }

    #[test]
    fn test_sequence_with_condition_and_action() {
        let tree = sequence(vec![
            condition(|ctx| ctx.blackboard.get_bool("enemy_visible")),
            action(|ctx| {
                ctx.blackboard.set_bool("attacking", true);
                Status::Success
            }),
        ]);
        let mut bb = Blackboard::new();
        bb.set_bool("enemy_visible", true);
        assert_eq!(tick_with(tree, &mut bb), Status::Success);
        assert!(bb.get_bool("attacking"));
    }

    #[test]
    fn test_sequence_condition_false() {
        let tree = sequence(vec![
            condition(|ctx| ctx.blackboard.get_bool("enemy_visible")),
            action(|_ctx| Status::Success),
        ]);
        let mut bb = Blackboard::new();
        bb.set_bool("enemy_visible", false);
        assert_eq!(tick_with(tree, &mut bb), Status::Failure);
    }

    #[test]
    fn test_repeater() {
        let tree = BtNode::Repeater {
            child: Box::new(BtNode::AlwaysSuccess),
            count: 3,
        };
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Success);
    }

    #[test]
    fn test_blackboard_vec() {
        let mut bb = Blackboard::new();
        bb.set_vec("target", glam::Vec3::new(1.0, 2.0, 3.0));
        assert!((bb.get_vec("target") - glam::Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        assert_eq!(bb.get_vec("unknown"), glam::Vec3::ZERO);
    }

    #[test]
    fn test_until_success() {
        // 初回は失敗、2 回目は成功するカウンタ
        // ここでは単純に AlwaysFailure で max_tries = 3 → Failure
        let tree = BtNode::UntilSuccess {
            child: Box::new(BtNode::AlwaysFailure),
            max_tries: 3,
        };
        let mut bb = Blackboard::new();
        assert_eq!(tick_with(tree, &mut bb), Status::Failure);
    }
}
