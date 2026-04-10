//! アウトライナパネル (Round 9)
//!
//! ワールド内の全エンティティをツリー表示する。クリックで選択、ダブルクリックでフォーカス。

use hecs::World;

use super::Editor;

pub fn show(ctx: &egui::Context, editor: &mut Editor, world: &mut World) {
    egui::SidePanel::left("outliner_panel")
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("アウトライナ");
            ui.text_edit_singleline(&mut editor.search_filter);
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let entities: Vec<hecs::Entity> = world.iter().map(|e| e.entity()).collect();
                let filter_lower = editor.search_filter.to_lowercase();
                for entity in entities {
                    let label = editor.entity_label(entity, world);
                    if !filter_lower.is_empty() && !label.to_lowercase().contains(&filter_lower) {
                        continue;
                    }
                    let is_selected = editor.selected == Some(entity);
                    let response = ui.selectable_label(is_selected, &label);
                    if response.clicked() {
                        editor.selected = Some(entity);
                    }
                    if response
                        .context_menu(|ui| {
                            if ui.button("削除").clicked() {
                                crate::ecs::spawn::despawn(world, entity);
                                if editor.selected == Some(entity) {
                                    editor.selected = None;
                                }
                                ui.close_menu();
                            }
                            if ui.button("複製").clicked() {
                                // Transform を取得 → drop してから spawn
                                let t_clone = world
                                    .get::<&crate::scene::transform::Transform>(entity)
                                    .ok()
                                    .map(|t| {
                                        let mut clone = (*t).clone();
                                        clone.translation += glam::Vec3::new(1.0, 0.0, 0.0);
                                        clone
                                    });
                                if let Some(t) = t_clone {
                                    let new_entity = world.spawn((t,));
                                    editor.selected = Some(new_entity);
                                }
                                ui.close_menu();
                            }
                            if ui.button("名前変更").clicked() {
                                editor
                                    .entity_names
                                    .entry(entity)
                                    .or_insert_with(|| label.clone());
                                ui.close_menu();
                            }
                        })
                        .is_some()
                    {}
                }
            });
        });
}

#[cfg(test)]
mod tests {
    // outliner は egui::Context が必要なため、ここでは smoke test のみ
    #[test]
    fn test_compiles() {}
}
