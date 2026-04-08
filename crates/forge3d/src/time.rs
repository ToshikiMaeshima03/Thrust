use std::time::Instant;

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
        self.delta = (now - self.last_frame).as_secs_f32();
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
