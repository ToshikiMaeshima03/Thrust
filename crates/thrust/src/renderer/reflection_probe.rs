//! リフレクションプローブ (Round 8)
//!
//! 特定のワールド位置で 6 face cubemap をレンダリングし、IBL prefilter として使う。
//! 静的シーンでは 1 度ベイクすればよく、動的シーンでは N フレームに 1 度更新する。
//!
//! ECS で `ReflectionProbe` コンポーネントを spawn すると、`reflection_probe_system`
//! が cubemap をベイクする。複数プローブのブレンドはオブジェクト側でブレンド距離を指定する。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;

/// 単一のリフレクションプローブ (ECS コンポーネント)
pub struct ReflectionProbe {
    /// プローブのワールド位置
    pub position: glam::Vec3,
    /// プローブの影響半径 (オブジェクトがこの距離以内なら使用)
    pub radius: f32,
    /// 周囲ボックス (オプション、parallax 補正用)
    pub box_extent: Option<glam::Vec3>,
    /// 解像度 (デフォルト 128)
    pub resolution: u32,
    /// 内部で生成: cubemap texture と view
    pub(crate) cubemap: Option<wgpu::Texture>,
    pub(crate) cube_view: Option<wgpu::TextureView>,
    pub(crate) face_views: Option<[wgpu::TextureView; 6]>,
    /// このプローブのベイク済みフラグ
    pub baked: bool,
    /// dynamic update interval (sec)、0 ならベイクしない
    pub update_interval: f32,
    pub time_since_bake: f32,
}

impl ReflectionProbe {
    pub fn new(position: glam::Vec3, radius: f32) -> Self {
        Self {
            position,
            radius,
            box_extent: None,
            resolution: 128,
            cubemap: None,
            cube_view: None,
            face_views: None,
            baked: false,
            update_interval: 0.0,
            time_since_bake: 0.0,
        }
    }

    pub fn with_box_extent(mut self, extent: glam::Vec3) -> Self {
        self.box_extent = Some(extent);
        self
    }

    pub fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn with_dynamic_update(mut self, interval_sec: f32) -> Self {
        self.update_interval = interval_sec;
        self
    }

    /// cubemap textures を初期化する
    pub fn ensure_cubemap(&mut self, device: &wgpu::Device) {
        if self.cubemap.is_some() {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Reflection Probe Cubemap"),
            size: wgpu::Extent3d {
                width: self.resolution,
                height: self.resolution,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let cube_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Reflection Probe Cube View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });
        let face_views: [wgpu::TextureView; 6] = std::array::from_fn(|face| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Reflection Probe Face {face}")),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        self.cubemap = Some(texture);
        self.cube_view = Some(cube_view);
        self.face_views = Some(face_views);
    }
}

/// プローブ管理用の uniform (シェーダーには使わないがメタ情報として)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ReflectionProbeUniform {
    /// xyz = position, w = radius
    pub position_radius: [f32; 4],
    /// xyz = box extent (0 if disabled), w = active
    pub box_active: [f32; 4],
}

impl Default for ReflectionProbeUniform {
    fn default() -> Self {
        Self {
            position_radius: [0.0, 0.0, 0.0, 1.0],
            box_active: [0.0; 4],
        }
    }
}

/// 6 face VP (cube ベイク用)
pub fn cube_face_views(position: glam::Vec3, near: f32, far: f32) -> [glam::Mat4; 6] {
    let proj = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    let dirs_ups: [(glam::Vec3, glam::Vec3); 6] = [
        (glam::Vec3::X, glam::Vec3::NEG_Y),
        (glam::Vec3::NEG_X, glam::Vec3::NEG_Y),
        (glam::Vec3::Y, glam::Vec3::Z),
        (glam::Vec3::NEG_Y, glam::Vec3::NEG_Z),
        (glam::Vec3::Z, glam::Vec3::NEG_Y),
        (glam::Vec3::NEG_Z, glam::Vec3::NEG_Y),
    ];
    let mut out = [glam::Mat4::IDENTITY; 6];
    for (i, (dir, up)) in dirs_ups.iter().enumerate() {
        let view = glam::Mat4::look_at_rh(position, position + *dir, *up);
        out[i] = proj * view;
    }
    out
}

/// グレースケールでプローブを初期化する (placeholder)
pub fn fill_probe_with_color(queue: &wgpu::Queue, probe: &ReflectionProbe, color: glam::Vec3) {
    let Some(texture) = probe.cubemap.as_ref() else {
        return;
    };
    let pixel: [u16; 4] = [
        f32_to_f16_bits(color.x),
        f32_to_f16_bits(color.y),
        f32_to_f16_bits(color.z),
        f32_to_f16_bits(1.0),
    ];
    let pixels: Vec<u16> = (0..(probe.resolution * probe.resolution) as usize)
        .flat_map(|_| pixel.iter().copied())
        .collect();
    for face in 0..6u32 {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: face,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(probe.resolution * 8),
                rows_per_image: Some(probe.resolution),
            },
            wgpu::Extent3d {
                width: probe.resolution,
                height: probe.resolution,
                depth_or_array_layers: 1,
            },
        );
    }
}

fn f32_to_f16_bits(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = ((bits >> 31) & 0x1) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7fffff;

    if exp == 0xff {
        if mantissa == 0 {
            return (sign << 15) | 0x7c00;
        }
        return (sign << 15) | 0x7c00 | ((mantissa >> 13) as u16 | 1);
    }
    let new_exp = exp - 127 + 15;
    if new_exp >= 0x1f {
        return (sign << 15) | 0x7c00;
    }
    if new_exp <= 0 {
        return sign << 15;
    }
    let new_mantissa = (mantissa >> 13) as u16;
    (sign << 15) | ((new_exp as u16) << 10) | new_mantissa
}

/// `world.spawn` 後に呼ぶ。Buffer を作成し、デフォルト色で初期化する
pub fn init_probe_resources(
    probe: &mut ReflectionProbe,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    init_color: glam::Vec3,
) -> wgpu::Buffer {
    probe.ensure_cubemap(device);
    fill_probe_with_color(queue, probe, init_color);
    let uniform = ReflectionProbeUniform {
        position_radius: [
            probe.position.x,
            probe.position.y,
            probe.position.z,
            probe.radius,
        ],
        box_active: probe
            .box_extent
            .map(|e| [e.x, e.y, e.z, 1.0])
            .unwrap_or([0.0; 4]),
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Reflection Probe Uniform"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_default() {
        let p = ReflectionProbe::new(glam::Vec3::ZERO, 5.0);
        assert!((p.radius - 5.0).abs() < 1e-5);
        assert_eq!(p.resolution, 128);
        assert!(!p.baked);
    }

    #[test]
    fn test_probe_uniform_size() {
        assert_eq!(std::mem::size_of::<ReflectionProbeUniform>(), 32);
    }

    #[test]
    fn test_cube_face_views_count() {
        let views = cube_face_views(glam::Vec3::ZERO, 0.1, 100.0);
        assert_eq!(views.len(), 6);
        for v in &views {
            for col in 0..4 {
                for row in 0..4 {
                    assert!(v.col(col)[row].is_finite());
                }
            }
        }
    }

    #[test]
    fn test_with_box_extent() {
        let p = ReflectionProbe::new(glam::Vec3::ZERO, 5.0)
            .with_box_extent(glam::Vec3::new(10.0, 5.0, 8.0));
        assert!(p.box_extent.is_some());
    }
}
