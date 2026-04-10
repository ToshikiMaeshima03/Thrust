//! Screen-Space Global Illumination (Round 9)
//!
//! Lumen 風の簡易 SSGI: depth + normal + HDR をサンプリングし、各ピクセルから
//! 半球方向に ray-march して 1 バウンス間接光を計算する。
//! 完全な GI には voxel cone tracing や RTX が必要だが、ここでは
//! スクリーン空間の近似で視覚効果を得る。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::renderer::prepass::GeometryPrepass;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SsgiUniform {
    /// x = max_distance, y = num_steps, z = num_rays, w = strength
    pub params: [f32; 4],
    /// x = bounce_intensity, y = thickness, z = noise_scale, w = enabled
    pub extra: [f32; 4],
}

impl Default for SsgiUniform {
    fn default() -> Self {
        Self {
            params: [10.0, 16.0, 8.0, 1.0],
            extra: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

pub struct Ssgi {
    pub uniform: SsgiUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub gi_texture: wgpu::Texture,
    pub gi_view: wgpu::TextureView,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl Ssgi {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
        hdr_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let gi_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSGI GI Texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let gi_view = gi_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = SsgiUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSGI Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSGI BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSGI BG"),
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
                    resource: wgpu::BindingResource::TextureView(&prepass.normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSGI Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SSGI_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSGI PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSGI Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_fullscreen"),
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            uniform,
            uniform_buffer,
            gi_texture,
            gi_view,
            bind_group_layout,
            bind_group,
            pipeline,
            width: w,
            height: h,
        }
    }

    pub fn set_params(
        &mut self,
        queue: &wgpu::Queue,
        max_distance: f32,
        num_rays: u32,
        strength: f32,
    ) {
        self.uniform.params[0] = max_distance;
        self.uniform.params[2] = num_rays as f32;
        self.uniform.params[3] = strength;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn set_enabled(&mut self, queue: &wgpu::Queue, enabled: bool) {
        self.uniform.extra[3] = if enabled { 1.0 } else { 0.0 };
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
    fn test_ssgi_uniform_size() {
        assert_eq!(std::mem::size_of::<SsgiUniform>(), 32);
    }

    #[test]
    fn test_ssgi_default_enabled() {
        let u = SsgiUniform::default();
        assert!(u.extra[3] > 0.5);
    }

    #[test]
    fn test_ssgi_default_rays() {
        let u = SsgiUniform::default();
        assert_eq!(u.params[2], 8.0);
    }
}
