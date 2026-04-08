use std::f32::consts::PI;

use glam::{Quat, Vec3};
use hecs::World;

use crate::scene::transform::Transform;

/// イージング関数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EaseFunction {
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    SineIn,
    SineOut,
    SineInOut,
}

/// イージング関数を適用する（t: 0.0..=1.0）
pub fn ease(t: f32, func: EaseFunction) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match func {
        EaseFunction::Linear => t,
        EaseFunction::QuadIn => t * t,
        EaseFunction::QuadOut => t * (2.0 - t),
        EaseFunction::QuadInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        EaseFunction::CubicIn => t * t * t,
        EaseFunction::CubicOut => {
            let t1 = t - 1.0;
            t1 * t1 * t1 + 1.0
        }
        EaseFunction::CubicInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let t1 = 2.0 * t - 2.0;
                0.5 * t1 * t1 * t1 + 1.0
            }
        }
        EaseFunction::SineIn => 1.0 - (t * PI * 0.5).cos(),
        EaseFunction::SineOut => (t * PI * 0.5).sin(),
        EaseFunction::SineInOut => 0.5 * (1.0 - (PI * t).cos()),
    }
}

/// Transform アニメーションコンポーネント
///
/// start から end へ duration 秒かけて補間する。
/// Translation/Scale は lerp、Rotation は slerp。
pub struct TransformAnimation {
    pub start: Transform,
    pub end: Transform,
    pub duration: f32,
    pub elapsed: f32,
    pub ease_fn: EaseFunction,
    pub looping: bool,
    pub ping_pong: bool,
    forward: bool,
}

impl TransformAnimation {
    /// 新しいアニメーションを作成
    pub fn new(start: Transform, end: Transform, duration: f32) -> Self {
        Self {
            start,
            end,
            duration,
            elapsed: 0.0,
            ease_fn: EaseFunction::Linear,
            looping: false,
            ping_pong: false,
            forward: true,
        }
    }

    /// イージング関数を設定（ビルダーパターン）
    pub fn with_ease(mut self, ease_fn: EaseFunction) -> Self {
        self.ease_fn = ease_fn;
        self
    }

    /// ループを有効にする
    pub fn with_loop(mut self) -> Self {
        self.looping = true;
        self
    }

    /// ピンポン（往復）を有効にする
    pub fn with_ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self.looping = true;
        self
    }

    /// アニメーションが完了したか
    pub fn is_finished(&self) -> bool {
        !self.looping && self.elapsed >= self.duration
    }
}

fn lerp_transform(start: &Transform, end: &Transform, t: f32) -> Transform {
    Transform {
        translation: Vec3::lerp(start.translation, end.translation, t),
        rotation: Quat::slerp(start.rotation, end.rotation, t),
        scale: Vec3::lerp(start.scale, end.scale, t),
    }
}

/// アニメーションシステム: TransformAnimation を進行し、Transform を補間する
pub fn animation_system(world: &mut World, dt: f32) {
    let mut finished: Vec<hecs::Entity> = Vec::new();

    for (entity, transform, anim) in
        world.query_mut::<(hecs::Entity, &mut Transform, &mut TransformAnimation)>()
    {
        anim.elapsed += dt;

        if anim.elapsed >= anim.duration {
            if anim.looping {
                if anim.ping_pong {
                    anim.forward = !anim.forward;
                }
                anim.elapsed %= anim.duration;
            } else {
                anim.elapsed = anim.duration;
                finished.push(entity);
            }
        }

        let raw_t = anim.elapsed / anim.duration;
        let eased_t = ease(raw_t, anim.ease_fn);
        let t = if anim.forward { eased_t } else { 1.0 - eased_t };

        *transform = lerp_transform(&anim.start, &anim.end, t);
    }

    // 完了したアニメーションを削除
    for entity in finished {
        let _ = world.remove_one::<TransformAnimation>(entity);
    }
}
