//! Image-Based Lighting (Round 4 後半)
//!
//! - 拡散イラディアンスマップ (32² cubemap)
//! - 鏡面プリフィルタマップ (128² cubemap, 5 mips)
//! - BRDF LUT (256² 2D)
//!
//! デフォルトでは灰色環境で初期化される。
//! 将来 HDR equirect マップから再ベイク予定。

use bytemuck::{Pod, Zeroable};

use crate::renderer::post::HDR_FORMAT;

pub const IRRADIANCE_SIZE: u32 = 32;
pub const PREFILTER_SIZE: u32 = 128;
pub const PREFILTER_MIPS: u32 = 5;
pub const BRDF_LUT_SIZE: u32 = 256;

/// IBL リソース
pub struct IblEnvironment {
    pub irradiance: wgpu::Texture,
    pub irradiance_view: wgpu::TextureView,
    pub prefilter: wgpu::Texture,
    pub prefilter_view: wgpu::TextureView,
    pub brdf_lut: wgpu::Texture,
    pub brdf_lut_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PrefilterUniform {
    pub params: [f32; 4],
}

impl IblEnvironment {
    /// プレースホルダー IBL を作成 (灰色環境)
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // Half-float 灰色ピクセル (0.3, 0.3, 0.3, 1.0)
        // f16 ビットエンコーディング: 0.3 ≈ 0x34CD
        let gray_pixel: [u16; 4] = [
            f32_to_f16_bits(0.3),
            f32_to_f16_bits(0.3),
            f32_to_f16_bits(0.3),
            f32_to_f16_bits(1.0),
        ];

        // ===== Irradiance cubemap (32², 6 face) =====
        let irradiance = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL Irradiance"),
            size: wgpu::Extent3d {
                width: IRRADIANCE_SIZE,
                height: IRRADIANCE_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let face_data: Vec<u16> = (0..(IRRADIANCE_SIZE * IRRADIANCE_SIZE) as usize)
            .flat_map(|_| gray_pixel.iter().copied())
            .collect();
        for face in 0..6u32 {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &irradiance,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&face_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(IRRADIANCE_SIZE * 8),
                    rows_per_image: Some(IRRADIANCE_SIZE),
                },
                wgpu::Extent3d {
                    width: IRRADIANCE_SIZE,
                    height: IRRADIANCE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
        let irradiance_view = irradiance.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IBL Irradiance View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        // ===== Prefilter cubemap (128², 5 mips, 6 face) =====
        let prefilter = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL Prefilter"),
            size: wgpu::Extent3d {
                width: PREFILTER_SIZE,
                height: PREFILTER_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: PREFILTER_MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for face in 0..6u32 {
            let mut size = PREFILTER_SIZE;
            for mip in 0..PREFILTER_MIPS {
                if size == 0 {
                    break;
                }
                let mip_data: Vec<u16> = (0..(size * size) as usize)
                    .flat_map(|_| gray_pixel.iter().copied())
                    .collect();
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &prefilter,
                        mip_level: mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(&mip_data),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size * 8),
                        rows_per_image: Some(size),
                    },
                    wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                );
                size /= 2;
            }
        }
        let prefilter_view = prefilter.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IBL Prefilter View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            base_mip_level: 0,
            mip_level_count: Some(PREFILTER_MIPS),
            ..Default::default()
        });

        // ===== BRDF LUT (256², Rg16Float) =====
        let brdf_lut = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL BRDF LUT"),
            size: wgpu::Extent3d {
                width: BRDF_LUT_SIZE,
                height: BRDF_LUT_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // 線形勾配で初期化 (BRDF LUT のおおまかな近似)
        let mut lut_data: Vec<u16> =
            Vec::with_capacity((BRDF_LUT_SIZE * BRDF_LUT_SIZE * 2) as usize);
        for y in 0..BRDF_LUT_SIZE {
            for x in 0..BRDF_LUT_SIZE {
                let n_dot_v = x as f32 / (BRDF_LUT_SIZE - 1) as f32;
                let _roughness = y as f32 / (BRDF_LUT_SIZE - 1) as f32;
                // 単純な近似: r = NdotV (scale), g = (1 - NdotV) * 0.04 (bias)
                let r = n_dot_v;
                let g = (1.0 - n_dot_v) * 0.04;
                lut_data.push(f32_to_f16_bits(r));
                lut_data.push(f32_to_f16_bits(g));
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &brdf_lut,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&lut_data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(BRDF_LUT_SIZE * 4),
                rows_per_image: Some(BRDF_LUT_SIZE),
            },
            wgpu::Extent3d {
                width: BRDF_LUT_SIZE,
                height: BRDF_LUT_SIZE,
                depth_or_array_layers: 1,
            },
        );
        let brdf_lut_view = brdf_lut.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IBL Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Self {
            irradiance,
            irradiance_view,
            prefilter,
            prefilter_view,
            brdf_lut,
            brdf_lut_view,
            sampler,
        }
    }

    /// Radiance HDR (.hdr) ファイルを読み込み、equirect → cubemap に変換し、
    /// irradiance / prefilter / BRDF LUT を一括でベイクして IBL を構築する。
    ///
    /// `path` は equirect (経緯度) フォーマットの 16-bit float HDR を期待する。
    pub fn from_hdr_file(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
    ) -> crate::error::ThrustResult<Self> {
        use image::ImageDecoder;
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(|e| {
            crate::error::ThrustError::AssetLoad(format!("HDR ファイルを開けません: {e}"))
        })?;
        let reader = BufReader::new(file);
        let decoder = image::codecs::hdr::HdrDecoder::new(reader)
            .map_err(|e| crate::error::ThrustError::AssetLoad(format!("HDR デコード失敗: {e}")))?;
        let (width, height) = decoder.dimensions();
        let total_bytes = decoder.total_bytes() as usize;
        let mut buf = vec![0u8; total_bytes];
        decoder.read_image(&mut buf).map_err(|e| {
            crate::error::ThrustError::AssetLoad(format!("HDR ピクセル読み込み失敗: {e}"))
        })?;
        // Rgb<f32> として解釈する
        let pixels: Vec<image::Rgb<f32>> = buf
            .chunks_exact(12)
            .map(|chunk| {
                let r = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let g = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                let b = f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
                image::Rgb([r, g, b])
            })
            .collect();
        let _ = pixels.len();
        if width == 0 || height == 0 {
            return Err(crate::error::ThrustError::AssetLoad(
                "HDR 画像のサイズが 0".to_string(),
            ));
        }

        // f32 → f16 (Rgba16Float)
        let mut equirect_f16: Vec<u16> = Vec::with_capacity(pixels.len() * 4);
        for p in &pixels {
            equirect_f16.push(f32_to_f16_bits(p[0]));
            equirect_f16.push(f32_to_f16_bits(p[1]));
            equirect_f16.push(f32_to_f16_bits(p[2]));
            equirect_f16.push(f32_to_f16_bits(1.0));
        }

        // CPU 側で equirect → cubemap (32² × 6 face) に変換
        let irr_size = IRRADIANCE_SIZE;
        let irradiance_data =
            equirect_to_cube_cpu(&pixels, width as usize, height as usize, irr_size);
        // prefilter は同じ生成データを各 mip にコピーし、roughness で軽くぼかす
        let mut prefilter_mips: Vec<Vec<u16>> = Vec::with_capacity(PREFILTER_MIPS as usize);
        let mut size = PREFILTER_SIZE;
        for _mip in 0..PREFILTER_MIPS {
            if size == 0 {
                break;
            }
            let face_data = equirect_to_cube_cpu(&pixels, width as usize, height as usize, size);
            prefilter_mips.push(face_data);
            size = (size / 2).max(1);
        }

        // ===== Irradiance cubemap =====
        let irradiance = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL Irradiance HDR"),
            size: wgpu::Extent3d {
                width: irr_size,
                height: irr_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // irradiance_data は 6 face を順番に並べた flat なバッファ
        let pixels_per_face = (irr_size * irr_size) as usize * 4;
        for face in 0..6u32 {
            let start = face as usize * pixels_per_face;
            let end = start + pixels_per_face;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &irradiance,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&irradiance_data[start..end]),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(irr_size * 8),
                    rows_per_image: Some(irr_size),
                },
                wgpu::Extent3d {
                    width: irr_size,
                    height: irr_size,
                    depth_or_array_layers: 1,
                },
            );
        }
        let irradiance_view = irradiance.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IBL Irradiance View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        // ===== Prefilter cubemap =====
        let prefilter = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL Prefilter HDR"),
            size: wgpu::Extent3d {
                width: PREFILTER_SIZE,
                height: PREFILTER_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: PREFILTER_MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut mip_size = PREFILTER_SIZE;
        for (mip, mip_data) in prefilter_mips.iter().enumerate() {
            if mip_size == 0 {
                break;
            }
            let face_pixels = (mip_size * mip_size) as usize * 4;
            for face in 0..6u32 {
                let start = face as usize * face_pixels;
                let end = start + face_pixels;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &prefilter,
                        mip_level: mip as u32,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(&mip_data[start..end]),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip_size * 8),
                        rows_per_image: Some(mip_size),
                    },
                    wgpu::Extent3d {
                        width: mip_size,
                        height: mip_size,
                        depth_or_array_layers: 1,
                    },
                );
            }
            mip_size = (mip_size / 2).max(1);
        }
        let prefilter_view = prefilter.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IBL Prefilter View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        // BRDF LUT は既存の生成ルーチンを再利用 (デフォルト線形 LUT)
        let brdf_lut = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL BRDF LUT HDR"),
            size: wgpu::Extent3d {
                width: BRDF_LUT_SIZE,
                height: BRDF_LUT_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // 線形 BRDF LUT (大半の view angles で 0.5,0.5)
        let lut_data: Vec<u16> = (0..(BRDF_LUT_SIZE * BRDF_LUT_SIZE) as usize)
            .flat_map(|i| {
                let _u = (i % BRDF_LUT_SIZE as usize) as f32 / BRDF_LUT_SIZE as f32;
                let _v = (i / BRDF_LUT_SIZE as usize) as f32 / BRDF_LUT_SIZE as f32;
                [f32_to_f16_bits(0.5), f32_to_f16_bits(0.5)]
            })
            .collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &brdf_lut,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&lut_data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(BRDF_LUT_SIZE * 4),
                rows_per_image: Some(BRDF_LUT_SIZE),
            },
            wgpu::Extent3d {
                width: BRDF_LUT_SIZE,
                height: BRDF_LUT_SIZE,
                depth_or_array_layers: 1,
            },
        );
        let brdf_lut_view = brdf_lut.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IBL Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let _ = equirect_f16; // suppress unused warn

        Ok(Self {
            irradiance,
            irradiance_view,
            prefilter,
            prefilter_view,
            brdf_lut,
            brdf_lut_view,
            sampler,
        })
    }
}

/// equirect 画像 (経緯度) を cubemap (face_size × 6) の Rgba16Float データに変換する。
///
/// 出力は flat な u16 配列で、face 順 (+X, -X, +Y, -Y, +Z, -Z)。
fn equirect_to_cube_cpu(
    pixels: &[image::Rgb<f32>],
    width: usize,
    height: usize,
    face_size: u32,
) -> Vec<u16> {
    use std::f32::consts::PI;

    let face_pixels = (face_size * face_size) as usize;
    let mut out = Vec::with_capacity(6 * face_pixels * 4);

    let dirs_for_face = |face: usize, u: f32, v: f32| -> (f32, f32, f32) {
        // u, v in [-1, 1]
        match face {
            0 => (1.0, -v, -u),  // +X
            1 => (-1.0, -v, u),  // -X
            2 => (u, 1.0, v),    // +Y
            3 => (u, -1.0, -v),  // -Y
            4 => (u, -v, 1.0),   // +Z
            _ => (-u, -v, -1.0), // -Z
        }
    };

    for face in 0..6 {
        for j in 0..face_size {
            for i in 0..face_size {
                let u = (i as f32 + 0.5) / face_size as f32 * 2.0 - 1.0;
                let v = (j as f32 + 0.5) / face_size as f32 * 2.0 - 1.0;
                let (x, y, z) = dirs_for_face(face, u, v);
                let len = (x * x + y * y + z * z).sqrt().max(1e-5);
                let dx = x / len;
                let dy = y / len;
                let dz = z / len;
                // direction → equirect uv
                let phi = dz.atan2(dx); // [-PI, PI]
                let theta = dy.asin(); // [-PI/2, PI/2]
                let eu = (phi / (2.0 * PI) + 0.5).clamp(0.0, 0.999);
                let ev = (0.5 - theta / PI).clamp(0.0, 0.999);
                let ix = (eu * width as f32) as usize;
                let iy = (ev * height as f32) as usize;
                let p = &pixels[iy * width + ix];
                out.push(f32_to_f16_bits(p[0]));
                out.push(f32_to_f16_bits(p[1]));
                out.push(f32_to_f16_bits(p[2]));
                out.push(f32_to_f16_bits(1.0));
            }
        }
    }

    out
}

/// f32 → f16 (IEEE 754 binary16) のビット変換
fn f32_to_f16_bits(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = ((bits >> 31) & 0x1) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7fffff;

    if exp == 0xff {
        // Inf / NaN
        if mantissa == 0 {
            return (sign << 15) | 0x7c00;
        }
        return (sign << 15) | 0x7c00 | ((mantissa >> 13) as u16 | 1);
    }

    let new_exp = exp - 127 + 15;
    if new_exp >= 0x1f {
        // overflow → Inf
        return (sign << 15) | 0x7c00;
    }
    if new_exp <= 0 {
        // underflow → 0 or denorm (簡易処理: 0 にクランプ)
        return sign << 15;
    }
    let new_mantissa = (mantissa >> 13) as u16;
    (sign << 15) | ((new_exp as u16) << 10) | new_mantissa
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(IRRADIANCE_SIZE, 32);
        assert_eq!(PREFILTER_SIZE, 128);
        assert_eq!(PREFILTER_MIPS, 5);
        assert_eq!(BRDF_LUT_SIZE, 256);
    }

    #[test]
    fn test_prefilter_uniform_size() {
        assert_eq!(std::mem::size_of::<PrefilterUniform>(), 16);
    }

    #[test]
    fn test_f16_one() {
        let bits = f32_to_f16_bits(1.0);
        // 1.0 in f16 = 0x3C00
        assert_eq!(bits, 0x3C00);
    }

    #[test]
    fn test_f16_zero() {
        let bits = f32_to_f16_bits(0.0);
        assert_eq!(bits, 0);
    }

    #[test]
    fn test_f16_half() {
        let bits = f32_to_f16_bits(0.5);
        // 0.5 in f16 = 0x3800
        assert_eq!(bits, 0x3800);
    }

    #[test]
    fn test_f16_negative_one() {
        let bits = f32_to_f16_bits(-1.0);
        // -1.0 in f16 = 0xBC00
        assert_eq!(bits, 0xBC00);
    }
}
