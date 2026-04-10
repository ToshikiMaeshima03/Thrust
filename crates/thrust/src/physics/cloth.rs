//! 布シミュレーション (Round 8)
//!
//! Verlet 積分 + 距離拘束 (Position-Based Dynamics) によるシンプルな布シミュレーション。
//! グリッド状の頂点配列を構築し、隣接ノード間に距離拘束を張る。
//!
//! 用途: フラッグ、カーテン、マント。リアルタイム性を重視したシンプル実装。

use glam::Vec3;
use hecs::World;

use crate::ecs::resources::Resources;

/// 布の頂点
#[derive(Debug, Clone, Copy)]
pub struct ClothNode {
    pub position: Vec3,
    pub previous: Vec3,
    /// pinned (動かない) フラグ
    pub pinned: bool,
}

impl ClothNode {
    pub fn new(pos: Vec3) -> Self {
        Self {
            position: pos,
            previous: pos,
            pinned: false,
        }
    }

    pub fn pinned_at(pos: Vec3) -> Self {
        Self {
            position: pos,
            previous: pos,
            pinned: true,
        }
    }
}

/// 距離拘束
#[derive(Debug, Clone, Copy)]
pub struct ClothConstraint {
    pub a: usize,
    pub b: usize,
    pub rest_length: f32,
    /// 拘束の硬さ (1.0 = 完全)
    pub stiffness: f32,
}

/// 布全体 (ECS コンポーネント)
pub struct Cloth {
    pub nodes: Vec<ClothNode>,
    pub constraints: Vec<ClothConstraint>,
    /// 重力 (m/s²)
    pub gravity: Vec3,
    /// 風の力 (m/s²)
    pub wind: Vec3,
    /// 拘束反復数 (多いほど剛性高い)
    pub iterations: usize,
    /// 横の頂点数 (グリッド再構築用、optional)
    pub width: usize,
    pub height: usize,
}

impl Cloth {
    /// グリッド状の布を生成する
    ///
    /// `width` × `height` ノード、`spacing` 間隔。
    /// 上端の 2 頂点 (左右) は pinned される。
    pub fn new_grid(width: usize, height: usize, spacing: f32, top_y: f32) -> Self {
        let mut nodes = Vec::with_capacity(width * height);
        let mut constraints = Vec::new();
        let half_w = (width as f32 - 1.0) * spacing * 0.5;

        for j in 0..height {
            for i in 0..width {
                let x = i as f32 * spacing - half_w;
                let y = top_y - j as f32 * spacing;
                let pinned = j == 0 && (i == 0 || i == width - 1);
                let pos = Vec3::new(x, y, 0.0);
                nodes.push(if pinned {
                    ClothNode::pinned_at(pos)
                } else {
                    ClothNode::new(pos)
                });
            }
        }

        // 構造拘束 (横と縦)
        for j in 0..height {
            for i in 0..width {
                let idx = j * width + i;
                if i + 1 < width {
                    let next = idx + 1;
                    constraints.push(ClothConstraint {
                        a: idx,
                        b: next,
                        rest_length: spacing,
                        stiffness: 1.0,
                    });
                }
                if j + 1 < height {
                    let next = idx + width;
                    constraints.push(ClothConstraint {
                        a: idx,
                        b: next,
                        rest_length: spacing,
                        stiffness: 1.0,
                    });
                }
                // 対角拘束 (シア対策)
                if i + 1 < width && j + 1 < height {
                    let diag = spacing * std::f32::consts::SQRT_2;
                    constraints.push(ClothConstraint {
                        a: idx,
                        b: idx + width + 1,
                        rest_length: diag,
                        stiffness: 0.5,
                    });
                    constraints.push(ClothConstraint {
                        a: idx + 1,
                        b: idx + width,
                        rest_length: diag,
                        stiffness: 0.5,
                    });
                }
            }
        }

        Self {
            nodes,
            constraints,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            wind: Vec3::ZERO,
            iterations: 4,
            width,
            height,
        }
    }

    /// 1 ステップ進める
    pub fn step(&mut self, dt: f32) {
        // Verlet 積分
        let damping = 0.99;
        for node in self.nodes.iter_mut() {
            if node.pinned {
                continue;
            }
            let velocity = (node.position - node.previous) * damping;
            node.previous = node.position;
            node.position = node.position + velocity + (self.gravity + self.wind) * dt * dt;
        }

        // 拘束を反復解決
        for _ in 0..self.iterations {
            for c in &self.constraints {
                let a_pos = self.nodes[c.a].position;
                let b_pos = self.nodes[c.b].position;
                let delta = b_pos - a_pos;
                let dist = delta.length().max(1e-5);
                let diff = (dist - c.rest_length) / dist * c.stiffness;
                let correction = delta * diff * 0.5;

                let a_pinned = self.nodes[c.a].pinned;
                let b_pinned = self.nodes[c.b].pinned;
                if !a_pinned {
                    self.nodes[c.a].position += correction;
                }
                if !b_pinned {
                    self.nodes[c.b].position -= correction;
                }
            }
        }
    }
}

/// `Cloth` を持つエンティティを毎フレーム step するシステム
pub fn cloth_system(world: &mut World, _res: &mut Resources, dt: f32) {
    let entities: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Cloth)>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for entity in entities {
        if let Ok(mut cloth) = world.get::<&mut Cloth>(entity) {
            cloth.step(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_creation() {
        let cloth = Cloth::new_grid(5, 5, 0.1, 5.0);
        assert_eq!(cloth.nodes.len(), 25);
        // 構造拘束: 横 (5*4) + 縦 (5*4) + 対角 (4*4*2) = 40 + 32 = 72
        assert_eq!(cloth.constraints.len(), 72);
    }

    #[test]
    fn test_top_corners_pinned() {
        let cloth = Cloth::new_grid(5, 5, 0.1, 5.0);
        assert!(cloth.nodes[0].pinned);
        assert!(cloth.nodes[4].pinned);
        // 中央上は pinned ではない
        assert!(!cloth.nodes[2].pinned);
    }

    #[test]
    fn test_step_moves_unpinned() {
        let mut cloth = Cloth::new_grid(3, 3, 0.1, 5.0);
        let initial_y = cloth.nodes[4].position.y;
        cloth.step(0.016);
        assert!(cloth.nodes[4].position.y < initial_y);
    }

    #[test]
    fn test_pinned_does_not_move() {
        let mut cloth = Cloth::new_grid(3, 3, 0.1, 5.0);
        let pinned_pos = cloth.nodes[0].position;
        for _ in 0..10 {
            cloth.step(0.016);
        }
        assert!((cloth.nodes[0].position - pinned_pos).length() < 1e-5);
    }

    #[test]
    fn test_constraint_keeps_distance() {
        let mut cloth = Cloth::new_grid(3, 3, 0.1, 5.0);
        for _ in 0..50 {
            cloth.step(0.016);
        }
        // 隣接ノード間の距離が rest_length に近い
        let d = (cloth.nodes[0].position - cloth.nodes[1].position).length();
        assert!((d - 0.1).abs() < 0.05, "距離 {d} != 0.1 ± 0.05");
    }
}
