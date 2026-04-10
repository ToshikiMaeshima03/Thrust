/// エンジン設定
///
/// `run()` に渡してウィンドウやレンダリングの挙動をカスタマイズする。
/// デフォルト値はゲーム開発に適した設定。
pub struct EngineConfig {
    /// ウィンドウタイトル
    pub window_title: String,
    /// ウィンドウ初期サイズ (幅, 高さ)
    pub window_size: (u32, u32),
    /// 背景クリアカラー (RGBA, 0.0-1.0)
    pub clear_color: [f32; 4],
    /// VSync 有効
    pub vsync: bool,
    /// 省電力 GPU を優先するか
    pub low_power: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            window_title: "Thrust".to_string(),
            window_size: (1280, 720),
            clear_color: [0.1, 0.1, 0.12, 1.0],
            vsync: true,
            low_power: false,
        }
    }
}

impl EngineConfig {
    /// ウィンドウタイトルを設定
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.window_title = title.into();
        self
    }

    /// ウィンドウサイズを設定
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.window_size = (width, height);
        self
    }

    /// 背景クリアカラーを設定
    pub fn with_clear_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.clear_color = [r, g, b, a];
        self
    }

    /// VSync を設定
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.vsync = vsync;
        self
    }

    /// 省電力モードを設定
    pub fn with_low_power(mut self, low_power: bool) -> Self {
        self.low_power = low_power;
        self
    }

    pub(crate) fn present_mode(&self) -> wgpu::PresentMode {
        if self.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        }
    }

    pub(crate) fn power_preference(&self) -> wgpu::PowerPreference {
        if self.low_power {
            wgpu::PowerPreference::LowPower
        } else {
            wgpu::PowerPreference::HighPerformance
        }
    }
}
