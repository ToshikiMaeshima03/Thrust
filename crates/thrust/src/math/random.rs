use glam::Vec3;

/// 簡易擬似乱数生成器（LCG: 線形合同法）
///
/// 外部クレート不要の決定的乱数生成器。
/// ゲームロジックでの軽量な乱数が必要な場面に適している。
/// 暗号学的に安全ではないため、セキュリティ用途には使用しないこと。
#[derive(Debug, Clone)]
pub struct SimpleRng {
    seed: u32,
}

impl SimpleRng {
    /// 指定したシードで乱数生成器を作成する
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    /// `[0.0, 1.0)` の範囲の乱数を生成する
    pub fn next_f32(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.seed >> 16) & 0x7FFF) as f32 / 32767.0
    }

    /// 指定範囲 `[min, max)` の乱数を生成する
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    /// 単位球の内部のランダムな点を生成する（リジェクションサンプリング）
    pub fn in_unit_sphere(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            if v.length_squared() <= 1.0 {
                return v;
            }
        }
    }

    /// 単位球の表面上のランダムな点を生成する
    pub fn on_unit_sphere(&mut self) -> Vec3 {
        self.in_unit_sphere().normalize_or_zero()
    }

    /// XY 平面上の単位円内のランダムな点を生成する
    pub fn in_unit_circle(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.range(-1.0, 1.0), self.range(-1.0, 1.0), 0.0);
            if v.length_squared() <= 1.0 {
                return v;
            }
        }
    }

    /// ランダムな単位方向ベクトルを生成する（`on_unit_sphere` のエイリアス）
    pub fn direction(&mut self) -> Vec3 {
        self.on_unit_sphere()
    }

    /// 現在のシード値を取得する（状態の保存・復元用）
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// シードを設定する（状態の復元用）
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_sequence() {
        let mut rng1 = SimpleRng::new(42);
        let mut rng2 = SimpleRng::new(42);

        for _ in 0..100 {
            assert_eq!(rng1.next_f32().to_bits(), rng2.next_f32().to_bits());
        }
    }

    #[test]
    fn test_next_f32_range() {
        let mut rng = SimpleRng::new(12345);
        for _ in 0..1000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "next_f32 が範囲外: {v}");
        }
    }

    #[test]
    fn test_range_bounds() {
        let mut rng = SimpleRng::new(99);
        for _ in 0..1000 {
            let v = rng.range(5.0, 10.0);
            assert!((5.0..10.0).contains(&v), "range(5, 10) が範囲外: {v}");
        }
    }

    #[test]
    fn test_in_unit_sphere() {
        let mut rng = SimpleRng::new(42);
        for _ in 0..100 {
            let v = rng.in_unit_sphere();
            assert!(
                v.length_squared() <= 1.0 + 1e-6,
                "in_unit_sphere が単位球外: {}",
                v.length()
            );
        }
    }

    #[test]
    fn test_on_unit_sphere() {
        let mut rng = SimpleRng::new(42);
        for _ in 0..100 {
            let v = rng.on_unit_sphere();
            assert!(
                (v.length() - 1.0).abs() < 1e-5,
                "on_unit_sphere の長さが 1.0 でない: {}",
                v.length()
            );
        }
    }

    #[test]
    fn test_in_unit_circle() {
        let mut rng = SimpleRng::new(42);
        for _ in 0..100 {
            let v = rng.in_unit_circle();
            assert!(
                v.length_squared() <= 1.0 + 1e-6,
                "in_unit_circle が単位円外"
            );
            assert!((v.z).abs() < 1e-6, "in_unit_circle の z 成分が非 0");
        }
    }

    #[test]
    fn test_seed_save_restore() {
        let mut rng = SimpleRng::new(42);
        // いくつか進める
        for _ in 0..10 {
            rng.next_f32();
        }
        let saved_seed = rng.seed();
        let expected = rng.next_f32();

        // 復元して同じ値が出るか検証
        rng.set_seed(saved_seed);
        let actual = rng.next_f32();
        assert_eq!(expected.to_bits(), actual.to_bits());
    }
}
