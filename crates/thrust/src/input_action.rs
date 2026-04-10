//! 入力アクションマップ (Round 8)
//!
//! 物理キー/マウスボタンを名前付きアクションに束ねる。
//! `Action`: ボタン (押下/離す) と `Axis`: 軸入力 (-1..1) をサポート。
//! ゲームコード側は `is_action_pressed("Jump")` のように扱える。

use std::collections::HashMap;

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use crate::input::Input;

/// アクションのバインディング (1 つ以上のキーを bind 可能)
#[derive(Debug, Clone, Default)]
pub struct ActionBinding {
    pub keys: Vec<KeyCode>,
    pub mouse_buttons: Vec<MouseButton>,
}

/// 軸入力のバインディング (positive/negative の 2 方向)
#[derive(Debug, Clone, Default)]
pub struct AxisBinding {
    pub positive_keys: Vec<KeyCode>,
    pub negative_keys: Vec<KeyCode>,
}

/// 入力アクションマップ
#[derive(Debug, Default)]
pub struct InputActionMap {
    actions: HashMap<String, ActionBinding>,
    axes: HashMap<String, AxisBinding>,
}

impl InputActionMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// アクションを登録 (キー)
    pub fn bind_action_key(&mut self, action: impl Into<String>, key: KeyCode) {
        self.actions
            .entry(action.into())
            .or_default()
            .keys
            .push(key);
    }

    /// アクションを登録 (マウスボタン)
    pub fn bind_action_mouse(&mut self, action: impl Into<String>, button: MouseButton) {
        self.actions
            .entry(action.into())
            .or_default()
            .mouse_buttons
            .push(button);
    }

    /// 軸を登録 (正負方向のキーペア)
    pub fn bind_axis_keys(
        &mut self,
        axis: impl Into<String>,
        positive: KeyCode,
        negative: KeyCode,
    ) {
        let entry = self.axes.entry(axis.into()).or_default();
        entry.positive_keys.push(positive);
        entry.negative_keys.push(negative);
    }

    /// アクションが現在押されているか
    pub fn is_action_held(&self, action: &str, input: &Input) -> bool {
        let Some(b) = self.actions.get(action) else {
            return false;
        };
        for k in &b.keys {
            if input.is_key_held(*k) {
                return true;
            }
        }
        for m in &b.mouse_buttons {
            if input.is_mouse_held(*m) {
                return true;
            }
        }
        false
    }

    /// 今フレームでアクションが押された (押下イベント) か
    pub fn is_action_pressed(&self, action: &str, input: &Input) -> bool {
        let Some(b) = self.actions.get(action) else {
            return false;
        };
        for k in &b.keys {
            if input.is_key_pressed(*k) {
                return true;
            }
        }
        for m in &b.mouse_buttons {
            if input.is_mouse_pressed(*m) {
                return true;
            }
        }
        false
    }

    /// 軸の値を取得 (-1.0..1.0)
    pub fn axis_value(&self, axis: &str, input: &Input) -> f32 {
        let Some(b) = self.axes.get(axis) else {
            return 0.0;
        };
        let mut v: f32 = 0.0;
        for k in &b.positive_keys {
            if input.is_key_held(*k) {
                v += 1.0;
            }
        }
        for k in &b.negative_keys {
            if input.is_key_held(*k) {
                v -= 1.0;
            }
        }
        v.clamp(-1.0, 1.0)
    }

    /// 2D 軸 (XY) を取得
    pub fn axis2d(&self, axis_x: &str, axis_y: &str, input: &Input) -> glam::Vec2 {
        glam::Vec2::new(
            self.axis_value(axis_x, input),
            self.axis_value(axis_y, input),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_and_lookup() {
        let mut m = InputActionMap::new();
        m.bind_action_key("Jump", KeyCode::Space);
        assert!(m.actions.contains_key("Jump"));
        assert_eq!(m.actions["Jump"].keys.len(), 1);
    }

    #[test]
    fn test_bind_axis() {
        let mut m = InputActionMap::new();
        m.bind_axis_keys("MoveX", KeyCode::KeyD, KeyCode::KeyA);
        assert!(m.axes.contains_key("MoveX"));
    }

    #[test]
    fn test_action_not_pressed_no_binding() {
        let m = InputActionMap::new();
        let input = Input::new();
        assert!(!m.is_action_pressed("Jump", &input));
    }

    #[test]
    fn test_axis_value_zero_no_input() {
        let mut m = InputActionMap::new();
        m.bind_axis_keys("MoveX", KeyCode::KeyD, KeyCode::KeyA);
        let input = Input::new();
        assert_eq!(m.axis_value("MoveX", &input), 0.0);
    }

    #[test]
    fn test_multiple_bindings_per_action() {
        let mut m = InputActionMap::new();
        m.bind_action_key("Jump", KeyCode::Space);
        m.bind_action_mouse("Jump", MouseButton::Left);
        assert_eq!(m.actions["Jump"].keys.len(), 1);
        assert_eq!(m.actions["Jump"].mouse_buttons.len(), 1);
    }
}
