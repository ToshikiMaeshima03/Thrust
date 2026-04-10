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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Aabb テスト ───

    #[test]
    fn test_aabb_center() {
        let aabb = Aabb::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.center(), Vec3::ZERO);
    }

    #[test]
    fn test_aabb_half_extents() {
        let aabb = Aabb::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.half_extents(), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_aabb_intersects_overlapping() {
        let a = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let b = Aabb::new(Vec3::ONE, Vec3::new(3.0, 3.0, 3.0));
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn test_aabb_intersects_touching() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::ONE, Vec3::new(2.0, 2.0, 2.0));
        // 境界が接触 → 交差とみなす
        assert!(a.intersects(&b));
    }

    #[test]
    fn test_aabb_no_intersection() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0));
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn test_aabb_intersects_one_axis_separated() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        // X 軸だけ離れている
        let b = Aabb::new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 1.0));
        assert!(!a.intersects(&b));
    }

    #[test]
    fn test_aabb_contains_point_inside() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0));
        assert!(aabb.contains_point(Vec3::new(5.0, 5.0, 5.0)));
    }

    #[test]
    fn test_aabb_contains_point_boundary() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(aabb.contains_point(Vec3::ZERO));
        assert!(aabb.contains_point(Vec3::ONE));
    }

    #[test]
    fn test_aabb_contains_point_outside() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(!aabb.contains_point(Vec3::new(-0.1, 0.5, 0.5)));
        assert!(!aabb.contains_point(Vec3::new(0.5, 1.1, 0.5)));
    }

    #[test]
    fn test_aabb_from_vertices() {
        let vertices = vec![
            crate::mesh::vertex::Vertex::new([-1.0, 0.0, 2.0], [0.0; 3], [0.0; 2]),
            crate::mesh::vertex::Vertex::new([3.0, -2.0, 0.0], [0.0; 3], [0.0; 2]),
            crate::mesh::vertex::Vertex::new([0.0, 5.0, -1.0], [0.0; 3], [0.0; 2]),
        ];
        let aabb = Aabb::from_vertices(&vertices);
        assert_eq!(aabb.min, Vec3::new(-1.0, -2.0, -1.0));
        assert_eq!(aabb.max, Vec3::new(3.0, 5.0, 2.0));
    }

    #[test]
    fn test_aabb_transformed_translation() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let matrix = glam::Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let t = aabb.transformed(&matrix);
        assert!((t.min - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
        assert!((t.max - Vec3::new(11.0, 1.0, 1.0)).length() < 1e-5);
    }

    #[test]
    fn test_aabb_transformed_scale() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::ONE);
        let matrix = glam::Mat4::from_scale(Vec3::splat(2.0));
        let t = aabb.transformed(&matrix);
        assert!((t.min - Vec3::splat(-2.0)).length() < 1e-5);
        assert!((t.max - Vec3::splat(2.0)).length() < 1e-5);
    }

    #[test]
    fn test_aabb_merge() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::new(-1.0, 2.0, 0.0), Vec3::new(0.5, 3.0, 4.0));
        let merged = a.merge(&b);
        assert_eq!(merged.min, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(merged.max, Vec3::new(1.0, 3.0, 4.0));
    }

    // ─── BoundingSphere テスト ───

    #[test]
    fn test_bounding_sphere_from_aabb() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::ONE);
        let sphere = BoundingSphere::from_aabb(&aabb);
        assert_eq!(sphere.center, Vec3::ZERO);
        assert!((sphere.radius - 3.0_f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn test_bounding_sphere_intersects() {
        let a = BoundingSphere::new(Vec3::ZERO, 1.0);
        let b = BoundingSphere::new(Vec3::new(1.5, 0.0, 0.0), 1.0);
        assert!(a.intersects(&b)); // 距離 1.5 < 半径合計 2.0

        let c = BoundingSphere::new(Vec3::new(3.0, 0.0, 0.0), 1.0);
        assert!(!a.intersects(&c)); // 距離 3.0 > 半径合計 2.0
    }

    #[test]
    fn test_bounding_sphere_intersects_aabb() {
        let sphere = BoundingSphere::new(Vec3::new(2.0, 0.5, 0.5), 0.6);
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        // 球の中心は (2.0, 0.5, 0.5)、AABB 最近接点は (1.0, 0.5, 0.5)、距離 1.0 > 半径 0.6
        assert!(!sphere.intersects_aabb(&aabb));

        let sphere2 = BoundingSphere::new(Vec3::new(1.3, 0.5, 0.5), 0.5);
        // 球の中心は (1.3, 0.5, 0.5)、AABB 最近接点は (1.0, 0.5, 0.5)、距離 0.3 < 半径 0.5
        assert!(sphere2.intersects_aabb(&aabb));
    }

    #[test]
    fn test_bounding_sphere_inside_aabb() {
        let sphere = BoundingSphere::new(Vec3::new(0.5, 0.5, 0.5), 0.1);
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(sphere.intersects_aabb(&aabb));
    }
}
