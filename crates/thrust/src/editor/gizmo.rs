//! Transform Gizmo (Round 9)
//!
//! 選択中エンティティの位置/回転/スケールをマウス操作で変更する 3D ギズモ。
//! 簡易版: 各軸ボタンを egui で表示し、ドラッグでオフセットを適用する。
//! 本格的な 3D ハンドル描画は別途専用パイプラインが必要だが、
//! ここでは数値スライダ + 軸別ドラッグで「動かせる」ことを優先。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

pub struct TransformGizmo {
    pub mode: GizmoMode,
    /// ドラッグ中の差分 (一時バッファ)
    pub drag_delta: glam::Vec3,
    /// スナップ間隔 (0 ならスナップなし)
    pub snap: f32,
}

impl Default for TransformGizmo {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Translate,
            drag_delta: glam::Vec3::ZERO,
            snap: 0.0,
        }
    }
}

impl TransformGizmo {
    /// 値をスナップする
    pub fn apply_snap(&self, value: f32) -> f32 {
        if self.snap > 1e-5 {
            (value / self.snap).round() * self.snap
        } else {
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_translate() {
        let g = TransformGizmo::default();
        assert_eq!(g.mode, GizmoMode::Translate);
        assert_eq!(g.snap, 0.0);
    }

    #[test]
    fn test_apply_snap_zero() {
        let g = TransformGizmo::default();
        assert!((g.apply_snap(1.234) - 1.234).abs() < 1e-5);
    }

    #[test]
    fn test_apply_snap_05() {
        let mut g = TransformGizmo::default();
        g.snap = 0.5;
        assert!((g.apply_snap(1.3) - 1.5).abs() < 1e-5);
        assert!((g.apply_snap(1.2) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_modes_distinct() {
        assert_ne!(GizmoMode::Translate, GizmoMode::Rotate);
        assert_ne!(GizmoMode::Rotate, GizmoMode::Scale);
    }
}
