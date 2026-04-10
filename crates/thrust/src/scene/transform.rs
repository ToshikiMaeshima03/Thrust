use glam::{Mat4, Quat, Vec3};

#[derive(Clone)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// 位置のみ指定して Transform を作成
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Default::default()
        }
    }

    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// 法線変換行列を計算する
    ///
    /// スケールがゼロに近い（特異行列）場合は単位行列を返し、NaN の伝播を防止する。
    pub fn normal_matrix(&self) -> Mat4 {
        let m = self.to_matrix();
        let det = m.determinant();
        if det.abs() < 1e-10 {
            return Mat4::IDENTITY;
        }
        m.inverse().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_identity() {
        let t = Transform::default();
        assert_eq!(t.translation, Vec3::ZERO);
        assert_eq!(t.rotation, Quat::IDENTITY);
        assert_eq!(t.scale, Vec3::ONE);
    }

    #[test]
    fn test_from_translation() {
        let t = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(t.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(t.rotation, Quat::IDENTITY);
        assert_eq!(t.scale, Vec3::ONE);
    }

    #[test]
    fn test_to_matrix_identity() {
        let t = Transform::default();
        let m = t.to_matrix();
        assert!((m - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-6));
    }

    #[test]
    fn test_to_matrix_translation() {
        let t = Transform::from_translation(Vec3::new(5.0, 0.0, 0.0));
        let m = t.to_matrix();
        let point = m.transform_point3(Vec3::ZERO);
        assert!((point - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_to_matrix_scale() {
        let t = Transform {
            scale: Vec3::splat(2.0),
            ..Default::default()
        };
        let m = t.to_matrix();
        let point = m.transform_point3(Vec3::ONE);
        assert!((point - Vec3::splat(2.0)).length() < 1e-5);
    }

    #[test]
    fn test_to_matrix_rotation() {
        let t = Transform {
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        };
        let m = t.to_matrix();
        // Z 軸正方向が X 軸正方向に回転する
        let point = m.transform_point3(Vec3::new(0.0, 0.0, 1.0));
        assert!((point - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_normal_matrix_uniform_scale() {
        // 均一スケールの場合、normal_matrix は基本的にスケールの逆数
        let t = Transform {
            scale: Vec3::splat(2.0),
            ..Default::default()
        };
        let nm = t.normal_matrix();
        let normal = nm.transform_vector3(Vec3::Y).normalize();
        assert!((normal - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn test_roundtrip_trs() {
        let t = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_z(0.5),
            scale: Vec3::new(1.0, 2.0, 1.5),
        };
        let m = t.to_matrix();
        let (s, r, tr) = m.to_scale_rotation_translation();
        assert!((tr - t.translation).length() < 1e-5);
        assert!((s - t.scale).length() < 1e-5);
        assert!((r - t.rotation).length() < 1e-5);
    }

    #[test]
    fn test_normal_matrix_zero_scale_returns_identity() {
        // ゼロスケールで NaN が伝播しないことを確認
        let t = Transform {
            scale: Vec3::ZERO,
            ..Default::default()
        };
        let nm = t.normal_matrix();
        assert!(
            nm.abs_diff_eq(Mat4::IDENTITY, 1e-5),
            "ゼロスケールの normal_matrix は単位行列を返すべき"
        );
    }

    #[test]
    fn test_normal_matrix_partial_zero_scale() {
        // 一軸だけゼロスケール（特異行列）
        let t = Transform {
            scale: Vec3::new(1.0, 0.0, 1.0),
            ..Default::default()
        };
        let nm = t.normal_matrix();
        // NaN でないことを確認
        for col in 0..4 {
            for row in 0..4 {
                assert!(
                    nm.col(col)[row].is_finite(),
                    "normal_matrix に NaN/Inf が含まれている: col={col}, row={row}"
                );
            }
        }
    }

    #[test]
    fn test_normal_matrix_non_uniform_scale() {
        // 非均一スケールでの法線変換が正しいか確認
        let t = Transform {
            scale: Vec3::new(1.0, 2.0, 1.0),
            ..Default::default()
        };
        let nm = t.normal_matrix();
        // Y 方向に 2x 伸張した場合、Y法線はそのまま Y を指すべき
        let normal = nm.transform_vector3(Vec3::Y).normalize();
        assert!((normal - Vec3::Y).length() < 1e-5);
    }
}
