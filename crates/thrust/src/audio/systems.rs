//! オーディオシステム (Round 4): リスナー同期 + エミッタ自動再生

use hecs::{Entity, World};

use crate::audio::{AudioEmitter, AudioListener, AudioManager};
use crate::ecs::components::ActiveCamera;
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

fn matrix_from(transform: &Transform, gt: Option<&GlobalTransform>) -> glam::Mat4 {
    match gt {
        Some(g) => g.0,
        None => transform.to_matrix(),
    }
}

/// アクティブカメラ + AudioListener マーカーから kira リスナーを更新する
///
/// 優先順位:
/// 1. `(AudioListener, Transform)` を持つエンティティ
/// 2. `(ActiveCamera, Transform)` を持つエンティティ
/// 3. `Camera` のみ (`position` フィールド)
pub fn audio_listener_system(world: &World, audio: &mut AudioManager) {
    // 1. AudioListener マーカー
    if let Some((_marker, transform, gt)) = world
        .query::<(&AudioListener, &Transform, Option<&GlobalTransform>)>()
        .iter()
        .next()
    {
        let matrix = matrix_from(transform, gt);
        let pos = matrix.w_axis.truncate();
        let (_, rot, _) = matrix.to_scale_rotation_translation();
        audio.update_listener(pos, rot);
        return;
    }

    // 2. ActiveCamera + Transform
    if let Some((_marker, transform, gt)) = world
        .query::<(&ActiveCamera, &Transform, Option<&GlobalTransform>)>()
        .iter()
        .next()
    {
        let matrix = matrix_from(transform, gt);
        let pos = matrix.w_axis.truncate();
        let (_, rot, _) = matrix.to_scale_rotation_translation();
        audio.update_listener(pos, rot);
        return;
    }

    // 3. Camera フォールバック (position のみ)
    if let Some(camera) = world
        .query::<&crate::camera::camera::Camera>()
        .iter()
        .next()
    {
        audio.update_listener(camera.position, glam::Quat::IDENTITY);
    }
}

/// AudioEmitter コンポーネントを処理し、auto_play フラグが立っていれば 1 度だけ再生する
pub fn audio_emitter_system(world: &mut World, audio: &mut AudioManager) {
    // 再生候補を収集 (借用衝突回避)
    let mut to_play: Vec<(Entity, glam::Vec3, f32, f32)> = Vec::new();
    for (entity, emitter, transform, gt) in world
        .query::<(Entity, &AudioEmitter, &Transform, Option<&GlobalTransform>)>()
        .iter()
    {
        if emitter.played || !emitter.auto_play {
            continue;
        }
        let pos = match gt {
            Some(g) => g.0.w_axis.truncate(),
            None => transform.translation,
        };
        to_play.push((entity, pos, emitter.min_distance, emitter.max_distance));
    }

    for (entity, pos, min_d, max_d) in to_play {
        // ソースを取得して再生
        let source = match world.get::<&AudioEmitter>(entity) {
            Ok(em) => em.source.clone(),
            Err(_) => continue,
        };
        if audio.play_spatial(&source, pos, min_d, max_d).is_ok()
            && let Ok(mut em) = world.get::<&mut AudioEmitter>(entity)
        {
            em.played = true;
        }
    }
}
