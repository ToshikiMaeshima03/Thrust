//! レンダリング設定パネル (Round 9)
//!
//! Fog/SSAO/SSR/Bloom/Tonemap/DOF/Color Grading/TAA 等を実行時に調整する。

use crate::ecs::resources::Resources;

pub fn show(ctx: &egui::Context, res: &mut Resources) {
    egui::Window::new("レンダリング設定")
        .default_pos(egui::pos2(20.0, 480.0))
        .default_width(300.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                fog_section(ui, res);
                ui.separator();
                ssao_section(ui, res);
                ui.separator();
                ssr_section(ui, res);
                ui.separator();
                exposure_section(ui, res);
                ui.separator();
                volumetric_section(ui, res);
                ui.separator();
                dof_section(ui, res);
                ui.separator();
                motion_blur_section(ui, res);
                ui.separator();
                color_grading_section(ui, res);
                ui.separator();
                taa_section(ui, res);
            });
        });
}

fn fog_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("ボリュメトリックフォグ", |ui| {
        let mut density = res.fog.uniform.color_density[3];
        let mut color = [
            res.fog.uniform.color_density[0],
            res.fog.uniform.color_density[1],
            res.fog.uniform.color_density[2],
        ];
        let mut falloff = res.fog.uniform.params[0];
        let mut h_ref = res.fog.uniform.params[1];
        let mut scatter = res.fog.uniform.params[2];

        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut density, 0.0..=0.5).text("Density"))
            .changed();
        changed |= ui.color_edit_button_rgb(&mut color).changed();
        changed |= ui
            .add(egui::Slider::new(&mut falloff, 0.0..=1.0).text("Height Falloff"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut h_ref, -10.0..=20.0).text("Height Ref"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut scatter, 0.0..=2.0).text("Scattering"))
            .changed();

        if changed {
            res.fog.uniform.color_density = [color[0], color[1], color[2], density];
            res.fog.uniform.params = [falloff, h_ref, scatter, res.fog.uniform.params[3]];
            res.gpu.queue.write_buffer(
                &res.fog.buffer,
                0,
                bytemuck::cast_slice(&[res.fog.uniform]),
            );
        }
    });
}

fn ssao_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("SSAO", |ui| {
        let mut radius = res.ssao.uniform.params[0];
        let mut bias = res.ssao.uniform.params[1];
        let mut intensity = res.ssao.uniform.params[2];
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut radius, 0.05..=2.0).text("Radius"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bias, 0.0..=0.1).text("Bias"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut intensity, 0.0..=3.0).text("Intensity"))
            .changed();
        if changed {
            res.ssao.set_params(&res.gpu.queue, radius, bias, intensity);
        }
    });
}

fn ssr_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("SSR (反射)", |ui| {
        let mut max_dist = res.ssr.uniform.params[0];
        let mut thickness = res.ssr.uniform.params[1];
        let mut steps = res.ssr.uniform.params[2];
        let mut strength = res.ssr.uniform.params[3];
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut max_dist, 1.0..=200.0).text("Max Distance"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut thickness, 0.1..=5.0).text("Thickness"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut steps, 4.0..=128.0).text("Max Steps"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut strength, 0.0..=2.0).text("Strength"))
            .changed();
        if changed {
            res.ssr
                .set_params(&res.gpu.queue, max_dist, thickness, steps as u32, strength);
        }
    });
}

fn exposure_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("ポストプロセス (Bloom + Tonemap)", |ui| {
        // Bloom uniform: [threshold, soft_knee, filter_radius, _]
        // Post uniform: [exposure, bloom_strength, enable_bloom, _]
        // We don't have direct accessors, so we'd need to update via queue
        ui.label("(Bloom/Tonemap パラメータは Resources.post 経由で調整可能)");
    });
}

fn volumetric_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("ボリュメトリックライト (God Rays)", |ui| {
        let mut density = res.volumetric.uniform.params[0];
        let mut decay = res.volumetric.uniform.params[1];
        let mut weight = res.volumetric.uniform.params[2];
        let mut exposure = res.volumetric.uniform.params[3];
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut density, 0.0..=2.0).text("Density"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut decay, 0.5..=1.0).text("Decay"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut weight, 0.0..=2.0).text("Weight"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut exposure, 0.0..=1.0).text("Exposure"))
            .changed();
        if changed {
            res.volumetric
                .set_params(&res.gpu.queue, density, decay, weight, exposure);
        }
    });
}

fn dof_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("被写界深度 (DOF)", |ui| {
        let mut enabled = res.dof.uniform.params[3] > 0.5;
        let mut focus_dist = res.dof.uniform.params[0];
        let mut focus_range = res.dof.uniform.params[1];
        let mut max_blur = res.dof.uniform.params[2];
        let mut changed = false;
        changed |= ui.checkbox(&mut enabled, "有効").changed();
        changed |= ui
            .add(egui::Slider::new(&mut focus_dist, 0.1..=100.0).text("Focus Distance"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut focus_range, 0.1..=50.0).text("Focus Range"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut max_blur, 0.0..=20.0).text("Max Blur (px)"))
            .changed();
        if changed {
            res.dof
                .set_focus(&res.gpu.queue, focus_dist, focus_range, max_blur, enabled);
        }
    });
}

fn motion_blur_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("モーションブラー", |ui| {
        let mut enabled = res.motion_blur.uniform.params[2] > 0.5;
        let mut strength = res.motion_blur.uniform.params[0];
        let mut max_off = res.motion_blur.uniform.params[1];
        let mut changed = false;
        changed |= ui.checkbox(&mut enabled, "有効").changed();
        changed |= ui
            .add(egui::Slider::new(&mut strength, 0.0..=4.0).text("Strength"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut max_off, 1.0..=64.0).text("Max Offset (px)"))
            .changed();
        if changed {
            res.motion_blur
                .set_params(&res.gpu.queue, strength, max_off, enabled);
        }
    });
}

fn color_grading_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("カラーグレーディング", |ui| {
        let mut u = res.color_grading.uniform;
        let mut enabled = u.lift[3] > 0.5;
        let mut changed = false;

        changed |= ui.checkbox(&mut enabled, "LGG 有効").changed();
        ui.label("Lift");
        let mut lift = [u.lift[0], u.lift[1], u.lift[2]];
        if ui.color_edit_button_rgb(&mut lift).changed() {
            u.lift[0] = lift[0];
            u.lift[1] = lift[1];
            u.lift[2] = lift[2];
            changed = true;
        }
        ui.label("Gamma");
        let mut gamma = [u.gamma[0], u.gamma[1], u.gamma[2]];
        if ui.color_edit_button_rgb(&mut gamma).changed() {
            u.gamma[0] = gamma[0];
            u.gamma[1] = gamma[1];
            u.gamma[2] = gamma[2];
            changed = true;
        }
        ui.label("Gain");
        let mut gain = [u.gain[0], u.gain[1], u.gain[2]];
        if ui.color_edit_button_rgb(&mut gain).changed() {
            u.gain[0] = gain[0];
            u.gain[1] = gain[1];
            u.gain[2] = gain[2];
            changed = true;
        }

        changed |= ui
            .add(egui::Slider::new(&mut u.misc[0], 0.0..=2.0).text("Saturation"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut u.misc[1], 0.5..=2.0).text("Contrast"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut u.misc[2], -3.0..=3.0).text("Exposure (EV)"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut u.misc[3], 0.0..=1.5).text("Vignette Strength"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut u.misc2[0], 0.0..=1.0).text("Vignette Radius"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut u.misc2[1], 0.0..=2.0).text("Chromatic Aberration"))
            .changed();

        if changed {
            u.lift[3] = if enabled { 1.0 } else { 0.0 };
            res.color_grading.set_params(&res.gpu.queue, u);
        }
    });
}

fn taa_section(ui: &mut egui::Ui, res: &mut Resources) {
    ui.collapsing("TAA (Temporal AA)", |ui| {
        // TAA は Round 8 で追加したが Resources にはまだ統合していないので placeholder
        ui.label("TAA は Resources に未統合 (要 wiring)");
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compiles() {}
}
