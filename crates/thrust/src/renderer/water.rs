//! 水レンダリング (Round 8)
//!
//! Gerstner 波 (4 波加算) + フレネル反射 + ノーマルマップ + 半透明合成。
//! 水面メッシュは別途用意し、`Water` コンポーネントを付与する。
//! 専用パイプラインを `WaterRenderer` に保持する。

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::mesh::vertex::Vertex;
use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::renderer::post::{HDR_FORMAT, MSAA_SAMPLES};
use crate::renderer::texture::ThrustTexture;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct WaterUniform {
    pub shallow_color: [f32; 4],
    pub deep_color: [f32; 4],
    /// x = wave_amplitude, y = wave_frequency, z = wave_speed, w = num_waves
    pub wave_params: [f32; 4],
    /// xy = wind_dir, z = fresnel_power, w = reflectivity
    pub extra: [f32; 4],
}

impl Default for WaterUniform {
    fn default() -> Self {
        Self {
            shallow_color: [0.4, 0.7, 0.85, 0.7],
            deep_color: [0.05, 0.15, 0.3, 1.0],
            wave_params: [0.15, 1.5, 1.0, 4.0],
            extra: [1.0, 0.0, 5.0, 0.7],
        }
    }
}

/// 水面コンポーネント (ECS で使用)
pub struct Water {
    pub uniform: WaterUniform,
    pub normal_map: Arc<ThrustTexture>,
}

impl Water {
    pub fn new(normal_map: Arc<ThrustTexture>) -> Self {
        Self {
            uniform: WaterUniform::default(),
            normal_map,
        }
    }
}

/// 水面レンダリングリソース
pub struct WaterRenderer {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
}

impl WaterRenderer {
    pub fn new(device: &wgpu::Device, layouts: &ThrustBindGroupLayouts) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Water BGL"),
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
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Water Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::WATER_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Water Pipeline Layout"),
            bind_group_layouts: &[
                Some(&layouts.camera),
                Some(&layouts.model),
                Some(&bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            bind_group_layout,
            pipeline,
        }
    }

    /// 水面 uniform バッファとバインドグループを作成
    pub fn create_resources(
        &self,
        device: &wgpu::Device,
        water: &Water,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Water Uniform"),
            contents: bytemuck::cast_slice(&[water.uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&water.normal_map.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&water.normal_map.sampler),
                },
            ],
        });
        (buffer, bg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_uniform_size() {
        // 4 vec4 = 64 B
        assert_eq!(std::mem::size_of::<WaterUniform>(), 64);
        assert_eq!(std::mem::size_of::<WaterUniform>() % 16, 0);
    }

    #[test]
    fn test_water_default() {
        let u = WaterUniform::default();
        assert!(u.wave_params[0] > 0.0); // amplitude
        assert!(u.wave_params[2] > 0.0); // speed
    }
}
