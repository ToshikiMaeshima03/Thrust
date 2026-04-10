use hecs::{Entity, World};

use crate::ecs::components::{MeshHandle, Visible};
use crate::ecs::resources::Resources;
use crate::error::ThrustResult;
use crate::material::material::Material;
use crate::mesh::mesh::Mesh;
use crate::scene::transform::Transform;

/// メッシュ・トランスフォーム・マテリアルでエンティティを生成
pub fn spawn_object(
    world: &mut World,
    mesh: Mesh,
    transform: Transform,
    material: Material,
) -> Entity {
    world.spawn((transform, MeshHandle(mesh), material, Visible::default()))
}

/// キューブエンティティを生成
pub fn spawn_cube(
    world: &mut World,
    res: &Resources,
    size: f32,
    transform: Transform,
    material: Material,
) -> Entity {
    let mesh = crate::mesh::primitives::create_cube(&res.gpu.device, size);
    spawn_object(world, mesh, transform, material)
}

/// 球体エンティティを生成
pub fn spawn_sphere(
    world: &mut World,
    res: &Resources,
    radius: f32,
    segments: u32,
    rings: u32,
    transform: Transform,
    material: Material,
) -> Entity {
    let mesh = crate::mesh::primitives::create_sphere(&res.gpu.device, radius, segments, rings);
    spawn_object(world, mesh, transform, material)
}

/// 平面エンティティを生成
pub fn spawn_plane(
    world: &mut World,
    res: &Resources,
    size: f32,
    transform: Transform,
    material: Material,
) -> Entity {
    let mesh = crate::mesh::primitives::create_plane(&res.gpu.device, size);
    spawn_object(world, mesh, transform, material)
}

/// 子エンティティを生成し、親に紐付ける
pub fn spawn_child(
    world: &mut World,
    parent: Entity,
    mesh: Mesh,
    transform: Transform,
    material: Material,
) -> Entity {
    let child = spawn_object(world, mesh, transform, material);
    crate::scene::hierarchy::set_parent(world, child, parent);
    child
}

/// モデルファイルからエンティティを生成する（拡張子で形式を自動判定）
///
/// サポート形式: OBJ, glTF/GLB, STL
/// メッシュごとにエンティティを生成し、全エンティティのリストを返す。
/// メッシュ数とマテリアル数が異なる場合、不足分にはデフォルトマテリアルを使用する。
pub fn spawn_model(
    world: &mut World,
    res: &mut Resources,
    path: &str,
    transform: Transform,
) -> ThrustResult<Vec<Entity>> {
    let result = crate::mesh::model_loader::load_model(
        &res.gpu.device,
        &res.gpu.queue,
        std::path::Path::new(path),
    )?;

    let mesh_count = result.meshes.len();
    let material_count = result.materials.len();
    if mesh_count != material_count {
        log::warn!(
            "モデル '{}' のメッシュ数({mesh_count})とマテリアル数({material_count})が不一致です",
            path
        );
    }

    let mut materials = result.materials;
    // マテリアルが不足している場合はデフォルトで補完
    materials.resize_with(mesh_count, Material::default);

    let mut entities = Vec::with_capacity(mesh_count);
    for (mesh, material) in result.meshes.into_iter().zip(materials) {
        let entity = spawn_object(world, mesh, transform.clone(), material);
        entities.push(entity);
    }

    Ok(entities)
}

/// エンティティを削除する
///
/// 親子階層を自動クリーンアップする:
/// - 親がいる場合、親の Children リストから自身を除去
/// - 子がいる場合、全子エンティティも反復的に削除（スタックオーバーフロー防止）
///
/// GPU リソースは RenderState の Drop で自動解放。
pub fn despawn(world: &mut World, entity: Entity) -> bool {
    // ルートエンティティの親から自身を除去
    if let Ok(parent_ref) = world.get::<&crate::scene::hierarchy::Parent>(entity) {
        let parent_entity = parent_ref.0;
        drop(parent_ref);
        if let Ok(mut children) = world.get::<&mut crate::scene::hierarchy::Children>(parent_entity)
        {
            children.0.retain(|&e| e != entity);
        }
    }

    // 反復的に全子孫を削除（明示的スタック）
    let mut stack = vec![entity];
    let mut success = true;

    while let Some(e) = stack.pop() {
        // 子エンティティをスタックに追加
        if let Ok(children_ref) = world.get::<&crate::scene::hierarchy::Children>(e) {
            stack.extend(children_ref.0.clone());
        }

        success &= world.despawn(e).is_ok();
    }

    success
}
