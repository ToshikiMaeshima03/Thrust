use hecs::{Entity, World};

use crate::ecs::components::{MeshHandle, Visible};
use crate::ecs::resources::Resources;
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

/// エンティティを削除（GPU リソースは RenderState の Drop で自動解放）
pub fn despawn(world: &mut World, entity: Entity) -> bool {
    world.despawn(entity).is_ok()
}
