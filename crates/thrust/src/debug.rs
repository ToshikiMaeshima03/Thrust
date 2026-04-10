/// デバッグ統計情報
///
/// `Resources.debug_stats` 経由でアクセス可能。
/// 毎フレーム自動更新される。
pub struct DebugStats {
    /// 現在の FPS (1秒ごとに更新)
    pub fps: f32,
    /// 直前フレームの所要時間 (ミリ秒)
    pub frame_time_ms: f32,
    /// FPS 計算用の内部カウンター
    frame_count: u32,
    elapsed_since_update: f32,
}

impl Default for DebugStats {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugStats {
    pub fn new() -> Self {
        Self {
            fps: 0.0,
            frame_time_ms: 0.0,
            frame_count: 0,
            elapsed_since_update: 0.0,
        }
    }

    /// フレームごとに呼び出す
    pub(crate) fn update(&mut self, dt: f32) {
        self.frame_time_ms = dt * 1000.0;
        self.frame_count += 1;
        self.elapsed_since_update += dt;

        // 1秒ごとに FPS を更新
        if self.elapsed_since_update >= 1.0 {
            self.fps = self.frame_count as f32 / self.elapsed_since_update;
            self.frame_count = 0;
            self.elapsed_since_update = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_zeroed() {
        let stats = DebugStats::new();
        assert!((stats.fps).abs() < 1e-5);
        assert!((stats.frame_time_ms).abs() < 1e-5);
    }

    #[test]
    fn test_update_frame_time() {
        let mut stats = DebugStats::new();
        stats.update(0.016);
        assert!((stats.frame_time_ms - 16.0).abs() < 0.1);
    }

    #[test]
    fn test_fps_updates_after_one_second() {
        let mut stats = DebugStats::new();
        // 62フレーム × 16.67ms > 1秒 → FPS 更新トリガー
        for _ in 0..62 {
            stats.update(1.0 / 60.0);
        }
        assert!(stats.fps > 55.0 && stats.fps < 65.0, "FPS: {}", stats.fps);
    }
}
