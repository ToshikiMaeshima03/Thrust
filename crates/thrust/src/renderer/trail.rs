//! トレイルレンダラー (Round 8)
//!
//! エンティティの移動軌跡をリボンメッシュで描画する。
//! 一定間隔で点を取得 → 連続する点をクアッドでつないで半透明合成。
//!
//! 用途: 剣の軌跡、弾丸の光跡、ジェット噴射、スピード感の演出

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use hecs::World;

use crate::scene::transform::Transform;

/// トレイルポイント
#[derive(Debug, Clone, Copy)]
pub struct TrailPoint {
    pub position: Vec3,
    pub age: f32,
}

/// トレイルコンポーネント (ECS)
#[derive(Debug, Clone)]
pub struct TrailRenderer {
    /// 過去の軌跡 (新しい順)
    pub points: Vec<TrailPoint>,
    /// 各点の生存時間 (sec)
    pub lifetime: f32,
    /// 最大点数 (リング bounded)
    pub max_points: usize,
    /// トレイル幅 (m)
    pub width: f32,
    /// 開始色
    pub color_start: glam::Vec4,
    /// 終了色
    pub color_end: glam::Vec4,
    /// 新しい点を追加する最小距離 (m)
    pub min_distance: f32,
    /// 直前のサンプリング位置
    pub last_position: Option<Vec3>,
}

impl Default for TrailRenderer {
    fn default() -> Self {
        Self {
            points: Vec::with_capacity(64),
            lifetime: 1.0,
            max_points: 64,
            width: 0.2,
            color_start: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            color_end: glam::Vec4::new(1.0, 1.0, 1.0, 0.0),
            min_distance: 0.05,
            last_position: None,
        }
    }
}

impl TrailRenderer {
    pub fn new(lifetime: f32, max_points: usize, width: f32) -> Self {
        Self {
            lifetime,
            max_points,
            width,
            ..Default::default()
        }
    }

    /// 新しい点を追加する (最小距離以上離れていれば)
    pub fn add_point(&mut self, position: Vec3) {
        let should_add = match self.last_position {
            Some(last) => (position - last).length() >= self.min_distance,
            None => true,
        };
        if should_add {
            self.points.insert(0, TrailPoint { position, age: 0.0 });
            if self.points.len() > self.max_points {
                self.points.truncate(self.max_points);
            }
            self.last_position = Some(position);
        }
    }

    /// 各点の age を進めて、寿命超過は削除
    pub fn update(&mut self, dt: f32) {
        for p in self.points.iter_mut() {
            p.age += dt;
        }
        self.points.retain(|p| p.age < self.lifetime);
    }

    /// リボン頂点バッファ用のクアッドを生成 (camera right ベクトルを必要とする)
    pub fn build_quads(&self, camera_right: Vec3) -> Vec<TrailVertex> {
        if self.points.len() < 2 {
            return Vec::new();
        }
        let half_w = self.width * 0.5;
        let mut out = Vec::with_capacity(self.points.len() * 2);
        for p in &self.points {
            let life_t = (p.age / self.lifetime).clamp(0.0, 1.0);
            let color = self.color_start.lerp(self.color_end, life_t);
            let left = p.position - camera_right * half_w * (1.0 - life_t);
            let right = p.position + camera_right * half_w * (1.0 - life_t);
            out.push(TrailVertex {
                position: [left.x, left.y, left.z],
                _pad0: 0.0,
                color: color.to_array(),
            });
            out.push(TrailVertex {
                position: [right.x, right.y, right.z],
                _pad0: 0.0,
                color: color.to_array(),
            });
        }
        out
    }
}

/// GPU 用トレイル頂点
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TrailVertex {
    pub position: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 4],
}

/// `TrailRenderer + Transform` を持つエンティティをサンプリングするシステム
pub fn trail_sample_system(world: &mut World, dt: f32) {
    let updates: Vec<(hecs::Entity, Vec3)> = world
        .query::<(hecs::Entity, &TrailRenderer, &Transform)>()
        .iter()
        .map(|(e, _, t)| (e, t.translation))
        .collect();

    for (entity, pos) in updates {
        if let Ok(mut tr) = world.get::<&mut TrailRenderer>(entity) {
            tr.update(dt);
            tr.add_point(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trail_default() {
        let t = TrailRenderer::default();
        assert!(t.points.is_empty());
        assert!(t.lifetime > 0.0);
    }

    #[test]
    fn test_add_point_first() {
        let mut t = TrailRenderer::default();
        t.add_point(Vec3::ZERO);
        assert_eq!(t.points.len(), 1);
    }

    #[test]
    fn test_add_point_min_distance() {
        let mut t = TrailRenderer::default();
        t.add_point(Vec3::ZERO);
        // 最小距離より近い → 追加されない
        t.add_point(Vec3::new(0.001, 0.0, 0.0));
        assert_eq!(t.points.len(), 1);
        // 最小距離より遠い → 追加される
        t.add_point(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(t.points.len(), 2);
    }

    #[test]
    fn test_max_points_truncated() {
        let mut t = TrailRenderer::new(10.0, 5, 0.1);
        for i in 0..20 {
            t.add_point(Vec3::new(i as f32, 0.0, 0.0));
        }
        assert!(t.points.len() <= 5);
    }

    #[test]
    fn test_update_ages_and_removes() {
        let mut t = TrailRenderer::new(1.0, 10, 0.1);
        t.add_point(Vec3::ZERO);
        t.update(0.5);
        assert!(t.points[0].age > 0.4);
        t.update(0.6);
        assert_eq!(t.points.len(), 0); // expired
    }

    #[test]
    fn test_build_quads_two_points() {
        let mut t = TrailRenderer::default();
        t.add_point(Vec3::ZERO);
        t.add_point(Vec3::new(1.0, 0.0, 0.0));
        let quads = t.build_quads(Vec3::Z);
        assert_eq!(quads.len(), 4); // 2 points × 2 verts each
    }

    #[test]
    fn test_trail_vertex_size() {
        // 16 + 16 = 32 B
        assert_eq!(std::mem::size_of::<TrailVertex>(), 32);
    }
}
