//! アニメーションステートマシン + ブレンドツリー (Round 6)
//!
//! UE 風の AnimationBlueprint の簡易版:
//! - 名前付き State (各 State は 1 つの KeyframeAnimation を参照)
//! - Transition (条件 bool で遷移)
//! - BlendTree (1D: `speed` パラメータで複数アニメを線形ブレンド)
//!
//! 使用例:
//! ```ignore
//! let mut sm = AnimationStateMachine::new("idle");
//! sm.add_state("idle", idle_anim);
//! sm.add_state("run", run_anim);
//! sm.add_transition("idle", "run", Condition::ParamGreater("speed".into(), 0.1));
//! sm.set_param("speed", 0.5);
//! ```

use std::collections::HashMap;

use hecs::World;

use crate::animation::keyframe::{KeyframeAnimation, KeyframeValues};
use crate::scene::transform::Transform;

/// ステートマシンのパラメータ値
#[derive(Debug, Clone, Copy)]
pub enum ParamValue {
    Float(f32),
    Bool(bool),
    Int(i32),
}

/// トランジション条件
#[derive(Debug, Clone)]
pub enum Condition {
    /// 常に true (無条件遷移)
    Always,
    /// パラメータ > 閾値
    ParamGreater(String, f32),
    /// パラメータ < 閾値
    ParamLess(String, f32),
    /// Bool パラメータが true
    ParamTrue(String),
    /// Bool パラメータが false
    ParamFalse(String),
    /// Int パラメータが値と一致
    ParamEquals(String, i32),
}

/// ステート間のトランジション
#[derive(Debug, Clone)]
pub struct Transition {
    pub to_state: String,
    pub condition: Condition,
    /// ブレンド時間 (秒)
    pub blend_duration: f32,
}

/// 1D ブレンドツリー: 1 つのパラメータで複数アニメを線形ブレンド
///
/// 例: speed 0 → idle, speed 0.5 → walk, speed 1.0 → run
pub struct BlendTree1D {
    pub param: String,
    /// (threshold, animation) のソート済みリスト
    pub samples: Vec<(f32, KeyframeAnimation)>,
}

impl BlendTree1D {
    pub fn new(param: impl Into<String>) -> Self {
        Self {
            param: param.into(),
            samples: Vec::new(),
        }
    }

    pub fn add_sample(&mut self, threshold: f32, anim: KeyframeAnimation) -> &mut Self {
        self.samples.push((threshold, anim));
        self.samples
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self
    }

    /// 現在のパラメータ値に基づいて Transform を評価する
    pub fn evaluate(&self, value: f32, time: f32) -> Option<Transform> {
        if self.samples.is_empty() {
            return None;
        }
        if self.samples.len() == 1 {
            return Some(evaluate_keyframe(&self.samples[0].1, time));
        }
        // 範囲外のクランプ
        if value <= self.samples[0].0 {
            return Some(evaluate_keyframe(&self.samples[0].1, time));
        }
        if value >= self.samples.last().unwrap().0 {
            return Some(evaluate_keyframe(&self.samples.last().unwrap().1, time));
        }
        // 2 つの隣接サンプルを見つけて補間
        for i in 0..self.samples.len() - 1 {
            let (t0, a0) = &self.samples[i];
            let (t1, a1) = &self.samples[i + 1];
            if value >= *t0 && value <= *t1 {
                let alpha = (value - t0) / (t1 - t0 + 1e-5);
                let tr0 = evaluate_keyframe(a0, time);
                let tr1 = evaluate_keyframe(a1, time);
                return Some(blend_transforms(&tr0, &tr1, alpha));
            }
        }
        None
    }
}

/// ステートの中身: 単一アニメか blend tree
pub enum StateContent {
    Single(KeyframeAnimation),
    Blend1D(BlendTree1D),
}

/// アニメーションステート
pub struct AnimationState {
    pub name: String,
    pub content: StateContent,
    pub transitions: Vec<Transition>,
    /// ローカル時間 (state enter 後の経過時間)
    pub local_time: f32,
}

/// アニメーションステートマシン (コンポーネント)
pub struct AnimationStateMachine {
    pub states: HashMap<String, AnimationState>,
    pub current: String,
    pub params: HashMap<String, ParamValue>,
    /// 遷移中のブレンド状態
    pub blending: Option<BlendState>,
}

pub struct BlendState {
    pub from_state: String,
    pub to_state: String,
    pub duration: f32,
    pub elapsed: f32,
}

impl AnimationStateMachine {
    pub fn new(initial: impl Into<String>) -> Self {
        Self {
            states: HashMap::new(),
            current: initial.into(),
            params: HashMap::new(),
            blending: None,
        }
    }

    pub fn add_state(&mut self, name: impl Into<String>, anim: KeyframeAnimation) -> &mut Self {
        let name = name.into();
        self.states.insert(
            name.clone(),
            AnimationState {
                name: name.clone(),
                content: StateContent::Single(anim),
                transitions: Vec::new(),
                local_time: 0.0,
            },
        );
        self
    }

    pub fn add_blend_state(
        &mut self,
        name: impl Into<String>,
        blend_tree: BlendTree1D,
    ) -> &mut Self {
        let name = name.into();
        self.states.insert(
            name.clone(),
            AnimationState {
                name: name.clone(),
                content: StateContent::Blend1D(blend_tree),
                transitions: Vec::new(),
                local_time: 0.0,
            },
        );
        self
    }

    pub fn add_transition(
        &mut self,
        from: &str,
        to: impl Into<String>,
        condition: Condition,
        blend_duration: f32,
    ) {
        if let Some(state) = self.states.get_mut(from) {
            state.transitions.push(Transition {
                to_state: to.into(),
                condition,
                blend_duration,
            });
        }
    }

    pub fn set_param_float(&mut self, name: impl Into<String>, value: f32) {
        self.params.insert(name.into(), ParamValue::Float(value));
    }

    pub fn set_param_bool(&mut self, name: impl Into<String>, value: bool) {
        self.params.insert(name.into(), ParamValue::Bool(value));
    }

    pub fn set_param_int(&mut self, name: impl Into<String>, value: i32) {
        self.params.insert(name.into(), ParamValue::Int(value));
    }

    pub fn float_param(&self, name: &str) -> f32 {
        match self.params.get(name) {
            Some(ParamValue::Float(v)) => *v,
            _ => 0.0,
        }
    }

    fn check_condition(&self, cond: &Condition) -> bool {
        match cond {
            Condition::Always => true,
            Condition::ParamGreater(n, t) => self.float_param(n) > *t,
            Condition::ParamLess(n, t) => self.float_param(n) < *t,
            Condition::ParamTrue(n) => matches!(self.params.get(n), Some(ParamValue::Bool(true))),
            Condition::ParamFalse(n) => matches!(self.params.get(n), Some(ParamValue::Bool(false))),
            Condition::ParamEquals(n, v) => {
                matches!(self.params.get(n), Some(ParamValue::Int(x)) if x == v)
            }
        }
    }

    /// 毎フレーム呼ばれる更新処理
    pub fn tick(&mut self, dt: f32) -> Option<Transform> {
        // 現在ステートのローカル時間を進める
        if let Some(state) = self.states.get_mut(&self.current) {
            state.local_time += dt;
        }

        // ブレンド中の elapsed を進める
        if let Some(blend) = &mut self.blending {
            blend.elapsed += dt;
            if blend.elapsed >= blend.duration {
                self.blending = None;
            }
        }

        // トランジション判定 (ブレンド中は追加トランジションなし)
        if self.blending.is_none() {
            // 現在ステートのトランジションを先にコピー (借用回避)
            let transitions = self
                .states
                .get(&self.current)
                .map(|s| s.transitions.clone())
                .unwrap_or_default();

            for t in &transitions {
                if self.check_condition(&t.condition) {
                    // 遷移開始
                    let from = self.current.clone();
                    let to = t.to_state.clone();
                    if let Some(new_state) = self.states.get_mut(&to) {
                        new_state.local_time = 0.0;
                    }
                    self.blending = Some(BlendState {
                        from_state: from,
                        to_state: to.clone(),
                        duration: t.blend_duration.max(0.001),
                        elapsed: 0.0,
                    });
                    self.current = to;
                    break;
                }
            }
        }

        // Transform 評価
        self.evaluate_current_transform()
    }

    fn evaluate_current_transform(&self) -> Option<Transform> {
        if let Some(blend) = &self.blending {
            let alpha = (blend.elapsed / blend.duration).clamp(0.0, 1.0);
            let from = self.evaluate_state(&blend.from_state);
            let to = self.evaluate_state(&blend.to_state);
            match (from, to) {
                (Some(f), Some(t)) => Some(blend_transforms(&f, &t, alpha)),
                (None, other) | (other, None) => other,
            }
        } else {
            self.evaluate_state(&self.current)
        }
    }

    fn evaluate_state(&self, name: &str) -> Option<Transform> {
        let state = self.states.get(name)?;
        match &state.content {
            StateContent::Single(anim) => Some(evaluate_keyframe(anim, state.local_time)),
            StateContent::Blend1D(tree) => {
                let value = self.float_param(&tree.param);
                tree.evaluate(value, state.local_time)
            }
        }
    }
}

/// KeyframeAnimation を時刻 t で評価して Transform を返す
fn evaluate_keyframe(anim: &KeyframeAnimation, time: f32) -> Transform {
    // ループ対応
    let t = if anim.duration > 0.0 {
        time.rem_euclid(anim.duration)
    } else {
        0.0
    };

    let mut transform = Transform::default();
    for track in &anim.tracks {
        match &track.values {
            KeyframeValues::Translation(vals) => {
                if let Some(v) = interp_vec3(&track.timestamps, vals, t) {
                    transform.translation = v;
                }
            }
            KeyframeValues::Rotation(vals) => {
                if let Some(v) = interp_quat(&track.timestamps, vals, t) {
                    transform.rotation = v;
                }
            }
            KeyframeValues::Scale(vals) => {
                if let Some(v) = interp_vec3(&track.timestamps, vals, t) {
                    transform.scale = v;
                }
            }
        }
    }
    transform
}

fn interp_vec3(timestamps: &[f32], values: &[glam::Vec3], t: f32) -> Option<glam::Vec3> {
    if timestamps.is_empty() || values.is_empty() || timestamps.len() != values.len() {
        return None;
    }
    if t <= timestamps[0] {
        return Some(values[0]);
    }
    if t >= *timestamps.last()? {
        return Some(*values.last()?);
    }
    for i in 0..timestamps.len() - 1 {
        if t >= timestamps[i] && t < timestamps[i + 1] {
            let span = timestamps[i + 1] - timestamps[i];
            let alpha = if span > 0.0 {
                (t - timestamps[i]) / span
            } else {
                0.0
            };
            return Some(values[i].lerp(values[i + 1], alpha));
        }
    }
    None
}

fn interp_quat(timestamps: &[f32], values: &[glam::Quat], t: f32) -> Option<glam::Quat> {
    if timestamps.is_empty() || values.is_empty() || timestamps.len() != values.len() {
        return None;
    }
    if t <= timestamps[0] {
        return Some(values[0]);
    }
    if t >= *timestamps.last()? {
        return Some(*values.last()?);
    }
    for i in 0..timestamps.len() - 1 {
        if t >= timestamps[i] && t < timestamps[i + 1] {
            let span = timestamps[i + 1] - timestamps[i];
            let alpha = if span > 0.0 {
                (t - timestamps[i]) / span
            } else {
                0.0
            };
            return Some(values[i].slerp(values[i + 1], alpha));
        }
    }
    None
}

fn blend_transforms(a: &Transform, b: &Transform, alpha: f32) -> Transform {
    Transform {
        translation: a.translation.lerp(b.translation, alpha),
        rotation: a.rotation.slerp(b.rotation, alpha),
        scale: a.scale.lerp(b.scale, alpha),
    }
}

/// ステートマシンを持つエンティティを毎フレーム更新するシステム
pub fn state_machine_system(world: &mut World, dt: f32) {
    for (transform, sm) in world.query_mut::<(&mut Transform, &mut AnimationStateMachine)>() {
        if let Some(new_transform) = sm.tick(dt) {
            *transform = new_transform;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::keyframe::KeyframeTrack;

    fn make_static_anim(pos: glam::Vec3) -> KeyframeAnimation {
        KeyframeAnimation::new(
            "static".into(),
            vec![KeyframeTrack {
                timestamps: vec![0.0, 1.0],
                values: KeyframeValues::Translation(vec![pos, pos]),
            }],
            1.0,
        )
    }

    #[test]
    fn test_state_machine_default_state() {
        let mut sm = AnimationStateMachine::new("idle");
        sm.add_state("idle", make_static_anim(glam::Vec3::ZERO));
        let t = sm.tick(0.016);
        assert!(t.is_some());
        assert!((t.unwrap().translation - glam::Vec3::ZERO).length() < 1e-5);
    }

    #[test]
    fn test_state_machine_transition() {
        let mut sm = AnimationStateMachine::new("idle");
        sm.add_state("idle", make_static_anim(glam::Vec3::ZERO));
        sm.add_state("run", make_static_anim(glam::Vec3::X));
        sm.add_transition(
            "idle",
            "run",
            Condition::ParamGreater("speed".into(), 0.1),
            0.0, // 即時遷移
        );
        sm.set_param_float("speed", 0.5);
        let _ = sm.tick(0.016);
        assert_eq!(sm.current, "run");
    }

    #[test]
    fn test_state_machine_no_transition_when_condition_false() {
        let mut sm = AnimationStateMachine::new("idle");
        sm.add_state("idle", make_static_anim(glam::Vec3::ZERO));
        sm.add_state("run", make_static_anim(glam::Vec3::X));
        sm.add_transition(
            "idle",
            "run",
            Condition::ParamGreater("speed".into(), 0.1),
            0.1,
        );
        sm.set_param_float("speed", 0.0);
        let _ = sm.tick(0.016);
        assert_eq!(sm.current, "idle");
    }

    #[test]
    fn test_state_machine_bool_condition() {
        let mut sm = AnimationStateMachine::new("idle");
        sm.add_state("idle", make_static_anim(glam::Vec3::ZERO));
        sm.add_state("jump", make_static_anim(glam::Vec3::Y));
        sm.add_transition("idle", "jump", Condition::ParamTrue("jumping".into()), 0.0);
        sm.set_param_bool("jumping", true);
        let _ = sm.tick(0.016);
        assert_eq!(sm.current, "jump");
    }

    #[test]
    fn test_blend_tree_1d_clamped_low() {
        let mut tree = BlendTree1D::new("speed");
        tree.add_sample(0.0, make_static_anim(glam::Vec3::ZERO));
        tree.add_sample(1.0, make_static_anim(glam::Vec3::X));
        // value = -0.5 はクランプされて 0.0 のサンプル (ZERO) を返す
        let t = tree.evaluate(-0.5, 0.0).unwrap();
        assert!((t.translation - glam::Vec3::ZERO).length() < 1e-5);
    }

    #[test]
    fn test_blend_tree_1d_middle() {
        let mut tree = BlendTree1D::new("speed");
        tree.add_sample(0.0, make_static_anim(glam::Vec3::ZERO));
        tree.add_sample(1.0, make_static_anim(glam::Vec3::X));
        // value = 0.5 → 中間ブレンド
        let t = tree.evaluate(0.5, 0.0).unwrap();
        assert!((t.translation - glam::Vec3::new(0.5, 0.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn test_blend_tree_1d_clamped_high() {
        let mut tree = BlendTree1D::new("speed");
        tree.add_sample(0.0, make_static_anim(glam::Vec3::ZERO));
        tree.add_sample(1.0, make_static_anim(glam::Vec3::X));
        let t = tree.evaluate(2.0, 0.0).unwrap();
        assert!((t.translation - glam::Vec3::X).length() < 1e-5);
    }

    #[test]
    fn test_blend_tree_three_samples() {
        let mut tree = BlendTree1D::new("speed");
        tree.add_sample(0.0, make_static_anim(glam::Vec3::ZERO));
        tree.add_sample(0.5, make_static_anim(glam::Vec3::new(2.0, 0.0, 0.0)));
        tree.add_sample(1.0, make_static_anim(glam::Vec3::new(4.0, 0.0, 0.0)));
        let t = tree.evaluate(0.75, 0.0).unwrap();
        // 0.5 と 1.0 の中間 → x = 3.0
        assert!((t.translation.x - 3.0).abs() < 1e-3);
    }

    #[test]
    fn test_blend_transforms() {
        let a = Transform::from_translation(glam::Vec3::ZERO);
        let b = Transform::from_translation(glam::Vec3::new(10.0, 0.0, 0.0));
        let blended = blend_transforms(&a, &b, 0.5);
        assert!((blended.translation - glam::Vec3::new(5.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_state_machine_blend_during_transition() {
        let mut sm = AnimationStateMachine::new("a");
        sm.add_state("a", make_static_anim(glam::Vec3::ZERO));
        sm.add_state("b", make_static_anim(glam::Vec3::new(10.0, 0.0, 0.0)));
        sm.add_transition("a", "b", Condition::Always, 1.0);
        // 最初の tick で遷移開始
        let _ = sm.tick(0.0);
        assert_eq!(sm.current, "b");
        assert!(sm.blending.is_some());
        // 0.5 秒後はブレンド 50%
        let t = sm.tick(0.5).unwrap();
        assert!((t.translation.x - 5.0).abs() < 0.5);
    }

    #[test]
    fn test_state_machine_blend_completes() {
        let mut sm = AnimationStateMachine::new("a");
        sm.add_state("a", make_static_anim(glam::Vec3::ZERO));
        sm.add_state("b", make_static_anim(glam::Vec3::new(10.0, 0.0, 0.0)));
        sm.add_transition("a", "b", Condition::Always, 0.1);
        let _ = sm.tick(0.0);
        let _ = sm.tick(0.2); // ブレンド完了
        assert!(sm.blending.is_none());
    }
}
