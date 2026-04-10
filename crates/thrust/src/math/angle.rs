use std::f32::consts::PI;

use glam::Vec3;

/// 度からラジアンに変換する
#[inline]
pub fn deg_to_rad(degrees: f32) -> f32 {
    degrees.to_radians()
}

/// ラジアンから度に変換する
#[inline]
pub fn rad_to_deg(radians: f32) -> f32 {
    radians.to_degrees()
}

/// 角度を `[-PI, PI]` の範囲に正規化する
pub fn normalize_angle(radians: f32) -> f32 {
    let mut a = radians % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a < -PI {
        a += 2.0 * PI;
    }
    a
}

/// 2つのベクトル間の符号付き角度（`axis` 周りの回転方向）を返す
///
/// 返り値は `[-PI, PI]` の範囲。
/// `axis` は正規化されている必要がある。
pub fn signed_angle(from: Vec3, to: Vec3, axis: Vec3) -> f32 {
    let unsigned = from.angle_between(to);
    let cross = from.cross(to);
    let sign = cross.dot(axis).signum();
    unsigned * sign
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn test_deg_to_rad() {
        assert!((deg_to_rad(180.0) - PI).abs() < 1e-6);
        assert!((deg_to_rad(90.0) - FRAC_PI_2).abs() < 1e-6);
        assert!((deg_to_rad(0.0)).abs() < 1e-6);
    }

    #[test]
    fn test_rad_to_deg() {
        assert!((rad_to_deg(PI) - 180.0).abs() < 1e-6);
        assert!((rad_to_deg(FRAC_PI_2) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_angle() {
        assert!((normalize_angle(0.0)).abs() < 1e-6);
        assert!((normalize_angle(PI) - PI).abs() < 1e-6);
        assert!((normalize_angle(3.0 * PI) - PI).abs() < 1e-5);
        assert!((normalize_angle(-3.0 * PI) + PI).abs() < 1e-5);
    }

    #[test]
    fn test_signed_angle() {
        let angle = signed_angle(Vec3::X, Vec3::Z, Vec3::Y);
        // X → Z は Y 軸周りで -90 度（右手系）
        assert!(
            (angle + FRAC_PI_2).abs() < 1e-5,
            "X→Z の Y 軸周り符号付き角度: {angle}"
        );

        let angle2 = signed_angle(Vec3::Z, Vec3::X, Vec3::Y);
        assert!(
            (angle2 - FRAC_PI_2).abs() < 1e-5,
            "Z→X の Y 軸周り符号付き角度: {angle2}"
        );
    }
}
