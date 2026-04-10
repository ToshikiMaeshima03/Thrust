/// Hermite 補間（スムーズステップ）
///
/// `edge0..edge1` の範囲で滑らかに 0.0..1.0 を補間する。
/// `edge0` 以下は 0.0、`edge1` 以上は 1.0 を返す。
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ken Perlin の改良スムーズステップ（6t⁵ - 15t⁴ + 10t³）
///
/// `smoothstep` より滑らかな遷移（1階・2階微分が端点で 0）。
pub fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 線形補間の逆関数（`a..b` の範囲で `value` の補間係数 t を取得）
///
/// `a == b` の場合は 0.0 を返す。
pub fn inverse_lerp(a: f32, b: f32, value: f32) -> f32 {
    if (b - a).abs() < f32::EPSILON {
        return 0.0;
    }
    (value - a) / (b - a)
}

/// 値を一方の範囲から別の範囲へ再マッピングする
///
/// `from_min..from_max` の範囲にある `value` を `to_min..to_max` に変換する。
pub fn remap(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    let t = inverse_lerp(from_min, from_max, value);
    to_min + (to_max - to_min) * t
}

/// 浮動小数点のイプシロン比較
pub fn nearly_equal(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() <= epsilon
}

/// 現在値をターゲットに向かって最大 `max_delta` だけ移動する（スカラー版）
///
/// `Vec3` 版は `Vec3::move_towards()` を使用してください。
pub fn move_towards(current: f32, target: f32, max_delta: f32) -> f32 {
    if (target - current).abs() <= max_delta {
        target
    } else {
        current + (target - current).signum() * max_delta
    }
}

/// 値を範囲内にラップする（浮動小数点の剰余）
///
/// 例: `wrap(370.0, 0.0, 360.0)` → `10.0`
/// 例: `wrap(-10.0, 0.0, 360.0)` → `350.0`
pub fn wrap(value: f32, min: f32, max: f32) -> f32 {
    let range = max - min;
    if range <= 0.0 {
        return min;
    }
    min + ((value - min) % range + range) % range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothstep() {
        assert!((smoothstep(0.0, 1.0, 0.0)).abs() < 1e-6);
        assert!((smoothstep(0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
        // 範囲外
        assert!((smoothstep(0.0, 1.0, -1.0)).abs() < 1e-6);
        assert!((smoothstep(0.0, 1.0, 2.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_smootherstep() {
        assert!((smootherstep(0.0, 1.0, 0.0)).abs() < 1e-6);
        assert!((smootherstep(0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((smootherstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_inverse_lerp() {
        assert!((inverse_lerp(0.0, 10.0, 5.0) - 0.5).abs() < 1e-6);
        assert!((inverse_lerp(0.0, 10.0, 0.0)).abs() < 1e-6);
        assert!((inverse_lerp(0.0, 10.0, 10.0) - 1.0).abs() < 1e-6);
        // a == b の場合
        assert!((inverse_lerp(5.0, 5.0, 5.0)).abs() < 1e-6);
    }

    #[test]
    fn test_remap() {
        // 0..10 → 0..100
        assert!((remap(5.0, 0.0, 10.0, 0.0, 100.0) - 50.0).abs() < 1e-6);
        // 0..1 → -1..1
        assert!((remap(0.5, 0.0, 1.0, -1.0, 1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_nearly_equal() {
        assert!(nearly_equal(1.0, 1.0 + 1e-8, 1e-7));
        assert!(!nearly_equal(1.0, 2.0, 0.5));
    }

    #[test]
    fn test_move_towards() {
        assert!((move_towards(0.0, 10.0, 3.0) - 3.0).abs() < 1e-6);
        assert!((move_towards(9.0, 10.0, 3.0) - 10.0).abs() < 1e-6);
        assert!((move_towards(10.0, 0.0, 3.0) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn test_wrap() {
        assert!((wrap(370.0, 0.0, 360.0) - 10.0).abs() < 1e-6);
        assert!((wrap(-10.0, 0.0, 360.0) - 350.0).abs() < 1e-6);
        assert!((wrap(180.0, 0.0, 360.0) - 180.0).abs() < 1e-6);
        assert!((wrap(0.0, 0.0, 360.0)).abs() < 1e-6);
    }
}
