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

        // 子を再帰的に処理
        propagate_recursive(world, entity, matrix);
    }
}

fn propagate_recursive(world: &mut World, parent_entity: Entity, parent_global: glam::Mat4) {
    use crate::scene::transform::Transform;

    // 親の Children リストを取得（コピーして借用を解放）
    let children: Vec<Entity> = match world.get::<&Children>(parent_entity) {
        Ok(c) => c.0.clone(),
        Err(_) => return,
    };

    for child in children {
        let child_matrix = {
            let Ok(t) = world.get::<&Transform>(child) else {
                continue;
            };
            t.to_matrix()
        };

        let global = parent_global * child_matrix;

        // GlobalTransform を更新
        if let Ok(mut gt) = world.get::<&mut GlobalTransform>(child) {
            gt.0 = global;
        } else {
            let _ = world.insert_one(child, GlobalTransform(global));
        }

        // さらに子を処理
        propagate_recursive(world, child, global);
    }
}
