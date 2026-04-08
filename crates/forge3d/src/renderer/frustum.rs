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
