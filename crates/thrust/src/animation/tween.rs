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
                // %= ではなく減算で精度劣化を防止
                anim.elapsed -= anim.duration;
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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── イージング関数テスト ───

    #[test]
    fn test_ease_boundary_values() {
        // 全イージング: t=0 → 0, t=1 → 1
        let funcs = [
            EaseFunction::Linear,
            EaseFunction::QuadIn,
            EaseFunction::QuadOut,
            EaseFunction::QuadInOut,
            EaseFunction::CubicIn,
            EaseFunction::CubicOut,
            EaseFunction::CubicInOut,
            EaseFunction::SineIn,
            EaseFunction::SineOut,
            EaseFunction::SineInOut,
        ];
        for f in funcs {
            assert!((ease(0.0, f) - 0.0).abs() < 1e-5, "{f:?} at t=0");
            assert!((ease(1.0, f) - 1.0).abs() < 1e-5, "{f:?} at t=1");
        }
    }

    #[test]
    fn test_ease_clamping() {
        // t < 0 は 0 にクランプ、t > 1 は 1 にクランプ
        assert!((ease(-0.5, EaseFunction::Linear) - 0.0).abs() < 1e-5);
        assert!((ease(1.5, EaseFunction::Linear) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_ease_linear_midpoint() {
        assert!((ease(0.5, EaseFunction::Linear) - 0.5).abs() < 1e-5);
        assert!((ease(0.25, EaseFunction::Linear) - 0.25).abs() < 1e-5);
    }

    #[test]
    fn test_ease_quad_in_slow_start() {
        // QuadIn は t=0.5 で 0.25（遅い開始）
        assert!((ease(0.5, EaseFunction::QuadIn) - 0.25).abs() < 1e-5);
    }

    #[test]
    fn test_ease_quad_out_fast_start() {
        // QuadOut は t=0.5 で 0.75（速い開始）
        assert!((ease(0.5, EaseFunction::QuadOut) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn test_ease_monotonic() {
        // 全イージング関数が単調増加であることを確認
        let funcs = [
            EaseFunction::Linear,
            EaseFunction::QuadIn,
            EaseFunction::QuadOut,
            EaseFunction::QuadInOut,
            EaseFunction::CubicIn,
            EaseFunction::CubicOut,
            EaseFunction::CubicInOut,
            EaseFunction::SineIn,
            EaseFunction::SineOut,
            EaseFunction::SineInOut,
        ];
        for f in funcs {
            let mut prev = ease(0.0, f);
            for i in 1..=100 {
                let t = i as f32 / 100.0;
                let val = ease(t, f);
                assert!(val >= prev - 1e-6, "{f:?} not monotonic at t={t}");
                prev = val;
            }
        }
    }

    #[test]
    fn test_ease_sine_inout_midpoint() {
        // SineInOut は t=0.5 で 0.5
        assert!((ease(0.5, EaseFunction::SineInOut) - 0.5).abs() < 1e-5);
    }

    // ─── TransformAnimation テスト ───

    #[test]
    fn test_animation_is_finished() {
        let start = Transform::default();
        let end = Transform::from_translation(Vec3::X);
        let mut anim = TransformAnimation::new(start, end, 1.0);
        assert!(!anim.is_finished());

        anim.elapsed = 1.0;
        assert!(anim.is_finished());
    }

    #[test]
    fn test_animation_looping_not_finished() {
        let start = Transform::default();
        let end = Transform::from_translation(Vec3::X);
        let mut anim = TransformAnimation::new(start, end, 1.0).with_loop();
        anim.elapsed = 2.0;
        assert!(!anim.is_finished());
    }

    #[test]
    fn test_animation_builder_chain() {
        let anim = TransformAnimation::new(Transform::default(), Transform::default(), 1.0)
            .with_ease(EaseFunction::CubicInOut)
            .with_ping_pong();
        assert_eq!(anim.ease_fn, EaseFunction::CubicInOut);
        assert!(anim.ping_pong);
        assert!(anim.looping); // ping_pong は looping を自動的に有効にする
    }

    // ─── animation_system テスト ───

    #[test]
    fn test_animation_system_basic() {
        let mut world = World::new();
        let start = Transform::default();
        let end = Transform::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let entity = world.spawn((start.clone(), TransformAnimation::new(start, end, 1.0)));

        // 50% 進行
        animation_system(&mut world, 0.5);
        {
            let t = world.get::<&Transform>(entity).unwrap();
            assert!((t.translation.x - 5.0).abs() < 1e-4);
        }

        // 100% → アニメーション削除
        animation_system(&mut world, 0.5);
        {
            let t = world.get::<&Transform>(entity).unwrap();
            assert!((t.translation.x - 10.0).abs() < 1e-4);
        }
        assert!(world.get::<&TransformAnimation>(entity).is_err());
    }

    #[test]
    fn test_animation_system_looping() {
        let mut world = World::new();
        let start = Transform::default();
        let end = Transform::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let entity = world.spawn((
            start.clone(),
            TransformAnimation::new(start, end, 1.0).with_loop(),
        ));

        // 150% → ループで 50%
        animation_system(&mut world, 1.5);
        {
            let t = world.get::<&Transform>(entity).unwrap();
            assert!((t.translation.x - 5.0).abs() < 1e-4);
        }
        // アニメーションは削除されない
        assert!(world.get::<&TransformAnimation>(entity).is_ok());
    }

    #[test]
    fn test_animation_system_ping_pong() {
        let mut world = World::new();
        let start = Transform::default();
        let end = Transform::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let entity = world.spawn((
            start.clone(),
            TransformAnimation::new(start, end, 1.0).with_ping_pong(),
        ));

        // 1回目のサイクル完了 → forward が反転
        animation_system(&mut world, 1.0);
        {
            let anim = world.get::<&TransformAnimation>(entity).unwrap();
            assert!(!anim.forward);
        }
    }
}
