//! 自動露出 (Round 8)
//!
//! HDR 画像の平均輝度に基づいて露出を時間積分で調整する。
//! GPU 側でヒストグラム compute shader を走らせる代わりに、
//! `PostUniform.exposure` を CPU 側で smoothly approach する形で実装。
//!
//! より高品質な auto exposure には GPU compute によるヒストグラム計算が必要だが、
//! ここでは「シーン中の代表輝度を CPU から指定 → smoothing」のシンプル形式とする。

use bytemuck::{Pod, Zeroable};

/// 自動露出コントローラー
#[derive(Debug, Clone, Copy)]
pub struct AutoExposure {
    /// 現在の露出値 (linear)
    pub current_exposure: f32,
    /// 目標露出値 (linear)
    pub target_exposure: f32,
    /// EV 補正 (露出補正)
    pub ev_compensation: f32,
    /// 適応速度 (1/sec)、明るくなる時
    pub adapt_speed_up: f32,
    /// 適応速度 (1/sec)、暗くなる時
    pub adapt_speed_down: f32,
    /// 最小露出
    pub min_exposure: f32,
    /// 最大露出
    pub max_exposure: f32,
    /// 有効/無効
    pub enabled: bool,
}

impl Default for AutoExposure {
    fn default() -> Self {
        Self {
            current_exposure: 1.0,
            target_exposure: 1.0,
            ev_compensation: 0.0,
            adapt_speed_up: 2.0,
            adapt_speed_down: 1.0,
            min_exposure: 0.05,
            max_exposure: 8.0,
            enabled: false,
        }
    }
}

impl AutoExposure {
    /// 平均輝度から目標露出を計算する。
    /// `avg_luminance` は scene 全体の平均輝度 (例: bloom mip[N-1] の中央ピクセルなど)
    pub fn set_target_from_luminance(&mut self, avg_luminance: f32) {
        // EV: 18% グレーが luminance ≈ 0.18 になる露出
        let target_grey = 0.18;
        let target = target_grey / avg_luminance.max(1e-4);
        self.target_exposure = (target * 2.0_f32.powf(self.ev_compensation))
            .clamp(self.min_exposure, self.max_exposure);
    }

    /// 時間積分で current_exposure を target に近づける
    pub fn update(&mut self, dt: f32) {
        if !self.enabled {
            return;
        }
        let speed = if self.target_exposure > self.current_exposure {
            self.adapt_speed_up
        } else {
            self.adapt_speed_down
        };
        let alpha = 1.0 - (-speed * dt).exp();
        self.current_exposure =
            self.current_exposure + (self.target_exposure - self.current_exposure) * alpha;
    }
}

/// GPU に送信する露出 uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ExposureUniform {
    /// x = exposure, y = ev_compensation, z = enabled, w = _
    pub params: [f32; 4],
}

impl Default for ExposureUniform {
    fn default() -> Self {
        Self {
            params: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

impl From<&AutoExposure> for ExposureUniform {
    fn from(ae: &AutoExposure) -> Self {
        Self {
            params: [
                ae.current_exposure,
                ae.ev_compensation,
                if ae.enabled { 1.0 } else { 0.0 },
                0.0,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_disabled() {
        let ae = AutoExposure::default();
        assert!(!ae.enabled);
        assert!((ae.current_exposure - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_target_from_luminance_brighter() {
        let mut ae = AutoExposure::default();
        ae.set_target_from_luminance(1.0); // 明るい
        assert!(ae.target_exposure < 1.0);
    }

    #[test]
    fn test_target_from_luminance_darker() {
        let mut ae = AutoExposure::default();
        ae.set_target_from_luminance(0.05); // 暗い
        assert!(ae.target_exposure > 1.0);
    }

    #[test]
    fn test_target_clamped() {
        let mut ae = AutoExposure::default();
        ae.set_target_from_luminance(1e-10);
        assert!(ae.target_exposure <= ae.max_exposure);
    }

    #[test]
    fn test_update_when_disabled() {
        let mut ae = AutoExposure::default();
        ae.target_exposure = 5.0;
        ae.update(1.0);
        assert!((ae.current_exposure - 1.0).abs() < 1e-5); // unchanged
    }

    #[test]
    fn test_update_when_enabled() {
        let mut ae = AutoExposure {
            enabled: true,
            ..Default::default()
        };
        ae.target_exposure = 2.0;
        for _ in 0..30 {
            ae.update(0.1);
        }
        assert!((ae.current_exposure - 2.0).abs() < 0.05);
    }

    #[test]
    fn test_uniform_size() {
        assert_eq!(std::mem::size_of::<ExposureUniform>(), 16);
    }
}
