//! Spawn メニュー (Round 9)
//!
//! プリミティブ、ライト、パーティクルを GUI から実行時に追加する。

use hecs::World;

use super::Editor;
use crate::ecs::resources::Resources;
use crate::light::light::{AmbientLight, DirectionalLight, PointLight, SpotLight};
use crate::material::material::Material;
use crate::scene::transform::Transform;

pub fn show(ctx: &egui::Context, editor: &mut Editor, world: &mut World, res: &mut Resources) {
    egui::Window::new("Spawn メニュー")
        .default_pos(egui::pos2(20.0, 240.0))
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.label("Spawn 位置");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut editor.spawn_position[0]).speed(0.1));
                ui.add(egui::DragValue::new(&mut editor.spawn_position[1]).speed(0.1));
                ui.add(egui::DragValue::new(&mut editor.spawn_position[2]).speed(0.1));
            });
            ui.separator();
            ui.label("プリミティブ");
            let pos = glam::Vec3::from(editor.spawn_position);

            if ui.button("Cube").clicked() {
                let mesh = crate::mesh::primitives::create_cube(&res.gpu.device, 1.0);
                let entity = crate::ecs::spawn::spawn_object(
                    world,
                    mesh,
                    Transform {
                        translation: pos,
                        ..Default::default()
                    },
                    Material::dielectric(glam::Vec3::splat(0.7), 0.5),
                );
                editor.selected = Some(entity);
            }
            if ui.button("Sphere").clicked() {
                let mesh = crate::mesh::primitives::create_sphere(&res.gpu.device, 0.5, 32, 16);
                let entity = crate::ecs::spawn::spawn_object(
                    world,
                    mesh,
                    Transform {
                        translation: pos,
                        ..Default::default()
                    },
                    Material::metallic(glam::Vec3::new(0.95, 0.95, 0.95), 0.3),
                );
                editor.selected = Some(entity);
            }
            if ui.button("Plane").clicked() {
                let mesh = crate::mesh::primitives::create_plane(&res.gpu.device, 5.0);
                let entity = crate::ecs::spawn::spawn_object(
                    world,
                    mesh,
                    Transform {
                        translation: pos,
                        ..Default::default()
                    },
                    Material::dielectric(glam::Vec3::splat(0.5), 0.7),
                );
                editor.selected = Some(entity);
            }

            ui.separator();
            ui.label("ライト");
            if ui.button("Directional Light").clicked() {
                let entity = world.spawn((DirectionalLight {
                    direction: glam::Vec3::new(0.3, -1.0, 0.2).normalize(),
                    color: glam::Vec3::ONE,
                    intensity: 3.0,
                },));
                editor.selected = Some(entity);
            }
            if ui.button("Point Light").clicked() {
                let entity = world.spawn((
                    Transform {
                        translation: pos,
                        ..Default::default()
                    },
                    PointLight {
                        color: glam::Vec3::new(1.0, 0.9, 0.8),
                        intensity: 30.0,
                        range: 10.0,
                    },
                ));
                editor.selected = Some(entity);
            }
            if ui.button("Spot Light").clicked() {
                let entity = world.spawn((
                    Transform {
                        translation: pos,
                        ..Default::default()
                    },
                    SpotLight {
                        color: glam::Vec3::ONE,
                        intensity: 50.0,
                        range: 20.0,
                        inner_angle: 15.0_f32.to_radians(),
                        outer_angle: 30.0_f32.to_radians(),
                        direction: glam::Vec3::NEG_Y,
                    },
                ));
                editor.selected = Some(entity);
            }
            if ui.button("Ambient Light").clicked() {
                let entity = world.spawn((
                    AmbientLight {
                        color: glam::Vec3::new(0.4, 0.5, 0.6),
                        intensity: 0.3,
                    },
                    crate::ecs::components::ActiveAmbientLight,
                ));
                editor.selected = Some(entity);
            }

            ui.separator();
            ui.label("マテリアルプリセット");
            ui.label("(選択中エンティティに適用)");
            if let Some(entity) = editor.selected {
                if ui.button("カーペイント (赤)").clicked() {
                    let _ = world
                        .insert_one(entity, Material::car_paint(glam::Vec3::new(0.8, 0.1, 0.1)));
                }
                if ui.button("ブラッシュメタル").clicked() {
                    let _ = world.insert_one(
                        entity,
                        Material::brushed_metal(glam::Vec3::new(0.9, 0.9, 0.9), 0.7),
                    );
                }
                if ui.button("スキン").clicked() {
                    let _ =
                        world.insert_one(entity, Material::skin(glam::Vec3::new(0.95, 0.75, 0.65)));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compiles() {}
}
