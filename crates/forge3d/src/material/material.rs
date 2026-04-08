use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::renderer::texture::ForgeTexture;

/// オブジェクトのマテリアル情報
pub struct Material {
    pub base_color: glam::Vec4,
    pub texture: Option<Arc<ForgeTexture>>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: glam::Vec4::new(0.8, 0.6, 0.4, 1.0),
            texture: None,
        }
    }
}

/// GPU に送信するマテリアルユニフォーム
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color: [f32; 4],
    pub has_texture: u32,
    pub _padding: [u32; 3],
}

impl MaterialUniform {
    pub fn from_material(mat: &Material) -> Self {
        Self {
            base_color: mat.base_color.to_array(),
            has_texture: u32::from(mat.texture.is_some()),
            _padding: [0; 3],
        }
    }
}
