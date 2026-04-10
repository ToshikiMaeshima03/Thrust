//! ボリュメトリックフォグ (Round 5)
//!
//! 指数高さフォグ + 太陽方向への光散乱 (シェーダー側で適用)。
//! `FogParams` リソースをアプリ側で書き換えるとリアルタイムでフォグが変化する。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// フォグパラメータ
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FogUniform {
    /// xyz = フォグ色, w = 密度 (0 = 無効)
    pub color_density: [f32; 4],
    /// x = 高さ減衰係数, y = 基準高さ, z = 散乱強度, w = max distance
    pub params: [f32; 4],
}

impl Default for FogUniform {
    fn default() -> Self {
        Self {
            // デフォルトは無効 (density = 0)
            color_density: [0.6, 0.7, 0.8, 0.0],
            params: [0.05, 0.0, 1.0, 200.0],
        }
    }
}

impl FogUniform {
    /// 標準的な屋外フォグ
    pub fn outdoor(color: glam::Vec3, density: f32) -> Self {
        Self {
            color_density: [color.x, color.y, color.z, density],
            params: [0.05, 0.0, 1.0, 200.0],
        }
    }

    /// 濃霧
    pub fn dense(color: glam::Vec3) -> Self {
        Self {
            color_density: [color.x, color.y, color.z, 0.05],
            params: [0.1, 0.0, 0.5, 50.0],
        }
    }
}

/// フォグリソース
pub struct Fog {
    pub uniform: FogUniform,
    pub buffer: wgpu::Buffer,
}

impl Fog {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform = FogUniform::default();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fog Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self { uniform, buffer }
    }

    /// パラメータを更新して GPU に書き込む
    pub fn update(&mut self, queue: &wgpu::Queue, uniform: FogUniform) {
        self.uniform = uniform;
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fog_uniform_size() {
        // 16 + 16 = 32 B
        assert_eq!(std::mem::size_of::<FogUniform>(), 32);
        assert_eq!(std::mem::size_of::<FogUniform>() % 16, 0);
    }

    #[test]
    fn test_fog_default_disabled() {
        let f = FogUniform::default();
        assert!(f.color_density[3] < 0.001, "default fog should be disabled");
    }

    #[test]
    fn test_fog_outdoor_helper() {
        let f = FogUniform::outdoor(glam::Vec3::new(0.7, 0.8, 0.9), 0.02);
        assert!((f.color_density[3] - 0.02).abs() < 1e-5);
        assert!((f.color_density[0] - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_fog_dense_helper() {
        let f = FogUniform::dense(glam::Vec3::splat(0.8));
        assert!(f.color_density[3] > 0.0);
        assert!(f.params[3] < 100.0); // 短い max distance
    }
}
