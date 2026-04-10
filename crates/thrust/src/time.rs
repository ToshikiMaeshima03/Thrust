use std::time::Instant;

/// デルタタイムの上限（秒）
///
/// Alt-Tab やデバッガ停止時のスパイクによる物理・アニメーション崩壊を防止する。
const MAX_DELTA: f32 = 0.1;

pub struct Time {
    start: Instant,
    last_frame: Instant,
    delta: f32,
    elapsed: f32,
    frame_count: u64,
}

impl Default for Time {
    fn default() -> Self {
        Self::new()
    }
}

impl Time {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last_frame: now,
            delta: 0.0,
            elapsed: 0.0,
            frame_count: 0,
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let raw_delta = (now - self.last_frame).as_secs_f32();
        self.delta = raw_delta.min(MAX_DELTA);
        self.elapsed = (now - self.start).as_secs_f32();
        self.last_frame = now;
        self.frame_count += 1;
    }

    pub fn delta(&self) -> f32 {
        self.delta
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new_zeroed() {
        let time = Time::new();
        assert_eq!(time.delta(), 0.0);
        assert_eq!(time.frame_count(), 0);
    }

    #[test]
    fn test_tick_increments_frame_count() {
        let mut time = Time::new();
        time.tick();
        assert_eq!(time.frame_count(), 1);
        time.tick();
        assert_eq!(time.frame_count(), 2);
    }

    #[test]
    fn test_tick_updates_delta() {
        let mut time = Time::new();
        thread::sleep(Duration::from_millis(10));
        time.tick();
        assert!(time.delta() > 0.0, "デルタは正の値であるべき");
        assert!(time.delta() < 1.0, "デルタが異常に大きい: {}", time.delta());
    }

    #[test]
    fn test_tick_updates_elapsed() {
        let mut time = Time::new();
        thread::sleep(Duration::from_millis(10));
        time.tick();
        assert!(time.elapsed() > 0.0, "経過時間は正の値であるべき");
    }

    #[test]
    fn test_delta_capped_at_max() {
        // MAX_DELTA を超えるスパイクがキャップされることを検証
        let mut time = Time::new();
        // last_frame を過去に設定してスパイクを模擬
        time.last_frame = Instant::now() - Duration::from_millis(500);
        time.tick();
        assert!(
            time.delta() <= MAX_DELTA + 1e-6,
            "デルタが MAX_DELTA を超えている: {} > {MAX_DELTA}",
            time.delta()
        );
    }
}
