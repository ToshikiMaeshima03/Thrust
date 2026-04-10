use glam::{Mat4, Quat, Vec3, Vec4};

/// 変換行列からスケールの各軸成分を抽出する
///
/// 各列ベクトルの長さからスケールを推定する。
pub fn extract_scale(mat: &Mat4) -> Vec3 {
    Vec3::new(
        mat.x_axis.truncate().length(),
        mat.y_axis.truncate().length(),
        mat.z_axis.truncate().length(),
    )
}

/// 変換行列から最大スケール成分を抽出する（均一スケール近似）
///
/// 球コライダーの半径スケーリングなどに使用する。
pub fn extract_max_scale(mat: &Mat4) -> f32 {
    let s = extract_scale(mat);
    s.x.max(s.y).max(s.z)
}

/// 変換行列から平行移動成分を抽出する
pub fn extract_translation(mat: &Mat4) -> Vec3 {
    mat.w_axis.truncate()
}

/// 変換行列を平行移動・回転・スケールに分解する
///
/// 返り値は `(translation, rotation, scale)`。
/// 負のスケール（ミラーリング）が含まれる場合、結果が不正確になる可能性がある。
pub fn decompose(mat: &Mat4) -> (Vec3, Quat, Vec3) {
    let translation = extract_translation(mat);
    let scale = extract_scale(mat);

    let inv_scale = Vec3::new(
        if scale.x.abs() > f32::EPSILON {
            1.0 / scale.x
        } else {
            0.0
        },
        if scale.y.abs() > f32::EPSILON {
            1.0 / scale.y
        } else {
            0.0
        },
        if scale.z.abs() > f32::EPSILON {
            1.0 / scale.z
        } else {
            0.0
        },
    );

    let rot_mat = Mat4::from_cols(
        (mat.x_axis.truncate() * inv_scale.x).extend(0.0),
        (mat.y_axis.truncate() * inv_scale.y).extend(0.0),
        (mat.z_axis.truncate() * inv_scale.z).extend(0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );

    let rotation = Quat::from_mat4(&rot_mat);
    (translation, rotation, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_scale() {
        let mat = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 4.0),
            Quat::IDENTITY,
            Vec3::ZERO,
        );
        let scale = extract_scale(&mat);
        assert!((scale.x - 2.0).abs() < 1e-5);
        assert!((scale.y - 3.0).abs() < 1e-5);
        assert!((scale.z - 4.0).abs() < 1e-5);
    }

    #[test]
    fn test_extract_max_scale() {
        let mat = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 5.0, 3.0),
            Quat::IDENTITY,
            Vec3::ZERO,
        );
        assert!((extract_max_scale(&mat) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_extract_translation() {
        let mat = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let t = extract_translation(&mat);
        assert!((t - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
    }

    #[test]
    fn test_decompose_round_trip() {
        let original_translation = Vec3::new(1.0, 2.0, 3.0);
        let original_rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let original_scale = Vec3::new(2.0, 3.0, 4.0);

        let mat = Mat4::from_scale_rotation_translation(
            original_scale,
            original_rotation,
            original_translation,
        );

        let (t, r, s) = decompose(&mat);

        assert!(
            (t - original_translation).length() < 1e-5,
            "平行移動の分解: {t}"
        );
        assert!((s - original_scale).length() < 1e-5, "スケールの分解: {s}");
        // クォータニオンは符号反転でも同じ回転を表す
        let angle_diff = r.angle_between(original_rotation);
        assert!(angle_diff < 1e-5, "回転の分解: 角度差 {angle_diff}");
    }
}
