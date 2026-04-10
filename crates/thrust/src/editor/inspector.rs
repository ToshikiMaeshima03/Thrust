//! プロパティインスペクタ (Round 9)
//!
//! 選択中エンティティのコンポーネントを編集できる。Transform/Material/Light をサポート。

use hecs::World;

use super::Editor;
use crate::light::light::{AmbientLight, DirectionalLight, PointLight, SpotLight};
use crate::material::material::Material;
use crate::scene::transform::Transform;

pub fn show(ctx: &egui::Context, editor: &mut Editor, world: &mut World) {
    egui::SidePanel::right("inspector_panel")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("インスペクタ");
            let Some(entity) = editor.selected else {
                ui.label("エンティティ未選択");
                return;
            };
            ui.label(format!("選択中: {}", editor.entity_label(entity, world)));
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Transform
                if let Ok(mut t) = world.get::<&mut Transform>(entity) {
                    ui.collapsing("Transform", |ui| {
                        ui.label("Translation");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut t.translation.x).speed(0.1));
                            ui.add(egui::DragValue::new(&mut t.translation.y).speed(0.1));
                            ui.add(egui::DragValue::new(&mut t.translation.z).speed(0.1));
                        });

                        // 回転は euler 角に変換して表示
                        let (yaw, pitch, roll) = t.rotation.to_euler(glam::EulerRot::YXZ);
                        let mut yaw_deg = yaw.to_degrees();
                        let mut pitch_deg = pitch.to_degrees();
                        let mut roll_deg = roll.to_degrees();
                        ui.label("Rotation (deg)");
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(egui::DragValue::new(&mut pitch_deg).speed(1.0).suffix("°"))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut yaw_deg).speed(1.0).suffix("°"))
                                .changed();
                            changed |= ui
                                .add(egui::DragValue::new(&mut roll_deg).speed(1.0).suffix("°"))
                                .changed();
                        });
                        if changed {
                            t.rotation = glam::Quat::from_euler(
                                glam::EulerRot::YXZ,
                                yaw_deg.to_radians(),
                                pitch_deg.to_radians(),
                                roll_deg.to_radians(),
                            );
                        }

                        ui.label("Scale");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut t.scale.x).speed(0.05));
                            ui.add(egui::DragValue::new(&mut t.scale.y).speed(0.05));
                            ui.add(egui::DragValue::new(&mut t.scale.z).speed(0.05));
                        });
                    });
                }

                // Material
                if let Ok(mut mat) = world.get::<&mut Material>(entity) {
                    ui.collapsing("Material", |ui| {
                        let mut color = [
                            mat.base_color_factor.x,
                            mat.base_color_factor.y,
                            mat.base_color_factor.z,
                        ];
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            mat.base_color_factor.x = color[0];
                            mat.base_color_factor.y = color[1];
                            mat.base_color_factor.z = color[2];
                        }
                        ui.add(
                            egui::Slider::new(&mut mat.metallic_factor, 0.0..=1.0).text("Metallic"),
                        );
                        ui.add(
                            egui::Slider::new(&mut mat.roughness_factor, 0.04..=1.0)
                                .text("Roughness"),
                        );
                        ui.label("Emissive");
                        let mut em = mat.emissive_factor.to_array();
                        if ui.color_edit_button_rgb(&mut em).changed() {
                            mat.emissive_factor = glam::Vec3::from(em);
                        }
                        ui.add(
                            egui::Slider::new(&mut mat.normal_scale, 0.0..=2.0)
                                .text("Normal Scale"),
                        );
                        ui.add(
                            egui::Slider::new(&mut mat.occlusion_strength, 0.0..=1.0)
                                .text("AO Strength"),
                        );
                        ui.separator();
                        ui.label("拡張 (Round 8)");
                        ui.add(egui::Slider::new(&mut mat.clearcoat, 0.0..=1.0).text("Clearcoat"));
                        ui.add(
                            egui::Slider::new(&mut mat.clearcoat_roughness, 0.0..=1.0)
                                .text("Clearcoat Rough"),
                        );
                        ui.add(
                            egui::Slider::new(&mut mat.anisotropy, -1.0..=1.0).text("Anisotropy"),
                        );
                        ui.add(
                            egui::Slider::new(&mut mat.subsurface, 0.0..=1.0).text("Subsurface"),
                        );
                    });
                }

                // Directional Light
                if let Ok(mut l) = world.get::<&mut DirectionalLight>(entity) {
                    ui.collapsing("Directional Light", |ui| {
                        let mut color = l.color.to_array();
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            l.color = glam::Vec3::from(color);
                        }
                        ui.add(egui::Slider::new(&mut l.intensity, 0.0..=20.0).text("Intensity"));
                        ui.label("Direction");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut l.direction.x).speed(0.05));
                            ui.add(egui::DragValue::new(&mut l.direction.y).speed(0.05));
                            ui.add(egui::DragValue::new(&mut l.direction.z).speed(0.05));
                        });
                    });
                }

                if let Ok(mut l) = world.get::<&mut PointLight>(entity) {
                    ui.collapsing("Point Light", |ui| {
                        let mut color = l.color.to_array();
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            l.color = glam::Vec3::from(color);
                        }
                        ui.add(egui::Slider::new(&mut l.intensity, 0.0..=200.0).text("Intensity"));
                        ui.add(egui::Slider::new(&mut l.range, 0.1..=100.0).text("Range"));
                    });
                }

                if let Ok(mut l) = world.get::<&mut SpotLight>(entity) {
                    ui.collapsing("Spot Light", |ui| {
                        let mut color = l.color.to_array();
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            l.color = glam::Vec3::from(color);
                        }
                        ui.add(egui::Slider::new(&mut l.intensity, 0.0..=200.0).text("Intensity"));
                        ui.add(egui::Slider::new(&mut l.range, 0.1..=100.0).text("Range"));
                        let mut inner_deg = l.inner_angle.to_degrees();
                        let mut outer_deg = l.outer_angle.to_degrees();
                        if ui
                            .add(egui::Slider::new(&mut inner_deg, 0.0..=89.0).text("Inner°"))
                            .changed()
                        {
                            l.inner_angle = inner_deg.to_radians();
                        }
                        if ui
                            .add(egui::Slider::new(&mut outer_deg, 0.0..=90.0).text("Outer°"))
                            .changed()
                        {
                            l.outer_angle = outer_deg.to_radians();
                        }
                    });
                }

                if let Ok(mut l) = world.get::<&mut AmbientLight>(entity) {
                    ui.collapsing("Ambient Light", |ui| {
                        let mut color = l.color.to_array();
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            l.color = glam::Vec3::from(color);
                        }
                        ui.add(egui::Slider::new(&mut l.intensity, 0.0..=2.0).text("Intensity"));
                    });
                }

                // Visible toggle
                if let Ok(mut v) = world.get::<&mut crate::ecs::components::Visible>(entity) {
                    ui.checkbox(&mut v.0, "Visible");
                }
            });
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compiles() {}
}
