//! Skybox + IBL 環境 (Round 4)
//!
//! プロシージャル sky (デフォルト) または HDR equirect → cubemap (オプション)。
//! Cubemap が設定されると IBL の環境光として PBR シェーダーから参照される。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::{HDR_FORMAT, MSAA_SAMPLES};
use crate::shader;

/// Skybox uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SkyboxUniform {
    /// xyz = 太陽方向, w = use_cubemap (0=procedural, 1=cubemap)
    pub sun_dir: [f32; 4],
    /// rgb = 地平線色, a = HDR 強度
    pub horizon: [f32; 4],
    /// rgb = 天頂色, a = _
    pub zenith: [f32; 4],
}

impl Default for SkyboxUniform {
    fn default() -> Self {
        Self {
            sun_dir: [-0.5, -0.7, -0.5, 0.0],
            horizon: [0.7, 0.85, 1.0, 1.0],
            zenith: [0.2, 0.4, 0.8, 0.0],
        }
    }
}

/// Skybox レンダリングリソース
pub struct Skybox {
    pub uniform: SkyboxUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub cubemap_texture: wgpu::Texture,
    pub cubemap_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
}

impl Skybox {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, camera_buffer: &wgpu::Buffer) -> Self {
        // フォールバック cubemap (1x1 黒、6 面)
        let cubemap_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Skybox Cubemap"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // 黒データを書き込む (Rgba16Float = 8 bytes per pixel)
        let black: [u8; 8] = [0; 8];
        for layer in 0..6 {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &cubemap_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &black,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        let cubemap_view = cubemap_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Skybox Cubemap View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Skybox Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let uniform = SkyboxUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Skybox Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Skybox BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Skybox Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&cubemap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // パイプライン
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Skybox Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SKYBOX_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Skybox Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Skybox Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            uniform,
            uniform_buffer,
            cubemap_texture,
            cubemap_view,
            sampler,
            bind_group_layout,
            bind_group,
            pipeline,
        }
    }

    /// 太陽方向と地平線/天頂色を更新する
    pub fn set_sun(&mut self, queue: &wgpu::Queue, sun_dir: glam::Vec3, intensity: f32) {
        let n = sun_dir.normalize_or(glam::Vec3::NEG_Y);
        self.uniform.sun_dir = [n.x, n.y, n.z, 0.0];
        self.uniform.horizon[3] = intensity;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn set_colors(
        &mut self,
        queue: &wgpu::Queue,
        horizon: glam::Vec3,
        zenith: glam::Vec3,
        intensity: f32,
    ) {
        self.uniform.horizon = [horizon.x, horizon.y, horizon.z, intensity];
        self.uniform.zenith = [zenith.x, zenith.y, zenith.z, 0.0];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skybox_uniform_size() {
        // 16 + 16 + 16 = 48 B
        assert_eq!(std::mem::size_of::<SkyboxUniform>(), 48);
        assert_eq!(std::mem::size_of::<SkyboxUniform>() % 16, 0);
    }

    #[test]
    fn test_skybox_uniform_default() {
        let s = SkyboxUniform::default();
        assert!((s.horizon[3] - 1.0).abs() < 1e-5);
        assert!(s.sun_dir[3] < 0.5);
    }
}
