//! グリッドベース navmesh (Round 5)
//!
//! ワールドを軸平行な 2D グリッド (Y=固定高さ) に分割し、各セルに通行可能フラグを持たせる。
//! より高度な navmesh (三角分割、リンク等) は将来実装。

use glam::Vec3;

/// 単一セルの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCell {
    /// 通行可能
    Walkable,
    /// 障害物
    Blocked,
}

/// グリッドベース navmesh
///
/// グリッドは XZ 平面 (Y=固定) に展開され、(0,0) は origin に対応する。
/// 各セルは `cell_size` の正方形。
pub struct NavMesh {
    /// セル幅 (世界座標)
    pub cell_size: f32,
    /// グリッド原点 (左下隅、ワールド座標)
    pub origin: Vec3,
    /// X 方向のセル数
    pub width: usize,
    /// Z 方向のセル数
    pub height: usize,
    /// セル状態 (row-major: cells[z * width + x])
    pub cells: Vec<NavCell>,
}

impl NavMesh {
    /// 全セルを Walkable で初期化する
    pub fn new(origin: Vec3, cell_size: f32, width: usize, height: usize) -> Self {
        Self {
            origin,
            cell_size,
            width,
            height,
            cells: vec![NavCell::Walkable; width * height],
        }
    }

    /// グリッド座標 → セル状態
    pub fn cell(&self, x: usize, z: usize) -> Option<NavCell> {
        if x >= self.width || z >= self.height {
            return None;
        }
        Some(self.cells[z * self.width + x])
    }

    /// セルを設定する
    pub fn set_cell(&mut self, x: usize, z: usize, cell: NavCell) {
        if x < self.width && z < self.height {
            self.cells[z * self.width + x] = cell;
        }
    }

    /// 通行可能か
    pub fn is_walkable(&self, x: usize, z: usize) -> bool {
        self.cell(x, z) == Some(NavCell::Walkable)
    }

    /// ワールド座標 → グリッド座標 (clamped)
    pub fn world_to_grid(&self, world: Vec3) -> Option<(usize, usize)> {
        let local = world - self.origin;
        let gx = (local.x / self.cell_size).floor() as i32;
        let gz = (local.z / self.cell_size).floor() as i32;
        if gx < 0 || gz < 0 {
            return None;
        }
        let (gx, gz) = (gx as usize, gz as usize);
        if gx >= self.width || gz >= self.height {
            return None;
        }
        Some((gx, gz))
    }

    /// グリッド座標 → ワールド座標 (セル中心)
    pub fn grid_to_world(&self, x: usize, z: usize) -> Vec3 {
        Vec3::new(
            self.origin.x + (x as f32 + 0.5) * self.cell_size,
            self.origin.y,
            self.origin.z + (z as f32 + 0.5) * self.cell_size,
        )
    }

    /// 隣接 8 セルを返す (通行可能のみ、コスト付き)
    pub fn neighbors(&self, x: usize, z: usize) -> Vec<((usize, usize), f32)> {
        let mut result = Vec::with_capacity(8);
        for dz in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let nz = z as i32 + dz;
                if nx < 0 || nz < 0 {
                    continue;
                }
                let (nx, nz) = (nx as usize, nz as usize);
                if !self.is_walkable(nx, nz) {
                    continue;
                }
                // 対角線は √2、軸は 1
                let cost = if dx != 0 && dz != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                result.push(((nx, nz), cost));
            }
        }
        result
    }
}

/// Navmesh ビルダー (球/AABB を障害物として登録)
pub struct NavMeshBuilder {
    pub navmesh: NavMesh,
}

impl NavMeshBuilder {
    pub fn new(origin: Vec3, cell_size: f32, width: usize, height: usize) -> Self {
        Self {
            navmesh: NavMesh::new(origin, cell_size, width, height),
        }
    }

    /// 円柱 (XZ 円) の領域を障害物としてマークする
    pub fn add_circle_obstacle(&mut self, center: Vec3, radius: f32) {
        let r2 = radius * radius;
        for z in 0..self.navmesh.height {
            for x in 0..self.navmesh.width {
                let cell_world = self.navmesh.grid_to_world(x, z);
                let dx = cell_world.x - center.x;
                let dz = cell_world.z - center.z;
                if dx * dx + dz * dz <= r2 {
                    self.navmesh.set_cell(x, z, NavCell::Blocked);
                }
            }
        }
    }

    /// AABB (XZ 投影) の領域を障害物としてマークする
    pub fn add_aabb_obstacle(&mut self, min: Vec3, max: Vec3) {
        for z in 0..self.navmesh.height {
            for x in 0..self.navmesh.width {
                let cell_world = self.navmesh.grid_to_world(x, z);
                if cell_world.x >= min.x
                    && cell_world.x <= max.x
                    && cell_world.z >= min.z
                    && cell_world.z <= max.z
                {
                    self.navmesh.set_cell(x, z, NavCell::Blocked);
                }
            }
        }
    }

    pub fn build(self) -> NavMesh {
        self.navmesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navmesh_default_walkable() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        assert_eq!(nm.cell(5, 5), Some(NavCell::Walkable));
    }

    #[test]
    fn test_navmesh_set_cell() {
        let mut nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        nm.set_cell(3, 4, NavCell::Blocked);
        assert_eq!(nm.cell(3, 4), Some(NavCell::Blocked));
        assert!(!nm.is_walkable(3, 4));
    }

    #[test]
    fn test_world_to_grid_origin() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        let cell = nm.world_to_grid(Vec3::new(0.5, 0.0, 0.5));
        assert_eq!(cell, Some((0, 0)));
    }

    #[test]
    fn test_world_to_grid_out_of_bounds() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        assert!(nm.world_to_grid(Vec3::new(-1.0, 0.0, 0.5)).is_none());
        assert!(nm.world_to_grid(Vec3::new(11.0, 0.0, 0.5)).is_none());
    }

    #[test]
    fn test_grid_to_world_center() {
        let nm = NavMesh::new(Vec3::ZERO, 2.0, 10, 10);
        let world = nm.grid_to_world(0, 0);
        assert!((world.x - 1.0).abs() < 1e-5);
        assert!((world.z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_neighbors_center() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        let neighbors = nm.neighbors(5, 5);
        assert_eq!(neighbors.len(), 8);
    }

    #[test]
    fn test_neighbors_corner() {
        let nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        let neighbors = nm.neighbors(0, 0);
        assert_eq!(neighbors.len(), 3); // (1,0), (0,1), (1,1)
    }

    #[test]
    fn test_neighbors_blocked_excluded() {
        let mut nm = NavMesh::new(Vec3::ZERO, 1.0, 10, 10);
        nm.set_cell(5, 4, NavCell::Blocked);
        let neighbors = nm.neighbors(5, 5);
        // (5, 4) は除外される
        assert_eq!(neighbors.len(), 7);
    }

    #[test]
    fn test_circle_obstacle() {
        let mut builder = NavMeshBuilder::new(Vec3::ZERO, 1.0, 10, 10);
        builder.add_circle_obstacle(Vec3::new(5.0, 0.0, 5.0), 2.0);
        let nm = builder.build();
        // 中心セルは塞がれる
        assert!(!nm.is_walkable(4, 4));
        // 遠いセルは通行可能
        assert!(nm.is_walkable(0, 0));
    }

    #[test]
    fn test_aabb_obstacle() {
        let mut builder = NavMeshBuilder::new(Vec3::ZERO, 1.0, 10, 10);
        builder.add_aabb_obstacle(Vec3::new(2.0, 0.0, 2.0), Vec3::new(4.0, 0.0, 4.0));
        let nm = builder.build();
        assert!(!nm.is_walkable(3, 3));
        assert!(nm.is_walkable(0, 0));
    }
}
