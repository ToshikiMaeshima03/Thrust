use hecs::{Entity, World};

/// 親エンティティへの参照
pub struct Parent(pub Entity);

/// 子エンティティのリスト（キャッシュ、Parent が正の情報源）
pub struct Children(pub Vec<Entity>);

/// ワールド空間の変換行列（ローカル Transform + 親チェーンから計算）
pub struct GlobalTransform(pub glam::Mat4);

impl Default for GlobalTransform {
    fn default() -> Self {
        Self(glam::Mat4::IDENTITY)
    }
}

/// 子エンティティを親に追加する
///
/// 子に Parent コンポーネントを設定し、親の Children リストに追加する。
/// GlobalTransform がなければ自動的に追加される。
pub fn set_parent(world: &mut World, child: Entity, parent: Entity) {
    // 子に Parent と GlobalTransform を設定
    if world.get::<&Parent>(child).is_ok() {
        // 既に親がいる場合は古い親の Children から除去
        if let Ok(old_parent_ref) = world.get::<&Parent>(child) {
            let old_parent = old_parent_ref.0;
            drop(old_parent_ref);
            if let Ok(mut children) = world.get::<&mut Children>(old_parent) {
                children.0.retain(|&e| e != child);
            }
        }
        let _ = world.remove_one::<Parent>(child);
    }

    let _ = world.insert(child, (Parent(parent), GlobalTransform::default()));

    // 親の Children リストを更新
    if let Ok(mut children) = world.get::<&mut Children>(parent) {
        if !children.0.contains(&child) {
            children.0.push(child);
        }
    } else {
        let _ = world.insert_one(parent, Children(vec![child]));
    }
}

/// Transform 階層をルートから子へ再帰的に伝播する
///
/// camera_system の後、render_prep_system の前に呼び出す。
pub fn propagate_transforms(world: &mut World) {
    use crate::scene::transform::Transform;

    // Step 1: ルートエンティティ（Parent を持たない）の GlobalTransform を計算
    let roots: Vec<Entity> = world
        .query::<(Entity, &Transform)>()
        .without::<&Parent>()
        .iter()
        .map(|(entity, _t)| entity)
        .collect();

    for entity in roots {
        let matrix = {
            let Ok(t) = world.get::<&Transform>(entity) else {
                continue;
            };
            t.to_matrix()
        };

        // GlobalTransform を設定（なければ挿入）
        if let Ok(mut gt) = world.get::<&mut GlobalTransform>(entity) {
            gt.0 = matrix;
        } else {
            let _ = world.insert_one(entity, GlobalTransform(matrix));
        }

        // 子を反復的に処理
        propagate_children(world, entity, matrix);
    }
}

/// 階層伝播の最大深度（スタックオーバーフロー防止）
const MAX_HIERARCHY_DEPTH: u32 = 256;

/// 子エンティティの GlobalTransform を反復的に伝播する
fn propagate_children(world: &mut World, root: Entity, root_global: glam::Mat4) {
    use crate::scene::transform::Transform;

    // 明示的スタック: (エンティティ, 親のグローバル行列, 深度)
    let mut stack: Vec<(Entity, glam::Mat4, u32)> = vec![(root, root_global, 0)];

    while let Some((parent_entity, parent_global, depth)) = stack.pop() {
        if depth >= MAX_HIERARCHY_DEPTH {
            log::warn!("階層深度上限 ({MAX_HIERARCHY_DEPTH}) に到達。子の伝播を中断します");
            continue;
        }

        let children: Vec<Entity> = match world.get::<&Children>(parent_entity) {
            Ok(c) => c.0.clone(),
            Err(_) => continue,
        };

        for child in children {
            let child_matrix = {
                let Ok(t) = world.get::<&Transform>(child) else {
                    continue;
                };
                t.to_matrix()
            };

            let global = parent_global * child_matrix;

            if let Ok(mut gt) = world.get::<&mut GlobalTransform>(child) {
                gt.0 = global;
            } else {
                let _ = world.insert_one(child, GlobalTransform(global));
            }

            stack.push((child, global, depth + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::transform::Transform;

    #[test]
    fn test_set_parent_basic() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child = world.spawn((Transform::default(),));

        set_parent(&mut world, child, parent);

        // 子に Parent が設定される
        let p = world.get::<&Parent>(child).unwrap();
        assert_eq!(p.0, parent);

        // 親に Children が設定される
        let c = world.get::<&Children>(parent).unwrap();
        assert_eq!(c.0.len(), 1);
        assert_eq!(c.0[0], child);

        // 子に GlobalTransform が追加される
        assert!(world.get::<&GlobalTransform>(child).is_ok());
    }

    #[test]
    fn test_set_parent_multiple_children() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child1 = world.spawn((Transform::default(),));
        let child2 = world.spawn((Transform::default(),));

        set_parent(&mut world, child1, parent);
        set_parent(&mut world, child2, parent);

        let c = world.get::<&Children>(parent).unwrap();
        assert_eq!(c.0.len(), 2);
        assert!(c.0.contains(&child1));
        assert!(c.0.contains(&child2));
    }

    #[test]
    fn test_set_parent_reparent() {
        let mut world = World::new();
        let old_parent = world.spawn((Transform::default(),));
        let new_parent = world.spawn((Transform::default(),));
        let child = world.spawn((Transform::default(),));

        set_parent(&mut world, child, old_parent);
        set_parent(&mut world, child, new_parent);

        // 古い親から除去される
        let old_children = world.get::<&Children>(old_parent).unwrap();
        assert!(!old_children.0.contains(&child));

        // 新しい親に追加される
        let new_children = world.get::<&Children>(new_parent).unwrap();
        assert!(new_children.0.contains(&child));

        // Parent コンポーネントが更新される
        let p = world.get::<&Parent>(child).unwrap();
        assert_eq!(p.0, new_parent);
    }

    #[test]
    fn test_set_parent_no_duplicate() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child = world.spawn((Transform::default(),));

        set_parent(&mut world, child, parent);
        set_parent(&mut world, child, parent); // 同じ親に再設定

        let c = world.get::<&Children>(parent).unwrap();
        assert_eq!(c.0.len(), 1); // 重複なし
    }

    #[test]
    fn test_propagate_transforms_root_only() {
        let mut world = World::new();
        let entity = world.spawn((Transform::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)),));

        propagate_transforms(&mut world);

        let gt = world.get::<&GlobalTransform>(entity).unwrap();
        let expected = Transform::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)).to_matrix();
        assert!((gt.0 - expected).abs_diff_eq(glam::Mat4::ZERO, 1e-5));
    }

    #[test]
    fn test_propagate_transforms_parent_child() {
        let mut world = World::new();
        let parent = world.spawn((Transform::from_translation(glam::Vec3::new(10.0, 0.0, 0.0)),));
        let child = world.spawn((Transform::from_translation(glam::Vec3::new(0.0, 5.0, 0.0)),));

        set_parent(&mut world, child, parent);
        propagate_transforms(&mut world);

        // 子のグローバル位置は (10, 5, 0)
        let gt = world.get::<&GlobalTransform>(child).unwrap();
        let pos = gt.0.transform_point3(glam::Vec3::ZERO);
        assert!((pos - glam::Vec3::new(10.0, 5.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_propagate_transforms_deep_hierarchy() {
        let mut world = World::new();
        let a = world.spawn((Transform::from_translation(glam::Vec3::new(1.0, 0.0, 0.0)),));
        let b = world.spawn((Transform::from_translation(glam::Vec3::new(0.0, 2.0, 0.0)),));
        let c = world.spawn((Transform::from_translation(glam::Vec3::new(0.0, 0.0, 3.0)),));

        set_parent(&mut world, b, a);
        set_parent(&mut world, c, b);
        propagate_transforms(&mut world);

        // C のグローバル位置は (1, 2, 3)
        let gt = world.get::<&GlobalTransform>(c).unwrap();
        let pos = gt.0.transform_point3(glam::Vec3::ZERO);
        assert!((pos - glam::Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn test_propagate_transforms_with_scale() {
        let mut world = World::new();
        let parent = world.spawn((Transform {
            scale: glam::Vec3::splat(2.0),
            ..Default::default()
        },));
        let child = world.spawn((Transform::from_translation(glam::Vec3::new(1.0, 0.0, 0.0)),));

        set_parent(&mut world, child, parent);
        propagate_transforms(&mut world);

        // 親のスケール 2x → 子のローカル (1,0,0) はグローバル (2,0,0)
        let gt = world.get::<&GlobalTransform>(child).unwrap();
        let pos = gt.0.transform_point3(glam::Vec3::ZERO);
        assert!((pos - glam::Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_propagate_deep_hierarchy_no_panic() {
        // MAX_HIERARCHY_DEPTH (256) を超える深い階層でもパニックしない
        let mut world = World::new();
        let depth = 300;
        let mut entities = Vec::with_capacity(depth);

        let root = world.spawn((Transform::from_translation(glam::Vec3::X),));
        entities.push(root);

        for i in 1..depth {
            let child = world.spawn((Transform::from_translation(glam::Vec3::Y * i as f32),));
            set_parent(&mut world, child, entities[i - 1]);
            entities.push(child);
        }

        // パニックしないことを検証
        propagate_transforms(&mut world);

        // ルート直下は正しく伝播されている
        let gt = world.get::<&GlobalTransform>(entities[1]).unwrap();
        let pos = gt.0.transform_point3(glam::Vec3::ZERO);
        assert!((pos - glam::Vec3::new(1.0, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn test_propagate_wide_hierarchy() {
        let mut world = World::new();
        let parent = world.spawn((Transform::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)),));

        let mut children = Vec::new();
        for i in 0..50 {
            let child = world.spawn((Transform::from_translation(glam::Vec3::Y * i as f32),));
            set_parent(&mut world, child, parent);
            children.push(child);
        }

        propagate_transforms(&mut world);

        // 全子が正しい GlobalTransform を持つ
        for (i, &child) in children.iter().enumerate() {
            let gt = world.get::<&GlobalTransform>(child).unwrap();
            let pos = gt.0.transform_point3(glam::Vec3::ZERO);
            let expected = glam::Vec3::new(5.0, i as f32, 0.0);
            assert!(
                (pos - expected).length() < 1e-5,
                "子 {i}: 期待 {expected}, 実際 {pos}"
            );
        }
    }

    #[test]
    fn test_despawn_removes_from_parent_children() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child = world.spawn((Transform::default(),));

        set_parent(&mut world, child, parent);

        // 子を despawn
        crate::ecs::spawn::despawn(&mut world, child);

        // 親の Children リストから除去されている
        let children = world.get::<&Children>(parent).unwrap();
        assert!(
            !children.0.contains(&child),
            "despawn 後、親の Children から除去されるべき"
        );
    }

    #[test]
    fn test_despawn_cascades_to_children() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child = world.spawn((Transform::default(),));
        let grandchild = world.spawn((Transform::default(),));

        set_parent(&mut world, child, parent);
        set_parent(&mut world, grandchild, child);

        // 子を despawn → 孫も削除される
        crate::ecs::spawn::despawn(&mut world, child);

        assert!(
            world.entity(child).is_err(),
            "despawn された子エンティティは存在しないべき"
        );
        assert!(
            world.entity(grandchild).is_err(),
            "despawn で孫エンティティも再帰削除されるべき"
        );
        // 親は生存
        assert!(world.entity(parent).is_ok());
    }

    #[test]
    fn test_despawn_root_entity() {
        let mut world = World::new();
        let entity = world.spawn((Transform::default(),));

        let result = crate::ecs::spawn::despawn(&mut world, entity);
        assert!(result, "ルートエンティティの despawn は成功するべき");
        assert!(world.entity(entity).is_err());
    }
}
