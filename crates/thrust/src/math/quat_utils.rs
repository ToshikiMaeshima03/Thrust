use glam::{Mat3, Quat, Vec3};

/// 指定した前方ベクトルと上方ベクトルから回転クォータニオンを生成する
///
/// `forward` と `up` は正規化されていなくても良い（内部で正規化される）。
/// `forward` がゼロベクトルの場合は `Quat::IDENTITY` を返す。
///
/// Unity の `Quaternion.LookRotation` に相当する。
pub fn look_rotation(forward: Vec3, up: Vec3) -> Quat {
    let f = forward.normalize_or_zero();
    if f == Vec3::ZERO {
        return Quat::IDENTITY;
    }

    let right = up.cross(f).normalize_or_zero();
    if right == Vec3::ZERO {
        // forward と up が平行な場合のフォールバック
        return Quat::from_rotation_arc(Vec3::NEG_Z, f);
    }

    let corrected_up = f.cross(right);
    Quat::from_mat3(&Mat3::from_cols(right, corrected_up, f))
}

/// 球面座標（yaw, pitch, distance）からワールド位置を計算する
///
/// Y-up 右手座標系。`OrbitalController` と同じ計算式。
pub fn spherical_to_cartesian(yaw: f32, pitch: f32, distance: f32) -> Vec3 {
    Vec3::new(
        distance * pitch.cos() * yaw.sin(),
        distance * pitch.sin(),
        distance * pitch.cos() * yaw.cos(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_look_rotation_forward_z() {
        // +Z 方向を向く → 恒等回転（ローカル +Z が +Z と一致）
        let q = look_rotation(Vec3::Z, Vec3::Y);
        let angle = q.angle_between(Quat::IDENTITY);
        assert!(angle < 1e-5, "+Z 方向は恒等回転に近いべき: {angle}");
    }

    #[test]
    fn test_look_rotation_forward_x() {
        let q = look_rotation(Vec3::X, Vec3::Y);
        // X 方向を向く → Y 軸周りに +90 度回転（ローカル +Z を +X に回転）
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let angle = q.angle_between(expected);
        assert!(angle < 1e-3, "X 方向の look_rotation: 角度差 {angle}");
    }

    #[test]
    fn test_look_rotation_zero_forward() {
        let q = look_rotation(Vec3::ZERO, Vec3::Y);
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn test_spherical_to_cartesian() {
        // yaw=0, pitch=0, distance=1 → (0, 0, 1)
        let p = spherical_to_cartesian(0.0, 0.0, 1.0);
        assert!((p - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);

        // yaw=PI/2, pitch=0, distance=1 → (1, 0, 0) 付近
        let p = spherical_to_cartesian(std::f32::consts::FRAC_PI_2, 0.0, 1.0);
        assert!((p - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);

        // pitch=PI/2, distance=2 → (0, 2, 0)
        let p = spherical_to_cartesian(0.0, std::f32::consts::FRAC_PI_2, 2.0);
        assert!((p - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-5);
    }
}
