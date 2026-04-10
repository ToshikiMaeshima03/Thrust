use glam::{Quat, Vec3};
use hecs::World;

use crate::scene::transform::Transform;

/// キーフレームアニメーションの値タイプ
#[derive(Debug, Clone)]
pub enum KeyframeValues {
    /// 位置キーフレーム
    Translation(Vec<Vec3>),
    /// 回転キーフレーム
    Rotation(Vec<Quat>),
    /// スケールキーフレーム
    Scale(Vec<Vec3>),
}

/// 一つの TRS プロパティに対するキーフレーム列
#[derive(Debug, Clone)]
pub struct KeyframeTrack {
    /// 各キーフレームのタイムスタンプ（秒）
    pub timestamps: Vec<f32>,
    /// キーフレーム値（timestamps と同じ長さ）
    pub values: KeyframeValues,
}

/// キーフレームアニメーションコンポーネント
///
/// glTF などからロードされたキーフレームデータを保持し、
/// 時間に応じて Transform を補間する。
pub struct KeyframeAnimation {
    /// アニメーション名
    pub name: String,
    /// Translation/Rotation/Scale のキーフレームトラック
    pub tracks: Vec<KeyframeTrack>,
    /// アニメーション全体の長さ（秒）
    pub duration: f32,
    /// 現在の再生時間
    pub elapsed: f32,
    /// ループ再生
    pub looping: bool,
    /// 再生速度倍率
    pub speed: f32,
    /// 再生中フラグ
    pub playing: bool,
}

impl KeyframeAnimation {
    /// 新しいキーフレームアニメーションを作成
    pub fn new(name: String, tracks: Vec<KeyframeTrack>, duration: f32) -> Self {
        Self {
            name,
            tracks,
            duration,
            elapsed: 0.0,
            looping: true,
            speed: 1.0,
            playing: true,
        }
    }

    /// ループ設定（ビルダーパターン）
    pub fn with_loop(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// 再生速度設定（ビルダーパターン）
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// アニメーションが完了したか
    pub fn is_finished(&self) -> bool {
        !self.looping && self.elapsed >= self.duration
    }
}

/// キーフレームアニメーションシステム: KeyframeAnimation を進行し、Transform を補間する
pub fn keyframe_animation_system(world: &mut World, dt: f32) {
    let mut finished: Vec<hecs::Entity> = Vec::new();

    for (entity, transform, anim) in
        world.query_mut::<(hecs::Entity, &mut Transform, &mut KeyframeAnimation)>()
    {
        if !anim.playing || anim.is_finished() {
            continue;
        }

        anim.elapsed += dt * anim.speed;

        if anim.elapsed >= anim.duration {
            if anim.looping {
                // %= ではなく減算で精度劣化を防止
                anim.elapsed -= anim.duration;
            } else {
                anim.elapsed = anim.duration;
                finished.push(entity);
            }
        }

        let t = anim.elapsed;

        // 各トラックを評価して Transform に適用
        for track in &anim.tracks {
            match &track.values {
                KeyframeValues::Translation(values) => {
                    transform.translation = interpolate_vec3(&track.timestamps, values, t);
                }
                KeyframeValues::Rotation(values) => {
                    transform.rotation = interpolate_quat(&track.timestamps, values, t);
                }
                KeyframeValues::Scale(values) => {
                    transform.scale = interpolate_vec3(&track.timestamps, values, t);
                }
            }
        }
    }

    // 完了したアニメーションを削除
    for entity in finished {
        let _ = world.remove_one::<KeyframeAnimation>(entity);
    }
}

/// タイムスタンプ列から二分探索で補間区間を見つけ、Vec3 を線形補間
fn interpolate_vec3(timestamps: &[f32], values: &[Vec3], t: f32) -> Vec3 {
    if timestamps.is_empty() || values.is_empty() || timestamps.len() != values.len() {
        return Vec3::ZERO;
    }
    if t <= timestamps[0] {
        return values[0];
    }
    let Some(&last_ts) = timestamps.last() else {
        return Vec3::ZERO;
    };
    let Some(&last_val) = values.last() else {
        return Vec3::ZERO;
    };
    if t >= last_ts {
        return last_val;
    }

    let idx = timestamps.partition_point(|&ts| ts <= t).saturating_sub(1);
    let next = (idx + 1).min(values.len() - 1);

    if idx == next {
        return values[idx];
    }

    let dt = timestamps[next] - timestamps[idx];
    let factor = if dt > 0.0 {
        (t - timestamps[idx]) / dt
    } else {
        0.0
    };

    Vec3::lerp(values[idx], values[next], factor)
}

/// タイムスタンプ列から二分探索で補間区間を見つけ、Quat を slerp
fn interpolate_quat(timestamps: &[f32], values: &[Quat], t: f32) -> Quat {
    if timestamps.is_empty() || values.is_empty() || timestamps.len() != values.len() {
        return Quat::IDENTITY;
    }
    if t <= timestamps[0] {
        return values[0];
    }
    let Some(&last_ts) = timestamps.last() else {
        return Quat::IDENTITY;
    };
    let Some(&last_val) = values.last() else {
        return Quat::IDENTITY;
    };
    if t >= last_ts {
        return last_val;
    }

    let idx = timestamps.partition_point(|&ts| ts <= t).saturating_sub(1);
    let next = (idx + 1).min(values.len() - 1);

    if idx == next {
        return values[idx];
    }

    let dt = timestamps[next] - timestamps[idx];
    let factor = if dt > 0.0 {
        (t - timestamps[idx]) / dt
    } else {
        0.0
    };

    Quat::slerp(values[idx], values[next], factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_vec3_basic() {
        let timestamps = vec![0.0, 1.0, 2.0];
        let values = vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ];

        // 開始前
        let v = interpolate_vec3(&timestamps, &values, -1.0);
        assert_eq!(v, Vec3::ZERO);

        // 中間
        let v = interpolate_vec3(&timestamps, &values, 0.5);
        assert!((v.x - 0.5).abs() < 1e-5);

        // 終了後
        let v = interpolate_vec3(&timestamps, &values, 3.0);
        assert_eq!(v, Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn test_interpolate_vec3_empty() {
        assert_eq!(interpolate_vec3(&[], &[], 0.5), Vec3::ZERO);
    }

    #[test]
    fn test_interpolate_vec3_single_keyframe() {
        let val = Vec3::new(1.0, 2.0, 3.0);
        let v = interpolate_vec3(&[0.0], &[val], 5.0);
        assert_eq!(v, val);
    }

    #[test]
    fn test_interpolate_vec3_mismatched_lengths() {
        // timestamps と values の長さが異なる場合は ZERO を返す
        let v = interpolate_vec3(&[0.0, 1.0], &[Vec3::ONE], 0.5);
        assert_eq!(v, Vec3::ZERO);
    }

    #[test]
    fn test_interpolate_quat_empty() {
        let q = interpolate_quat(&[], &[], 0.5);
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn test_interpolate_quat_single_keyframe() {
        let val = Quat::from_rotation_y(1.0);
        let q = interpolate_quat(&[0.0], &[val], 5.0);
        assert!(q.dot(val).abs() > 0.999);
    }

    #[test]
    fn test_interpolate_quat_mismatched_lengths() {
        let q = interpolate_quat(&[0.0, 1.0], &[Quat::IDENTITY], 0.5);
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn test_keyframe_animation_system_basic() {
        let mut world = World::new();
        let tracks = vec![KeyframeTrack {
            timestamps: vec![0.0, 1.0],
            values: KeyframeValues::Translation(vec![Vec3::ZERO, Vec3::X]),
        }];
        let anim = KeyframeAnimation::new("test".into(), tracks, 1.0).with_loop(false);
        let entity = world.spawn((Transform::default(), anim));

        keyframe_animation_system(&mut world, 0.5);

        let t = world.get::<&Transform>(entity).unwrap();
        assert!((t.translation.x - 0.5).abs() < 0.1);
    }

    #[test]
    fn test_keyframe_animation_finished_removes_component() {
        let mut world = World::new();
        let tracks = vec![KeyframeTrack {
            timestamps: vec![0.0, 1.0],
            values: KeyframeValues::Translation(vec![Vec3::ZERO, Vec3::X]),
        }];
        let anim = KeyframeAnimation::new("test".into(), tracks, 1.0).with_loop(false);
        let entity = world.spawn((Transform::default(), anim));

        // 2秒進めてアニメーション完了
        keyframe_animation_system(&mut world, 2.0);

        // KeyframeAnimation コンポーネントが削除されている
        assert!(world.get::<&KeyframeAnimation>(entity).is_err());
    }

    #[test]
    fn test_interpolate_quat_basic() {
        let timestamps = vec![0.0, 1.0];
        let values = vec![
            Quat::IDENTITY,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        ];

        let q = interpolate_quat(&timestamps, &values, 0.5);
        // 中間の回転は 45 度付近
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        assert!(q.dot(expected).abs() > 0.99);
    }
}
