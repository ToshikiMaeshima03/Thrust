//! ゲーム内エディタ (Round 9)
//!
//! egui ベースのエディタ UI を提供する。エンティティブラウザ、プロパティインスペクタ、
//! Spawn メニュー、Render 設定パネル、パフォーマンス HUD を 1 つの window に統合。
//!
//! ユーザーアプリの `ThrustAppHandler::ui()` 内で `Editor::show(ctx, world, res)` を
//! 呼べばエディタが起動する。

#![allow(deprecated)]
#![allow(unused_variables)]

use std::collections::HashMap;

use hecs::{Entity, World};

use crate::ecs::components::{ActiveCamera, MeshHandle};
use crate::ecs::resources::Resources;
use crate::light::light::{AmbientLight, DirectionalLight, PointLight, SpotLight};

pub mod gizmo;
pub mod inspector;
pub mod outliner;
pub mod render_settings;
pub mod spawn_menu;

pub use gizmo::{GizmoMode, TransformGizmo};

/// エディタの状態
pub struct Editor {
    /// 現在選択されているエンティティ
    pub selected: Option<Entity>,
    /// アウトライナを表示するか
    pub show_outliner: bool,
    /// インスペクタを表示するか
    pub show_inspector: bool,
    /// Spawn メニューを表示するか
    pub show_spawn_menu: bool,
    /// レンダリング設定を表示するか
    pub show_render_settings: bool,
    /// パフォーマンス HUD を表示するか
    pub show_performance: bool,
    /// ギズモ
    pub gizmo: TransformGizmo,
    /// エディタが動作中か (false でゲームプレイモード)
    pub enabled: bool,
    /// 検索フィルタ
    pub search_filter: String,
    /// エンティティに付ける表示名
    pub entity_names: HashMap<Entity, String>,
    /// 次に spawn する位置 (UI 入力)
    pub spawn_position: [f32; 3],
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            selected: None,
            show_outliner: true,
            show_inspector: true,
            show_spawn_menu: true,
            show_render_settings: true,
            show_performance: true,
            gizmo: TransformGizmo::default(),
            enabled: true,
            search_filter: String::new(),
            entity_names: HashMap::new(),
            spawn_position: [0.0, 1.0, 0.0],
        }
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// メインメニュー + 各パネルを描画する
    ///
    /// `ThrustAppHandler::ui` 内から呼ぶ。
    pub fn show(&mut self, ctx: &egui::Context, world: &mut World, res: &mut Resources) {
        if !self.enabled {
            return;
        }

        // メインメニュー
        egui::TopBottomPanel::top("editor_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("シーンを保存 (scene.json)").clicked() {
                        let scene = crate::scene::serialize::SerScene::from_world(world);
                        if let Err(e) = scene.save_to_file("scene.json") {
                            log::warn!("シーン保存失敗: {e}");
                        }
                        ui.close_menu();
                    }
                    if ui.button("シーンを読み込み (scene.json)").clicked() {
                        if let Ok(scene) =
                            crate::scene::serialize::SerScene::load_from_file("scene.json")
                        {
                            scene.apply_to_world(world);
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("表示", |ui| {
                    ui.checkbox(&mut self.show_outliner, "アウトライナ");
                    ui.checkbox(&mut self.show_inspector, "インスペクタ");
                    ui.checkbox(&mut self.show_spawn_menu, "Spawn メニュー");
                    ui.checkbox(&mut self.show_render_settings, "レンダリング設定");
                    ui.checkbox(&mut self.show_performance, "パフォーマンス");
                });
                ui.menu_button("ツール", |ui| {
                    ui.radio_value(&mut self.gizmo.mode, GizmoMode::Translate, "移動 (T)");
                    ui.radio_value(&mut self.gizmo.mode, GizmoMode::Rotate, "回転 (R)");
                    ui.radio_value(&mut self.gizmo.mode, GizmoMode::Scale, "スケール (S)");
                });
                ui.label(format!(
                    "FPS: {:.1}  |  エンティティ: {}",
                    res.debug_stats.fps,
                    world.iter().count()
                ));
            });
        });

        if self.show_outliner {
            outliner::show(ctx, self, world);
        }
        if self.show_inspector {
            inspector::show(ctx, self, world);
        }
        if self.show_spawn_menu {
            spawn_menu::show(ctx, self, world, res);
        }
        if self.show_render_settings {
            render_settings::show(ctx, res);
        }
        if self.show_performance {
            performance_panel(ctx, res, world);
        }
    }

    /// 選択中エンティティの表示名を取得
    pub fn entity_label(&self, entity: Entity, world: &World) -> String {
        if let Some(name) = self.entity_names.get(&entity) {
            return name.clone();
        }
        // コンポーネントから推測
        if world.get::<&MeshHandle>(entity).is_ok() {
            return format!("Mesh #{}", entity.id());
        }
        if world.get::<&DirectionalLight>(entity).is_ok() {
            return format!("DirLight #{}", entity.id());
        }
        if world.get::<&PointLight>(entity).is_ok() {
            return format!("PointLight #{}", entity.id());
        }
        if world.get::<&SpotLight>(entity).is_ok() {
            return format!("SpotLight #{}", entity.id());
        }
        if world.get::<&AmbientLight>(entity).is_ok() {
            return format!("AmbientLight #{}", entity.id());
        }
        if world.get::<&crate::camera::camera::Camera>(entity).is_ok() {
            return format!("Camera #{}", entity.id());
        }
        format!("Entity #{}", entity.id())
    }
}

/// パフォーマンス HUD パネル
fn performance_panel(ctx: &egui::Context, res: &Resources, world: &World) {
    egui::Window::new("パフォーマンス")
        .default_pos(egui::pos2(20.0, 60.0))
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.label(format!("FPS: {:.1}", res.debug_stats.fps));
            ui.label(format!(
                "Frame Time: {:.2} ms",
                res.debug_stats.frame_time_ms
            ));
            ui.separator();
            ui.label(format!("Entities: {}", world.iter().count()));

            let mut mesh_count = 0;
            let mut light_dir = 0;
            let mut light_point = 0;
            let mut light_spot = 0;
            for entity_ref in world.iter() {
                if entity_ref.has::<MeshHandle>() {
                    mesh_count += 1;
                }
                if entity_ref.has::<DirectionalLight>() {
                    light_dir += 1;
                }
                if entity_ref.has::<PointLight>() {
                    light_point += 1;
                }
                if entity_ref.has::<SpotLight>() {
                    light_spot += 1;
                }
            }
            ui.label(format!("Meshes: {mesh_count}"));
            ui.label(format!("Dir Lights: {light_dir}"));
            ui.label(format!("Point Lights: {light_point}"));
            ui.label(format!("Spot Lights: {light_spot}"));
        });
}

/// アクティブカメラを取得するヘルパー
pub fn active_camera(world: &World) -> Option<crate::camera::camera::Camera> {
    world
        .query::<(&crate::camera::camera::Camera, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(c, _)| crate::camera::camera::Camera {
            position: c.position,
            target: c.target,
            up: c.up,
            fov_y: c.fov_y,
            aspect: c.aspect,
            z_near: c.z_near,
            z_far: c.z_far,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_default_enabled() {
        let e = Editor::new();
        assert!(e.enabled);
        assert!(e.show_outliner);
        assert!(e.show_inspector);
    }

    #[test]
    fn test_editor_no_selection_initially() {
        let e = Editor::new();
        assert!(e.selected.is_none());
    }

    #[test]
    fn test_entity_label_unknown() {
        let e = Editor::new();
        let world = World::new();
        let mut w = world;
        let entity = w.spawn(("dummy",));
        let label = e.entity_label(entity, &w);
        assert!(label.contains("Entity"));
    }

    #[test]
    fn test_entity_label_with_name() {
        let mut e = Editor::new();
        let mut world = World::new();
        let entity = world.spawn(("dummy",));
        e.entity_names.insert(entity, "Player".to_string());
        let label = e.entity_label(entity, &world);
        assert_eq!(label, "Player");
    }
}
