//! スケルタルアニメーション (Round 4)
//!
//! glTF skin パース → joint hierarchy → skin matrix SSBO → 頂点シェーダー skinning。
//! 既存 KeyframeAnimation を joint エンティティに付与すれば自動でアニメする。

use glam::Mat4;
use hecs::{Entity, World};

use crate::ecs::components::RenderState;
use crate::ecs::resources::Resources;
use crate::scene::hierarchy::GlobalTransform;

/// スケルトン: 関節エンティティのリストとインバースバインド行列
pub struct Skeleton {
    /// 関節エンティティ (順序が SkinnedMesh の joints に対応)
    pub joint_entities: Vec<Entity>,
    /// インバースバインド行列 (joint ローカル空間 ← メッシュ空間)
    pub inverse_bind_matrices: Vec<Mat4>,
}

/// 関節マーカー: スケルトンの何番目の関節か
pub struct Joint {
    pub skeleton: Entity,
    pub index: usize,
}

/// スキンメッシュ: スケルトン参照と現在の skin matrix キャッシュ
pub struct SkinnedMesh {
    pub skeleton: Entity,
    /// 現在の joint matrices (`skin_system` が更新)
    pub joint_matrices: Vec<Mat4>,
}

impl SkinnedMesh {
    pub fn new(skeleton: Entity, joint_count: usize) -> Self {
        Self {
            skeleton,
            joint_matrices: vec![Mat4::IDENTITY; joint_count],
        }
    }
}

/// スキンシステム: 各 SkinnedMesh について joint matrices を計算する
///
/// `propagate_transforms()` の後に呼ぶ必要がある (GlobalTransform を読むため)。
/// CPU 側のキャッシュ更新のみ。GPU アップロードは `skin_upload_system` が担当。
pub fn skin_system(world: &mut World) {
    // SkinnedMesh のリストを収集 (借用衝突回避)
    let skinned_entities: Vec<(Entity, Entity)> = world
        .query::<(Entity, &SkinnedMesh)>()
        .iter()
        .map(|(e, sm)| (e, sm.skeleton))
        .collect();

    for (mesh_entity, skel_entity) in skinned_entities {
        // スケルトン情報を取得
        let (joint_entities, inverse_binds) = {
            let Ok(skel_ref) = world.entity(skel_entity) else {
                continue;
            };
            let Some(skel) = skel_ref.get::<&Skeleton>() else {
                continue;
            };
            (
                skel.joint_entities.clone(),
                skel.inverse_bind_matrices.clone(),
            )
        };

        let joint_count = joint_entities.len().min(inverse_binds.len());
        let mut matrices = Vec::with_capacity(joint_count);

        for i in 0..joint_count {
            let joint_entity = joint_entities[i];
            let global = world
                .get::<&GlobalTransform>(joint_entity)
                .map(|gt| gt.0)
                .unwrap_or(Mat4::IDENTITY);
            matrices.push(global * inverse_binds[i]);
        }

        // 結果を SkinnedMesh に書き戻す
        if let Ok(mut sm) = world.get::<&mut SkinnedMesh>(mesh_entity) {
            sm.joint_matrices = matrices;
        }
    }
}

/// SkinnedMesh の joint_matrices を GPU バッファにアップロードする
///
/// `render_prep_system` の直後 / `render_system` の直前に実行する。
pub fn skin_upload_system(world: &mut World, res: &mut Resources) {
    for (sm, render_state) in world.query_mut::<(&SkinnedMesh, &RenderState)>() {
        if !render_state.owns_joint_buffer {
            continue;
        }
        if sm.joint_matrices.is_empty() {
            continue;
        }
        let raw: Vec<[[f32; 4]; 4]> = sm
            .joint_matrices
            .iter()
            .map(|m: &Mat4| m.to_cols_array_2d())
            .collect();
        res.gpu
            .queue
            .write_buffer(&render_state.joint_buffer, 0, bytemuck::cast_slice(&raw));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::transform::Transform;

    #[test]
    fn test_skinned_mesh_new() {
        let dummy_entity = hecs::Entity::from_bits(0x100000001).unwrap();
        let sm = SkinnedMesh::new(dummy_entity, 5);
        assert_eq!(sm.joint_matrices.len(), 5);
        for m in &sm.joint_matrices {
            assert_eq!(*m, Mat4::IDENTITY);
        }
    }

    #[test]
    fn test_skin_system_identity() {
        let mut world = World::new();
        // 単純なスケルトン: 1 つの関節、識別バインド行列
        let joint_entity = world.spawn((Transform::default(), GlobalTransform(Mat4::IDENTITY)));
        let skel_entity = world.spawn((Skeleton {
            joint_entities: vec![joint_entity],
            inverse_bind_matrices: vec![Mat4::IDENTITY],
        },));
        let mesh_entity = world.spawn((SkinnedMesh::new(skel_entity, 1),));

        skin_system(&mut world);

        let sm = world.get::<&SkinnedMesh>(mesh_entity).unwrap();
        assert_eq!(sm.joint_matrices.len(), 1);
        assert_eq!(sm.joint_matrices[0], Mat4::IDENTITY);
    }

    #[test]
    fn test_skin_system_translation() {
        let mut world = World::new();
        // 関節が +X 方向に 1 単位移動
        let joint_global = Mat4::from_translation(glam::Vec3::X);
        let joint_entity = world.spawn((Transform::default(), GlobalTransform(joint_global)));
        let skel_entity = world.spawn((Skeleton {
            joint_entities: vec![joint_entity],
            inverse_bind_matrices: vec![Mat4::IDENTITY],
        },));
        let mesh_entity = world.spawn((SkinnedMesh::new(skel_entity, 1),));

        skin_system(&mut world);

        let sm = world.get::<&SkinnedMesh>(mesh_entity).unwrap();
        // global * IBM = +X translation
        let expected = Mat4::from_translation(glam::Vec3::X);
        assert!((sm.joint_matrices[0].x_axis - expected.x_axis).length() < 1e-5);
        assert!((sm.joint_matrices[0].w_axis - expected.w_axis).length() < 1e-5);
    }

    #[test]
    fn test_skin_system_inverse_bind() {
        let mut world = World::new();
        // global = T(2, 0, 0), IBM = T(-2, 0, 0) → 結果は IDENTITY
        let global = Mat4::from_translation(glam::Vec3::new(2.0, 0.0, 0.0));
        let ibm = Mat4::from_translation(glam::Vec3::new(-2.0, 0.0, 0.0));
        let joint_entity = world.spawn((Transform::default(), GlobalTransform(global)));
        let skel_entity = world.spawn((Skeleton {
            joint_entities: vec![joint_entity],
            inverse_bind_matrices: vec![ibm],
        },));
        let mesh_entity = world.spawn((SkinnedMesh::new(skel_entity, 1),));

        skin_system(&mut world);

        let sm = world.get::<&SkinnedMesh>(mesh_entity).unwrap();
        let result = sm.joint_matrices[0];
        // T(2) * T(-2) = identity
        assert!((result.w_axis - glam::Vec4::new(0.0, 0.0, 0.0, 1.0)).length() < 1e-5);
    }
}
