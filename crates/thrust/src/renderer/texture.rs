use std::path::Path;

use crate::error::{ThrustError, ThrustResult};

/// GPU テクスチャリソース
pub struct ThrustTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl ThrustTexture {
    /// バイト列から画像テクスチャを作成
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
    ) -> ThrustResult<Self> {
        let img = image::load_from_memory(bytes)?;
        let rgba = img.to_rgba8();
        let (width, height) = (rgba.width(), rgba.height());

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{label} Sampler")),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }

    /// 生の RGBA ピクセルデータからテクスチャを作成
    pub fn from_rgba_data(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: &str,
    ) -> ThrustResult<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|p| p.checked_mul(4))
            .ok_or_else(|| {
                ThrustError::TextureData(
                    "テクスチャサイズが大きすぎます（オーバーフロー）".to_string(),
                )
            })?;
        if rgba.len() != expected {
            return Err(ThrustError::TextureData(format!(
                "RGBAデータサイズが不正です: 期待 {expected} バイト、実際 {} バイト",
                rgba.len()
            )));
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{label} Sampler")),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }

    /// ファイルパスから画像テクスチャを作成
    pub fn from_path(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &Path,
    ) -> ThrustResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| ThrustError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        Self::from_bytes(device, queue, &bytes, &path.display().to_string())
    }

    /// 1x1 白テクスチャ（テクスチャなしマテリアル用フォールバック）
    pub fn white_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::single_pixel_srgb(device, queue, [255, 255, 255, 255], "White Pixel")
    }

    /// 1x1 ノーマルマップフォールバック (RGB = 128,128,255 = (0,0,1) 法線)
    ///
    /// 注意: ノーマルマップは線形空間が必要なため Rgba8Unorm を使用する。
    pub fn flat_normal_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::single_pixel_linear(device, queue, [128, 128, 255, 255], "Flat Normal Pixel")
    }

    /// 1x1 MR/AO 系フォールバック (G=255 → roughness=1, B=0 → metallic=0, R=255 → AO=1)
    pub fn flat_mr_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::single_pixel_linear(device, queue, [255, 255, 0, 255], "Flat MR Pixel")
    }

    /// 1x1 黒テクスチャ (emissive 等のフォールバック)
    pub fn black_pixel(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::single_pixel_srgb(device, queue, [0, 0, 0, 255], "Black Pixel")
    }

    fn single_pixel_srgb(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: [u8; 4],
        label: &'static str,
    ) -> Self {
        Self::single_pixel(
            device,
            queue,
            rgba,
            label,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    }

    fn single_pixel_linear(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: [u8; 4],
        label: &'static str,
    ) -> Self {
        Self::single_pixel(device, queue, rgba, label, wgpu::TextureFormat::Rgba8Unorm)
    }

    fn single_pixel(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: [u8; 4],
        label: &'static str,
        format: wgpu::TextureFormat,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(label),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }
}
