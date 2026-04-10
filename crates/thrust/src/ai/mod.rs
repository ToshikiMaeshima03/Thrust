//! AI システム (Round 5 / Round 6)
//!
//! - グリッドベース navmesh (Round 5)
//! - A* パスファインディング (Round 5)
//! - AgentMover コンポーネント (Transform を waypoint に向けて移動) (Round 5)
//! - ビヘイビアツリー (Round 6)

pub mod behavior_tree;
pub mod navmesh;
pub mod pathfinding;
pub mod systems;

pub use behavior_tree::{
    BehaviorTree, Blackboard, BtContext, BtNode, Status, action, behavior_tree_system, condition,
    selector, sequence,
};
pub use navmesh::{NavCell, NavMesh, NavMeshBuilder};
pub use pathfinding::{find_path, smooth_path};
pub use systems::{AgentMover, agent_movement_system};
