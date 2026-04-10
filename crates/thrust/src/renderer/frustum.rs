use glam::{Mat4, Vec4};

use crate::math::Aabb;

/// ビューフラスタム（6平面）
///
/// ビュー射影行列から平面を抽出し、AABB/球との交差判定を行う。
pub struct Frustum {
    /// 6つの平面 (法線xyz + 距離w)
    /// 順序: Left, Right, Bottom, Top, Near, Far
    pub planes: [Vec4; 6],
}

impl Frustum {
    /// ビュー射影行列からフラスタム平面を抽出する（Gribb/Hartmann 法）
    pub fn from_view_projection(vp: &Mat4) -> Self {
        let m = vp.to_cols_array_2d();

        // 行ベースでアクセスするため転置的に読む
        // m[col][row] の形式
        let row = |r: usize| -> Vec4 { Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]) };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let mut planes = [
            r3 + r0, // Left
            r3 - r0, // Right
            r3 + r1, // Bottom
            r3 - r1, // Top
            r3 + r2, // Near
            r3 - r2, // Far
        ];

        // 正規化
        for plane in &mut planes {
            let len = plane.truncate().length();
            if len > 0.0 {
                *plane /= len;
            }
        }

        Self { planes }
    }

    /// AABB がフラスタム内に少なくとも部分的に含まれるか判定
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        for plane in &self.planes {
            let normal = plane.truncate();
            let d = plane.w;

            // AABB の最も平面に近い頂点（正の頂点）を求める
            let p = glam::Vec3::new(
                if normal.x >= 0.0 {
                    aabb.max.x
                } else {
                    aabb.min.x
                },
                if normal.y >= 0.0 {
                    aabb.max.y
                } else {
                    aabb.min.y
                },
                if normal.z >= 0.0 {
                    aabb.max.z
                } else {
                    aabb.min.z
                },
            );

            // 正の頂点が平面の外側にある場合、AABB は完全に外側
            if normal.dot(p) + d < 0.0 {
                return false;
            }
        }

        true
    }

    /// 球がフラスタム内に少なくとも部分的に含まれるか判定
    pub fn intersects_sphere(&self, center: glam::Vec3, radius: f32) -> bool {
        for plane in &self.planes {
            let normal = plane.truncate();
            let d = plane.w;

            if normal.dot(center) + d < -radius {
                return false;
            }
        }

        true
    }
}

/// バウンディングボリュームコンポーネント（フラスタムカリング用）
///
/// このコンポーネントを持つエンティティはフラスタムカリングの対象になる。
/// 持たないエンティティは常に描画される。
pub struct BoundingVolume(pub Aabb);

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frustum() -> Frustum {
        // 標準的な透視投影フラスタム
        let proj = Mat4::perspective_rh(45.0_f32.to_radians(), 1.0, 0.1, 100.0);
        let view = Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        Frustum::from_view_projection(&(proj * view))
    }

    #[test]
    fn test_frustum_contains_origin() {
        let frustum = test_frustum();
        let aabb = Aabb::new(glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5));
        assert!(
            frustum.intersects_aabb(&aabb),
            "原点のAABBはフラスタム内であるべき"
        );
    }

    #[test]
    fn test_frustum_rejects_behind_camera() {
        let frustum = test_frustum();
        // カメラ (0,0,5) の背後 (z=10+)
        let aabb = Aabb::new(
            glam::Vec3::new(-1.0, -1.0, 10.0),
            glam::Vec3::new(1.0, 1.0, 12.0),
        );
        assert!(
            !frustum.intersects_aabb(&aabb),
            "カメラ背後のAABBはカリングされるべき"
        );
    }

    #[test]
    fn test_frustum_rejects_far_away() {
        let frustum = test_frustum();
        // far plane (100) を大きく超える位置
        let aabb = Aabb::new(
            glam::Vec3::new(-1.0, -1.0, -200.0),
            glam::Vec3::new(1.0, 1.0, -198.0),
        );
        assert!(
            !frustum.intersects_aabb(&aabb),
            "far plane 外のAABBはカリングされるべき"
        );
    }

    #[test]
    fn test_frustum_rejects_far_left() {
        let frustum = test_frustum();
        let aabb = Aabb::new(
            glam::Vec3::new(-500.0, -1.0, -5.0),
            glam::Vec3::new(-498.0, 1.0, -3.0),
        );
        assert!(
            !frustum.intersects_aabb(&aabb),
            "フラスタム左外のAABBはカリングされるべき"
        );
    }

    #[test]
    fn test_frustum_sphere_inside() {
        let frustum = test_frustum();
        assert!(
            frustum.intersects_sphere(glam::Vec3::ZERO, 1.0),
            "原点の球はフラスタム内であるべき"
        );
    }

    #[test]
    fn test_frustum_sphere_outside() {
        let frustum = test_frustum();
        assert!(
            !frustum.intersects_sphere(glam::Vec3::new(0.0, 0.0, 200.0), 1.0),
            "遠方の球はカリングされるべき"
        );
    }

    #[test]
    fn test_frustum_planes_normalized() {
        let frustum = test_frustum();
        for (i, plane) in frustum.planes.iter().enumerate() {
            let normal_len = plane.truncate().length();
            assert!(
                (normal_len - 1.0).abs() < 1e-4,
                "平面 {i} の法線が正規化されていない: {normal_len}"
            );
        }
    }
}
