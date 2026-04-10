//! スプライン / ベジェ曲線 (Round 8)
//!
//! - Catmull-Rom スプライン (制御点を通過)
//! - Cubic Bezier 曲線 (制御点 4 つ)
//! - 弧長パラメタライゼーションでスムーズな移動
//!
//! 用途: パスアニメ、AI のパトロール、カメラパス。

use glam::Vec3;

/// Catmull-Rom スプライン (centripetal version)
#[derive(Debug, Clone)]
pub struct CatmullRomSpline {
    pub points: Vec<Vec3>,
    /// 弧長サンプル (建てた後にキャッシュ)
    arc_lengths: Vec<f32>,
    /// 各セグメントの始点パラメータ
    segment_starts: Vec<f32>,
}

impl CatmullRomSpline {
    /// 制御点から構築する。3 点未満は許可しない。
    pub fn new(points: Vec<Vec3>) -> Self {
        let mut s = Self {
            points,
            arc_lengths: Vec::new(),
            segment_starts: Vec::new(),
        };
        s.recompute_arc_lengths();
        s
    }

    /// 1 セグメント (4 制御点) を t ∈ [0, 1] で評価
    pub fn segment(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
        let t2 = t * t;
        let t3 = t2 * t;
        let half = 0.5;
        half * ((2.0 * p1)
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
    }

    /// グローバルパラメータ `s ∈ [0, 1]` で曲線を評価する (パラメータ的に均等)
    pub fn evaluate(&self, s: f32) -> Vec3 {
        let n = self.points.len();
        if n < 2 {
            return self.points.first().copied().unwrap_or(Vec3::ZERO);
        }
        if n == 2 {
            return self.points[0].lerp(self.points[1], s.clamp(0.0, 1.0));
        }
        let segments = n - 1;
        let s_clamped = s.clamp(0.0, 1.0);
        let segment_pos = s_clamped * segments as f32;
        let seg_idx = (segment_pos as usize).min(segments - 1);
        let local_t = segment_pos - seg_idx as f32;

        let p0 = self.points[seg_idx.saturating_sub(1).min(n - 1)];
        let p1 = self.points[seg_idx];
        let p2 = self.points[(seg_idx + 1).min(n - 1)];
        let p3 = self.points[(seg_idx + 2).min(n - 1)];
        Self::segment(p0, p1, p2, p3, local_t)
    }

    /// 弧長で正規化されたパラメータで曲線を評価する
    /// `arc_s ∈ [0, total_length]` を入力すると、線形に動くポイントを返す
    pub fn evaluate_by_arc(&self, arc_s: f32) -> Vec3 {
        let total = self.total_length();
        if total < 1e-5 {
            return self.points.first().copied().unwrap_or(Vec3::ZERO);
        }
        let target = arc_s.clamp(0.0, total);
        // 二分探索で arc_lengths から該当 segment を見つける
        let mut lo = 0;
        let mut hi = self.arc_lengths.len() - 1;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.arc_lengths[mid] < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let idx = lo.max(1);
        let prev = self.arc_lengths[idx - 1];
        let curr = self.arc_lengths[idx];
        let local = if (curr - prev).abs() > 1e-5 {
            (target - prev) / (curr - prev)
        } else {
            0.0
        };
        let s_param = self.segment_starts[idx - 1]
            + local * (self.segment_starts[idx] - self.segment_starts[idx - 1]);
        self.evaluate(s_param)
    }

    /// 全長
    pub fn total_length(&self) -> f32 {
        self.arc_lengths.last().copied().unwrap_or(0.0)
    }

    fn recompute_arc_lengths(&mut self) {
        let samples = 200;
        self.arc_lengths.clear();
        self.segment_starts.clear();
        if self.points.len() < 2 {
            return;
        }
        let mut prev = self.evaluate(0.0);
        self.arc_lengths.push(0.0);
        self.segment_starts.push(0.0);
        for i in 1..=samples {
            let s = i as f32 / samples as f32;
            let p = self.evaluate(s);
            let last = self.arc_lengths.last().copied().unwrap_or(0.0);
            self.arc_lengths.push(last + (p - prev).length());
            self.segment_starts.push(s);
            prev = p;
        }
    }
}

/// Cubic Bezier 曲線 (4 制御点)
#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    pub p0: Vec3,
    pub p1: Vec3,
    pub p2: Vec3,
    pub p3: Vec3,
}

impl CubicBezier {
    pub fn new(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// t ∈ [0, 1] で評価
    pub fn evaluate(&self, t: f32) -> Vec3 {
        let t = t.clamp(0.0, 1.0);
        let one_minus_t = 1.0 - t;
        let b0 = one_minus_t * one_minus_t * one_minus_t;
        let b1 = 3.0 * one_minus_t * one_minus_t * t;
        let b2 = 3.0 * one_minus_t * t * t;
        let b3 = t * t * t;
        self.p0 * b0 + self.p1 * b1 + self.p2 * b2 + self.p3 * b3
    }

    /// 接ベクトル (一階微分)
    pub fn tangent(&self, t: f32) -> Vec3 {
        let t = t.clamp(0.0, 1.0);
        let one_minus_t = 1.0 - t;
        3.0 * one_minus_t * one_minus_t * (self.p1 - self.p0)
            + 6.0 * one_minus_t * t * (self.p2 - self.p1)
            + 3.0 * t * t * (self.p3 - self.p2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catmull_rom_passes_through_points() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
        ];
        let s = CatmullRomSpline::new(pts.clone());
        let p0 = s.evaluate(0.0);
        let p1 = s.evaluate(1.0);
        // 始点と終点が制御点に近い
        assert!((p0 - pts[0]).length() < 0.1);
        assert!((p1 - pts[3]).length() < 0.1);
    }

    #[test]
    fn test_catmull_rom_total_length() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ];
        let s = CatmullRomSpline::new(pts);
        let total = s.total_length();
        assert!(total > 1.5 && total < 2.5);
    }

    #[test]
    fn test_evaluate_by_arc_uniform() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
        ];
        let s = CatmullRomSpline::new(pts);
        let p_mid = s.evaluate_by_arc(s.total_length() * 0.5);
        // x ≈ 5
        assert!((p_mid.x - 5.0).abs() < 0.5);
    }

    #[test]
    fn test_bezier_endpoints() {
        let b = CubicBezier::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(2.0, 1.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
        );
        assert!((b.evaluate(0.0) - b.p0).length() < 1e-5);
        assert!((b.evaluate(1.0) - b.p3).length() < 1e-5);
    }

    #[test]
    fn test_bezier_tangent_at_start() {
        let b = CubicBezier::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
        );
        let t = b.tangent(0.0);
        // 接線は p0 → p1 方向
        assert!(t.x > 0.0);
    }

    #[test]
    fn test_two_point_spline() {
        let pts = vec![Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)];
        let s = CatmullRomSpline::new(pts);
        let mid = s.evaluate(0.5);
        assert!((mid.x - 0.5).abs() < 1e-5);
    }
}
