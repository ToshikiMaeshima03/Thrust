//! A* パスファインディング (Round 5)

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use glam::Vec3;

use crate::ai::navmesh::NavMesh;

/// A* 探索ノード (優先度キュー用)
#[derive(Debug, Clone, Copy)]
struct Node {
    pos: (usize, usize),
    f_score: f32,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.f_score == other.f_score
    }
}

impl Eq for Node {}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap は max-heap なので f_score 反転
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(Ordering::Equal)
    }
}

/// マンハッタン距離ヒューリスティック (8 方向対応)
fn heuristic(a: (usize, usize), b: (usize, usize)) -> f32 {
    let dx = (a.0 as f32 - b.0 as f32).abs();
    let dz = (a.1 as f32 - b.1 as f32).abs();
    let min = dx.min(dz);
    let max = dx.max(dz);
    // diagonal: √2 * min + (max - min) * 1
    std::f32::consts::SQRT_2 * min + (max - min)
}

/// A* でパスを探索する
///
/// 戻り値: ワールド座標のウェイポイント列 (start も含む)。
/// 経路が見つからない場合は空の Vec を返す。
pub fn find_path(navmesh: &NavMesh, start_world: Vec3, goal_world: Vec3) -> Vec<Vec3> {
    let Some(start) = navmesh.world_to_grid(start_world) else {
        return Vec::new();
    };
    let Some(goal) = navmesh.world_to_grid(goal_world) else {
        return Vec::new();
    };

    if !navmesh.is_walkable(start.0, start.1) || !navmesh.is_walkable(goal.0, goal.1) {
        return Vec::new();
    }

    let mut open: BinaryHeap<Node> = BinaryHeap::new();
    let mut came_from: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut g_score: HashMap<(usize, usize), f32> = HashMap::new();

    open.push(Node {
        pos: start,
        f_score: heuristic(start, goal),
    });
    g_score.insert(start, 0.0);

    while let Some(current) = open.pop() {
        if current.pos == goal {
            // パスを再構築
            let mut path = Vec::new();
            let mut cur = goal;
            path.push(navmesh.grid_to_world(cur.0, cur.1));
            while let Some(prev) = came_from.get(&cur) {
                cur = *prev;
                path.push(navmesh.grid_to_world(cur.0, cur.1));
            }
            path.reverse();
            return path;
        }

        let current_g = *g_score.get(&current.pos).unwrap_or(&f32::INFINITY);
        for ((nx, nz), cost) in navmesh.neighbors(current.pos.0, current.pos.1) {
            let tentative_g = current_g + cost;
            let neighbor_g = *g_score.get(&(nx, nz)).unwrap_or(&f32::INFINITY);
            if tentative_g < neighbor_g {
                came_from.insert((nx, nz), current.pos);
                g_score.insert((nx, nz), tentative_g);
                let f = tentative_g + heuristic((nx, nz), goal);
                open.push(Node {
                    pos: (nx, nz),
                    f_score: f,
                });
            }
        }
    }

    // 経路なし
    Vec::new()
}

/// パスを単純化する (line-of-sight ベースの string pulling)
///
/// 連続するウェイポイントの間で navmesh 上に障害物がなければ中間点を削除する。
pub fn smooth_path(navmesh: &NavMesh, path: &[Vec3]) -> Vec<Vec3> {
    if path.len() <= 2 {
        return path.to_vec();
    }
    let mut result = Vec::with_capacity(path.len());
    result.push(path[0]);
    let mut anchor = 0;

    while anchor < path.len() - 1 {
        let mut next = anchor + 1;
        // 最も遠い line-of-sight が通る点を探す
        for i in (anchor + 2)..path.len() {
            if line_of_sight(navmesh, path[anchor], path[i]) {
                next = i;
            } else {
                break;
            }
        }
        result.push(path[next]);
        anchor = next;
    }
    result
}

/// 2 点間の line-of-sight を Bresenham 風にチェックする
fn line_of_sight(navmesh: &NavMesh, a: Vec3, b: Vec3) -> bool {
    let Some(start) = navmesh.world_to_grid(a) else {
        return false;
    };
    let Some(end) = navmesh.world_to_grid(b) else {
        return false;
    };

    let mut x = start.0 as i32;
    let mut y = start.1 as i32;
    let x1 = end.0 as i32;
    let y1 = end.1 as i32;
    let dx = (x1 - x).abs();
    let dy = (y1 - y).abs();
    let sx = if x < x1 { 1 } else { -1 };
    let sy = if y < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        if x < 0 || y < 0 || x >= navmesh.width as i32 || y >= navmesh.height as i32 {
            return false;
        }
        if !navmesh.is_walkable(x as usize, y as usize) {
            return false;
        }
        if x == x1 && y == y1 {
            return true;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::navmesh::{NavMesh, NavMeshBuilder};

    #[test]
    fn test_heuristic_zero() {
        assert!((heuristic((5, 5), (5, 5)) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_heuristic_diagonal() {
        // (0,0) → (3,3): diag = 3√2
        let h = heuristic((0, 0), (3, 3));
        assert!((h - 3.0 * std::f32::consts::SQRT_2).abs() < 1e-5);
    }

    #[test]
    fn test_find_path_straight() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        let path = find_path(&nm, Vec3::new(0.5, 0.0, 0.5), Vec3::new(8.5, 0.0, 0.5));
        assert!(!path.is_empty());
        assert!(path.first().unwrap().x < 1.0);
        assert!(path.last().unwrap().x > 8.0);
    }

    #[test]
    fn test_find_path_around_obstacle() {
        let mut builder = NavMeshBuilder::new(Vec3::ZERO, 1.0, 10, 10);
        // 真ん中に障害物
        builder.add_aabb_obstacle(Vec3::new(4.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 8.0));
        let nm = builder.build();
        let path = find_path(&nm, Vec3::new(0.5, 0.0, 0.5), Vec3::new(8.5, 0.0, 0.5));
        // 経路は迂回して障害物を避ける
        assert!(!path.is_empty());
        assert!(path.len() > 5, "obstacle should force longer path");
    }

    #[test]
    fn test_find_path_no_route() {
        let mut builder = NavMeshBuilder::new(Vec3::ZERO, 1.0, 10, 10);
        // 横一線に障害物 → start と goal を分断
        for x in 0..10 {
            builder
                .navmesh
                .set_cell(x, 5, crate::ai::navmesh::NavCell::Blocked);
        }
        let nm = builder.build();
        let path = find_path(&nm, Vec3::new(0.5, 0.0, 0.5), Vec3::new(0.5, 0.0, 9.5));
        assert!(path.is_empty(), "blocked path should return empty");
    }

    #[test]
    fn test_find_path_blocked_start() {
        let mut nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        nm.set_cell(0, 0, crate::ai::navmesh::NavCell::Blocked);
        let path = find_path(&nm, Vec3::new(0.5, 0.0, 0.5), Vec3::new(8.5, 0.0, 8.5));
        assert!(path.is_empty());
    }

    #[test]
    fn test_smooth_path_straight() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        let path = vec![
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 0.5),
            Vec3::new(2.5, 0.0, 0.5),
            Vec3::new(3.5, 0.0, 0.5),
        ];
        let smoothed = smooth_path(&nm, &path);
        // line-of-sight 通るので 2 点だけになる
        assert_eq!(smoothed.len(), 2);
    }

    #[test]
    fn test_line_of_sight_open() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        assert!(line_of_sight(
            &nm,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(5.5, 0.0, 5.5)
        ));
    }

    #[test]
    fn test_line_of_sight_blocked() {
        let mut nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        nm.set_cell(2, 2, crate::ai::navmesh::NavCell::Blocked);
        assert!(!line_of_sight(
            &nm,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(5.5, 0.0, 5.5)
        ));
    }
}
