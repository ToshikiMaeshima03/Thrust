use glam::Vec3;

/// 点から線分への最近接点を求める
pub fn closest_point_on_line_segment(point: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let len_sq = ab.dot(ab);
    if len_sq < 1e-10 {
        return a; // 退化した線分（点）
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// 点から無限直線への距離
///
/// `line_dir` は正規化されている必要がある。
pub fn point_to_line_distance(point: Vec3, line_start: Vec3, line_dir: Vec3) -> f32 {
    let v = point - line_start;
    let proj = v.dot(line_dir);
    (v - line_dir * proj).length()
}

/// レイと平面の交差判定
///
/// 交差する場合はレイの原点からの距離 `t` を返す（`t >= 0` で前方交差）。
/// レイが平面と平行な場合は `None` を返す。
pub fn ray_plane_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> Option<f32> {
    let denom = ray_dir.dot(plane_normal);
    if denom.abs() < 1e-6 {
        return None; // レイが平面と平行
    }
    let t = (plane_point - ray_origin).dot(plane_normal) / denom;
    if t >= 0.0 { Some(t) } else { None }
}

/// Möller-Trumbore 法によるレイと三角形の交差判定
///
/// 交差する場合はレイの原点からの距離 `t` を返す。
pub fn ray_triangle_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<f32> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray_dir.cross(edge2);
    let a = edge1.dot(h);

    if a.abs() < 1e-6 {
        return None; // レイが三角形と平行
    }

    let f = 1.0 / a;
    let s = ray_origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray_dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);
    if t > 1e-6 { Some(t) } else { None }
}

/// 三角形内の点の重心座標を計算する
///
/// 返り値は `(u, v, w)` で、`point = u*v0 + v*v1 + w*v2`。
/// 点が三角形面上にない場合、結果は面への投影に基づく。
pub fn barycentric_coords(point: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> (f32, f32, f32) {
    let v0v1 = v1 - v0;
    let v0v2 = v2 - v0;
    let v0p = point - v0;

    let d00 = v0v1.dot(v0v1);
    let d01 = v0v1.dot(v0v2);
    let d11 = v0v2.dot(v0v2);
    let d20 = v0p.dot(v0v1);
    let d21 = v0p.dot(v0v2);

    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-10 {
        return (1.0, 0.0, 0.0); // 退化三角形
    }

    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    (u, v, w)
}

/// 三角形の面積を計算する
pub fn triangle_area(v0: Vec3, v1: Vec3, v2: Vec3) -> f32 {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    edge1.cross(edge2).length() * 0.5
}

/// 三角形の法線を計算する（正規化済み）
///
/// 退化三角形の場合は `Vec3::ZERO` を返す。
pub fn triangle_normal(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    edge1.cross(edge2).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closest_point_on_line_segment() {
        let a = Vec3::ZERO;
        let b = Vec3::new(10.0, 0.0, 0.0);

        // 中間点への投影
        let p = closest_point_on_line_segment(Vec3::new(5.0, 3.0, 0.0), a, b);
        assert!((p - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-6);

        // 端点 A 側のクランプ
        let p = closest_point_on_line_segment(Vec3::new(-5.0, 0.0, 0.0), a, b);
        assert!((p - Vec3::ZERO).length() < 1e-6);

        // 端点 B 側のクランプ
        let p = closest_point_on_line_segment(Vec3::new(15.0, 0.0, 0.0), a, b);
        assert!((p - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn test_point_to_line_distance() {
        let dist = point_to_line_distance(Vec3::new(0.0, 5.0, 0.0), Vec3::ZERO, Vec3::X);
        assert!((dist - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_ray_plane_intersection() {
        // 垂直にヒット
        let t = ray_plane_intersection(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, Vec3::ZERO, Vec3::Y);
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 1e-6);

        // 平行（ミス）
        let t = ray_plane_intersection(Vec3::new(0.0, 5.0, 0.0), Vec3::X, Vec3::ZERO, Vec3::Y);
        assert!(t.is_none());

        // 背後（ミス）
        let t = ray_plane_intersection(Vec3::new(0.0, 5.0, 0.0), Vec3::Y, Vec3::ZERO, Vec3::Y);
        assert!(t.is_none());
    }

    #[test]
    fn test_ray_triangle_intersection() {
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // ヒット
        let t = ray_triangle_intersection(Vec3::new(0.0, 0.0, -5.0), Vec3::Z, v0, v1, v2);
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 1e-5);

        // ミス（三角形の外側）
        let t = ray_triangle_intersection(Vec3::new(5.0, 5.0, -5.0), Vec3::Z, v0, v1, v2);
        assert!(t.is_none());
    }

    #[test]
    fn test_barycentric_coords() {
        let v0 = Vec3::ZERO;
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // 頂点 v0
        let (u, v, w) = barycentric_coords(v0, v0, v1, v2);
        assert!((u - 1.0).abs() < 1e-6);
        assert!(v.abs() < 1e-6);
        assert!(w.abs() < 1e-6);

        // 重心
        let center = (v0 + v1 + v2) / 3.0;
        let (u, v, w) = barycentric_coords(center, v0, v1, v2);
        assert!((u - 1.0 / 3.0).abs() < 1e-5);
        assert!((v - 1.0 / 3.0).abs() < 1e-5);
        assert!((w - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_triangle_area() {
        let v0 = Vec3::ZERO;
        let v1 = Vec3::new(2.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 3.0, 0.0);
        assert!((triangle_area(v0, v1, v2) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_triangle_normal() {
        let v0 = Vec3::ZERO;
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        let n = triangle_normal(v0, v1, v2);
        assert!((n - Vec3::Z).length() < 1e-6);
    }
}
