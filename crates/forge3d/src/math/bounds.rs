use glam::Vec3;

use crate::mesh::vertex::Vertex;

/// 軸平行バウンディングボックス
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// 2点から AABB を作成
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// 頂点配列から AABB を計算
    pub fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        Self { min, max }
    }

    /// 中心座標を取得
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// 半径（各軸の半分のサイズ）を取得
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// 別の AABB と交差するか判定
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// 点が AABB 内に含まれるか判定
    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// 変換行列を適用した近似 AABB を計算（8隅を変換して新しい AABB を作成）
    pub fn transformed(&self, matrix: &glam::Mat4) -> Self {
        let corners = [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ];

        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for corner in &corners {
            let transformed = matrix.transform_point3(*corner);
            min = min.min(transformed);
            max = max.max(transformed);
        }

        Self { min, max }
    }

    /// 2つの AABB を結合
    pub fn merge(&self, other: &Aabb) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

/// バウンディング球
#[derive(Debug, Clone, Copy)]
pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f32,
}

impl BoundingSphere {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// AABB からバウンディング球を計算
    pub fn from_aabb(aabb: &Aabb) -> Self {
        let center = aabb.center();
        let radius = aabb.half_extents().length();
        Self { center, radius }
    }

    /// 別のバウンディング球と交差するか判定
    pub fn intersects(&self, other: &BoundingSphere) -> bool {
        let dist_sq = self.center.distance_squared(other.center);
        let radius_sum = self.radius + other.radius;
        dist_sq <= radius_sum * radius_sum
    }

    /// AABB と交差するか判定
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        // 球の中心から AABB への最近接点を求める
        let closest = self.center.clamp(aabb.min, aabb.max);
        let dist_sq = self.center.distance_squared(closest);
        dist_sq <= self.radius * self.radius
    }
}
