use bytemuck::{Pod, Zeroable};

/// 平行光源
pub struct DirectionalLight {
    pub direction: glam::Vec3,
    pub color: glam::Vec3,
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: glam::Vec3::new(1.0, 1.0, 1.0).normalize(),
            color: glam::Vec3::ONE,
            intensity: 0.85,
        }
    }
}

/// 環境光
pub struct AmbientLight {
    pub color: glam::Vec3,
    pub intensity: f32,
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: glam::Vec3::ONE,
            intensity: 0.15,
        }
    }
}

/// GPU に送信するライトユニフォーム（48 bytes、16 バイトアライメント）
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LightUniform {
    pub direction: [f32; 3],
    pub intensity: f32,
    pub color: [f32; 3],
    pub ambient_intensity: f32,
    pub ambient_color: [f32; 3],
    pub _padding: f32,
}

impl LightUniform {
    pub fn new(dir: &DirectionalLight, ambient: &AmbientLight) -> Self {
        Self {
            direction: dir.direction.normalize().to_array(),
            intensity: dir.intensity,
            color: dir.color.to_array(),
            ambient_intensity: ambient.intensity,
            ambient_color: ambient.color.to_array(),
            _padding: 0.0,
        }
    }
}
