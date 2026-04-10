//! レンズフレア (Round 8)
//!
//! 画面内の光源位置に対し、画面中心から光源への方向に半透明スプライトを並べる。
//! デプスバッファとの遮蔽判定で光源が隠れている場合はフェードアウトする。
//!
//! ECS: `LensFlareSource` を方向光や強烈な点光源にアタッチ。
//! `lens_flare_system` が CPU 側で描画パラメータを計算する。

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4Swizzles};

/// 1 つの flare ゴースト要素
#[derive(Debug, Clone, Copy)]
pub struct FlareGhost {
    /// 画面中心からの相対位置 (0 = 光源、1 = 中心、2 = 反対側)
    pub offset: f32,
    /// サイズ (NDC 単位)
    pub size: f32,
    /// 色味
    pub color: glam::Vec4,
}

/// レンズフレア光源コンポーネント
#[derive(Debug, Clone)]
pub struct LensFlareSource {
    /// ワールド位置 (directional light は遠距離に置く)
    pub world_position: Vec3,
    /// ベース強度
    pub intensity: f32,
    /// ゴースト要素のリスト
    pub ghosts: Vec<FlareGhost>,
    /// 内部計算: 画面に投影された UV 位置 (0..1)
    pub screen_uv: Vec2,
    /// 遮蔽係数 (0..1, 1=完全可視)
    pub visibility: f32,
}

impl Default for LensFlareSource {
    fn default() -> Self {
        Self {
            world_position: Vec3::new(50.0, 100.0, 50.0),
            intensity: 1.0,
            ghosts: default_ghosts(),
            screen_uv: Vec2::ZERO,
            visibility: 0.0,
        }
    }
}

/// デフォルトのゴースト構成
pub fn default_ghosts() -> Vec<FlareGhost> {
    vec![
        FlareGhost {
            offset: 0.0,
            size: 0.15,
            color: glam::Vec4::new(1.0, 0.9, 0.7, 0.7),
        },
        FlareGhost {
            offset: 0.5,
            size: 0.06,
            color: glam::Vec4::new(0.9, 0.6, 0.3, 0.4),
        },
        FlareGhost {
            offset: 0.7,
            size: 0.04,
            color: glam::Vec4::new(0.5, 0.7, 0.9, 0.5),
        },
        FlareGhost {
            offset: 1.0,
            size: 0.08,
            color: glam::Vec4::new(0.7, 0.5, 0.4, 0.3),
        },
        FlareGhost {
            offset: 1.3,
            size: 0.05,
            color: glam::Vec4::new(0.5, 0.9, 0.6, 0.3),
        },
        FlareGhost {
            offset: 1.5,
            size: 0.10,
            color: glam::Vec4::new(0.4, 0.4, 0.9, 0.4),
        },
    ]
}

/// GPU instance データ (1 ゴースト = 1 instance)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LensFlareInstance {
    /// xy = 中心 UV, z = size, w = visibility
    pub center_size_vis: [f32; 4],
    /// rgba = color
    pub color: [f32; 4],
}

impl LensFlareSource {
    /// ワールド位置を view-projection 行列で投影する
    pub fn project(&mut self, view_proj: glam::Mat4) {
        let clip = view_proj
            * glam::Vec4::new(
                self.world_position.x,
                self.world_position.y,
                self.world_position.z,
                1.0,
            );
        if clip.w <= 0.0 {
            self.visibility = 0.0;
            return;
        }
        let ndc = clip.xyz() / clip.w;
        if ndc.x.abs() > 1.5 || ndc.y.abs() > 1.5 || ndc.z < 0.0 || ndc.z > 1.0 {
            self.visibility = 0.0;
            return;
        }
        self.screen_uv = Vec2::new(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
        // フェードアウト: 画面端ほど弱い
        let edge_x: f32 = (1.0_f32 - ndc.x.abs()).max(0.0);
        let edge_y: f32 = (1.0_f32 - ndc.y.abs()).max(0.0);
        self.visibility = (edge_x * edge_y).clamp(0.0, 1.0);
    }

    /// 各ゴーストの描画パラメータをインスタンスデータとして生成
    pub fn build_instances(&self) -> Vec<LensFlareInstance> {
        if self.visibility < 0.001 {
            return Vec::new();
        }
        let center = Vec2::new(0.5, 0.5);
        let dir = self.screen_uv - center;
        self.ghosts
            .iter()
            .map(|g| {
                let pos = self.screen_uv - dir * g.offset * 2.0;
                let mut color = g.color * self.intensity * self.visibility;
                color.w = (color.w).clamp(0.0, 1.0);
                LensFlareInstance {
                    center_size_vis: [pos.x, pos.y, g.size, self.visibility],
                    color: color.to_array(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_ghosts() {
        let f = LensFlareSource::default();
        assert!(!f.ghosts.is_empty());
    }

    #[test]
    fn test_project_off_screen() {
        let mut f = LensFlareSource::default();
        // カメラの後ろに光源を置く
        f.world_position = Vec3::new(0.0, 0.0, -100.0);
        let view = glam::Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = glam::Mat4::perspective_rh(45_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        f.project(proj * view);
        assert!(f.visibility < 0.5);
    }

    #[test]
    fn test_project_on_screen_visible() {
        let mut f = LensFlareSource::default();
        // 画面中心に強く投影される位置 (カメラ前方)
        f.world_position = Vec3::new(0.0, 0.0, 0.0);
        let view = glam::Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = glam::Mat4::perspective_rh(60_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        f.project(proj * view);
        assert!(
            f.visibility > 0.0,
            "visibility = {}, screen_uv = {:?}",
            f.visibility,
            f.screen_uv
        );
    }

    #[test]
    fn test_build_instances_when_invisible() {
        let mut f = LensFlareSource::default();
        f.visibility = 0.0;
        let instances = f.build_instances();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_lens_flare_instance_size() {
        // 16 + 16 = 32 B
        assert_eq!(std::mem::size_of::<LensFlareInstance>(), 32);
    }
}
