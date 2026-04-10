use std::collections::HashSet;

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct Input {
    keys_held: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,

    mouse_held: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    mouse_released: HashSet<MouseButton>,

    mouse_position: (f64, f64),
    mouse_delta: (f64, f64),
    scroll_delta: f32,

    prev_mouse_position: Option<(f64, f64)>,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            mouse_held: HashSet::new(),
            mouse_pressed: HashSet::new(),
            mouse_released: HashSet::new(),
            mouse_position: (0.0, 0.0),
            mouse_delta: (0.0, 0.0),
            scroll_delta: 0.0,
            prev_mouse_position: None,
        }
    }

    /// フレーム単位の状態をリセット（フレーム末尾で呼び出す）
    pub fn begin_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_pressed.clear();
        self.mouse_released.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll_delta = 0.0;
    }

    /// winit イベントを処理して入力状態を更新
    pub fn process_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                self.keys_pressed.insert(key);
                            }
                            self.keys_held.insert(key);
                        }
                        ElementState::Released => {
                            self.keys_released.insert(key);
                            self.keys_held.remove(&key);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    self.mouse_pressed.insert(*button);
                    self.mouse_held.insert(*button);
                }
                ElementState::Released => {
                    self.mouse_released.insert(*button);
                    self.mouse_held.remove(button);
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = (position.x, position.y);
                if let Some(prev) = self.prev_mouse_position {
                    self.mouse_delta.0 += new_pos.0 - prev.0;
                    self.mouse_delta.1 += new_pos.1 - prev.1;
                }
                self.prev_mouse_position = Some(new_pos);
                self.mouse_position = new_pos;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.scroll_delta += match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                };
            }
            _ => {}
        }
    }

    // キーボードクエリ

    pub fn is_key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    // マウスクエリ

    pub fn is_mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_held.contains(&button)
    }

    pub fn is_mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_pressed.contains(&button)
    }

    pub fn is_mouse_released(&self, button: MouseButton) -> bool {
        self.mouse_released.contains(&button)
    }

    pub fn mouse_position(&self) -> (f64, f64) {
        self.mouse_position
    }

    pub fn mouse_delta(&self) -> (f64, f64) {
        self.mouse_delta
    }

    pub fn scroll_delta(&self) -> f32 {
        self.scroll_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let input = Input::new();
        assert!(!input.is_key_held(KeyCode::KeyA));
        assert!(!input.is_key_pressed(KeyCode::KeyA));
        assert!(!input.is_mouse_held(MouseButton::Left));
        assert_eq!(input.mouse_position(), (0.0, 0.0));
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
        assert!((input.scroll_delta()).abs() < 1e-5);
    }

    #[test]
    fn test_begin_frame_clears_per_frame_state() {
        let mut input = Input::new();
        input.keys_pressed.insert(KeyCode::KeyA);
        input.keys_released.insert(KeyCode::KeyB);
        input.mouse_pressed.insert(MouseButton::Left);
        input.mouse_released.insert(MouseButton::Right);
        input.mouse_delta = (10.0, 20.0);
        input.scroll_delta = 3.0;

        input.begin_frame();

        assert!(!input.is_key_pressed(KeyCode::KeyA));
        assert!(!input.is_key_released(KeyCode::KeyB));
        assert!(!input.is_mouse_pressed(MouseButton::Left));
        assert!(!input.is_mouse_released(MouseButton::Right));
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
        assert!((input.scroll_delta()).abs() < 1e-5);
    }

    #[test]
    fn test_key_held_persists_across_frames() {
        let mut input = Input::new();
        input.keys_held.insert(KeyCode::KeyW);

        input.begin_frame();
        // held はフレーム間で維持される
        assert!(input.is_key_held(KeyCode::KeyW));
    }

    #[test]
    fn test_mouse_held_persists_across_frames() {
        let mut input = Input::new();
        input.mouse_held.insert(MouseButton::Left);

        input.begin_frame();
        assert!(input.is_mouse_held(MouseButton::Left));
    }

    // Note: winit 0.30 の WindowEvent 構築はプラットフォーム固有のフィールドを要求するため、
    // 内部状態の直接操作でキー/マウスの状態遷移をテストする。

    #[test]
    fn test_key_press_and_release_lifecycle() {
        let mut input = Input::new();

        // 押下をシミュレート
        input.keys_pressed.insert(KeyCode::KeyA);
        input.keys_held.insert(KeyCode::KeyA);

        assert!(input.is_key_pressed(KeyCode::KeyA));
        assert!(input.is_key_held(KeyCode::KeyA));
        assert!(!input.is_key_released(KeyCode::KeyA));

        // フレーム切り替え
        input.begin_frame();
        assert!(!input.is_key_pressed(KeyCode::KeyA));
        assert!(input.is_key_held(KeyCode::KeyA));

        // 離すをシミュレート
        input.keys_released.insert(KeyCode::KeyA);
        input.keys_held.remove(&KeyCode::KeyA);

        assert!(input.is_key_released(KeyCode::KeyA));
        assert!(!input.is_key_held(KeyCode::KeyA));
    }

    #[test]
    fn test_mouse_button_lifecycle() {
        let mut input = Input::new();

        // 押下
        input.mouse_pressed.insert(MouseButton::Left);
        input.mouse_held.insert(MouseButton::Left);
        assert!(input.is_mouse_pressed(MouseButton::Left));
        assert!(input.is_mouse_held(MouseButton::Left));

        input.begin_frame();
        assert!(!input.is_mouse_pressed(MouseButton::Left));
        assert!(input.is_mouse_held(MouseButton::Left));

        // 離す
        input.mouse_released.insert(MouseButton::Left);
        input.mouse_held.remove(&MouseButton::Left);
        assert!(input.is_mouse_released(MouseButton::Left));
        assert!(!input.is_mouse_held(MouseButton::Left));
    }

    #[test]
    fn test_mouse_delta_accumulation() {
        let mut input = Input::new();

        // 直接デルタ設定でフレーム内の蓄積をテスト
        input.mouse_delta = (10.0, 5.0);
        assert_eq!(input.mouse_delta(), (10.0, 5.0));

        input.begin_frame();
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
    }

    #[test]
    fn test_scroll_delta_reset_per_frame() {
        let mut input = Input::new();
        input.scroll_delta = 2.5;
        assert!((input.scroll_delta() - 2.5).abs() < 1e-5);

        input.begin_frame();
        assert!((input.scroll_delta()).abs() < 1e-5);
    }

    #[test]
    fn test_multiple_keys_independent() {
        let mut input = Input::new();
        input.keys_held.insert(KeyCode::KeyW);
        input.keys_held.insert(KeyCode::KeyS);
        input.keys_pressed.insert(KeyCode::KeyW);

        assert!(input.is_key_held(KeyCode::KeyW));
        assert!(input.is_key_held(KeyCode::KeyS));
        assert!(input.is_key_pressed(KeyCode::KeyW));
        assert!(!input.is_key_pressed(KeyCode::KeyS));
    }

    #[test]
    fn test_full_frame_cycle() {
        let mut input = Input::new();

        // フレーム1: キー押下
        input.keys_pressed.insert(KeyCode::KeyW);
        input.keys_held.insert(KeyCode::KeyW);
        assert!(input.is_key_pressed(KeyCode::KeyW));
        assert!(input.is_key_held(KeyCode::KeyW));

        // フレーム2: begin_frame でリセット
        input.begin_frame();
        assert!(!input.is_key_pressed(KeyCode::KeyW)); // pressed はクリア
        assert!(input.is_key_held(KeyCode::KeyW)); // held は維持
    }
}
