use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::renderer::texture::ThrustTexture;

/// PBR (Physically-Based Rendering) マテリアル
///
/// glTF 2.0 metallic-roughness ワークフロー準拠。
/// Round 8: clearcoat / anisotropic / subsurface の拡張を追加。
#[derive(Clone)]
pub struct Material {
    pub base_color_factor: glam::Vec4,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: glam::Vec3,
    /// ノーマルマップの強度
    pub normal_scale: f32,
    /// アンビエントオクルージョンの強度
    pub occlusion_strength: f32,

    // Round 8: Clearcoat (車塗装、塗られた木材)
    /// クリアコート強度 (0..1)
    pub clearcoat: f32,
    /// クリアコートのラフネス (0..1)
    pub clearcoat_roughness: f32,

    // Round 8: Anisotropic (ブラッシュメタル、髪、ベルベット)
    /// 異方性強度 (-1..1)。0 で isotropic
    pub anisotropy: f32,
    /// 異方性方向 (XY 接平面、Z は無視)
    pub anisotropy_direction: glam::Vec2,

    // Round 8: Subsurface Scattering (肌、ロウ、大理石)
    /// SSS 強度 (0..1)
    pub subsurface: f32,
    /// SSS の色調 (通常は肌色系)
    pub subsurface_color: glam::Vec3,

    pub base_color_map: Option<Arc<ThrustTexture>>,
    /// glTF 規約: G=roughness, B=metallic
    pub metallic_roughness_map: Option<Arc<ThrustTexture>>,
    pub normal_map: Option<Arc<ThrustTexture>>,
    pub occlusion_map: Option<Arc<ThrustTexture>>,
    pub emissive_map: Option<Arc<ThrustTexture>>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color_factor: glam::Vec4::new(0.8, 0.6, 0.4, 1.0),
            metallic_factor: 0.0,
            roughness_factor: 0.7,
            emissive_factor: glam::Vec3::ZERO,
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            clearcoat: 0.0,
            clearcoat_roughness: 0.03,
            anisotropy: 0.0,
            anisotropy_direction: glam::Vec2::new(1.0, 0.0),
            subsurface: 0.0,
            subsurface_color: glam::Vec3::new(1.0, 0.3, 0.2),
            base_color_map: None,
            metallic_roughness_map: None,
            normal_map: None,
            occlusion_map: None,
            emissive_map: None,
        }
    }
}

impl Material {
    /// 単色の非テクスチャマテリアルを作成する
    pub fn flat_color(color: glam::Vec3) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(color.x, color.y, color.z, 1.0),
            ..Default::default()
        }
    }

    /// 金属マテリアルを作成する
    pub fn metallic(color: glam::Vec3, roughness: f32) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(color.x, color.y, color.z, 1.0),
            metallic_factor: 1.0,
            roughness_factor: roughness.clamp(0.04, 1.0),
            ..Default::default()
        }
    }

    /// 誘電体マテリアルを作成する
    pub fn dielectric(color: glam::Vec3, roughness: f32) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(color.x, color.y, color.z, 1.0),
            metallic_factor: 0.0,
            roughness_factor: roughness.clamp(0.04, 1.0),
            ..Default::default()
        }
    }

    /// カーペイント (Round 8): 金属 + clearcoat 層
    pub fn car_paint(color: glam::Vec3) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(color.x, color.y, color.z, 1.0),
            metallic_factor: 0.8,
            roughness_factor: 0.4,
            clearcoat: 1.0,
            clearcoat_roughness: 0.03,
            ..Default::default()
        }
    }

    /// ブラッシュメタル (Round 8): 異方性金属
    pub fn brushed_metal(color: glam::Vec3, anisotropy: f32) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(color.x, color.y, color.z, 1.0),
            metallic_factor: 1.0,
            roughness_factor: 0.3,
            anisotropy: anisotropy.clamp(-1.0, 1.0),
            anisotropy_direction: glam::Vec2::new(1.0, 0.0),
            ..Default::default()
        }
    }

    /// スキン (Round 8): SSS 付き誘電体
    pub fn skin(base_color: glam::Vec3) -> Self {
        Self {
            base_color_factor: glam::Vec4::new(base_color.x, base_color.y, base_color.z, 1.0),
            metallic_factor: 0.0,
            roughness_factor: 0.6,
            subsurface: 0.5,
            subsurface_color: glam::Vec3::new(0.9, 0.3, 0.25),
            ..Default::default()
        }
    }
}

/// GPU に送信する PBR マテリアルユニフォーム (Round 8: 128 B に拡張)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
    /// x = metallic, y = roughness, z = normal_scale, w = occlusion_strength
    pub mr_no: [f32; 4],
    /// xyz = emissive, w = padding
    pub emissive: [f32; 4],
    /// テクスチャ存在ビットフラグ
    /// bit 0 = base_color, 1 = MR, 2 = normal, 3 = occlusion, 4 = emissive
    pub texture_flags: [u32; 4],
    /// Round 8: x = clearcoat, y = clearcoat_roughness, z = anisotropy, w = subsurface
    pub extended: [f32; 4],
    /// Round 8: xy = anisotropy_direction, zw = _
    pub aniso_dir: [f32; 4],
    /// Round 8: rgb = subsurface_color, w = _
    pub subsurface_color: [f32; 4],
    /// Round 8: padding to align to 128 B
    pub _padding2: [f32; 4],
}

impl MaterialUniform {
    pub fn from_material(mat: &Material) -> Self {
        let mut flags = 0u32;
        if mat.base_color_map.is_some() {
            flags |= 1 << 0;
        }
        if mat.metallic_roughness_map.is_some() {
            flags |= 1 << 1;
        }
        if mat.normal_map.is_some() {
            flags |= 1 << 2;
        }
        if mat.occlusion_map.is_some() {
            flags |= 1 << 3;
        }
        if mat.emissive_map.is_some() {
            flags |= 1 << 4;
        }
        Self {
            base_color_factor: mat.base_color_factor.to_array(),
            mr_no: [
                mat.metallic_factor,
                mat.roughness_factor,
                mat.normal_scale,
                mat.occlusion_strength,
            ],
            emissive: [
                mat.emissive_factor.x,
                mat.emissive_factor.y,
                mat.emissive_factor.z,
                0.0,
            ],
            texture_flags: [flags, 0, 0, 0],
            extended: [
                mat.clearcoat,
                mat.clearcoat_roughness,
                mat.anisotropy,
                mat.subsurface,
            ],
            aniso_dir: [
                mat.anisotropy_direction.x,
                mat.anisotropy_direction.y,
                0.0,
                0.0,
            ],
            subsurface_color: [
                mat.subsurface_color.x,
                mat.subsurface_color.y,
                mat.subsurface_color.z,
                0.0,
            ],
            _padding2: [0.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_default() {
        let mat = Material::default();
        assert_eq!(mat.metallic_factor, 0.0);
        assert!(mat.base_color_map.is_none());
        assert!((mat.base_color_factor.w - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_material_uniform_no_texture_flags_zero() {
        let mat = Material::default();
        let uniform = MaterialUniform::from_material(&mat);
        assert_eq!(uniform.texture_flags[0], 0);
    }

    #[test]
    fn test_material_uniform_size_alignment() {
        // Round 8: 拡張後のサイズ
        assert_eq!(std::mem::size_of::<MaterialUniform>(), 128);
        assert_eq!(std::mem::size_of::<MaterialUniform>() % 16, 0);
    }

    #[test]
    fn test_metallic_clamps_roughness() {
        let mat = Material::metallic(glam::Vec3::ONE, 0.0);
        assert!((mat.roughness_factor - 0.04).abs() < 1e-5);
        assert!((mat.metallic_factor - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_flat_color_white_alpha() {
        let mat = Material::flat_color(glam::Vec3::new(0.5, 0.5, 0.5));
        assert!((mat.base_color_factor.w - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_car_paint() {
        let mat = Material::car_paint(glam::Vec3::new(1.0, 0.0, 0.0));
        assert!((mat.clearcoat - 1.0).abs() < 1e-5);
        assert!((mat.metallic_factor - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_brushed_metal_anisotropy() {
        let mat = Material::brushed_metal(glam::Vec3::ONE, 0.5);
        assert!((mat.anisotropy - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_skin_has_sss() {
        let mat = Material::skin(glam::Vec3::new(0.8, 0.6, 0.5));
        assert!(mat.subsurface > 0.0);
        assert!(mat.subsurface_color.x > 0.5);
    }

    #[test]
    fn test_extended_uniform_fields() {
        let mat = Material::car_paint(glam::Vec3::new(0.5, 0.5, 0.5));
        let u = MaterialUniform::from_material(&mat);
        assert!((u.extended[0] - 1.0).abs() < 1e-5); // clearcoat
        assert!((u.extended[1] - 0.03).abs() < 1e-5); // clearcoat_roughness
    }
}
