//! Screen-Space Reflections (Round 7)
//!
//! Linear ray-marching を view 空間で行い、反射ベクトルが当たるピクセルから
//! HDR カラーを取得する。Roughness によって反射強度を変える。
//! 結果は別テクスチャ (Rgba16Float) に書き出され、tonemap パスで合成される。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::renderer::prepass::GeometryPrepass;
use crate::shader;

/// SSR パラメータ
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SsrUniform {
    /// x = max_distance, y = thickness, z = max_steps, w = strength
    pub params: [f32; 4],
    /// x = roughness_cutoff, y = fade_distance, z = jitter_strength, w = _
    pub extra: [f32; 4],
}

impl Default for SsrUniform {
    fn default() -> Self {
        Self {
            params: [50.0, 1.0, 32.0, 1.0],
            extra: [0.6, 50.0, 1.0, 0.0],
        }
    }
}

/// SSR リソース
pub struct Ssr {
    pub uniform: SsrUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub reflection_texture: wgpu::Texture,
    pub reflection_view: wgpu::TextureView,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl Ssr {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
        hdr_resolved_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let reflection_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Reflection Texture"),
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
        let reflection_view =
            reflection_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = SsrUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSR Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSR BGL"),
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
                // 2: normal (Rgba16Float)
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
                // 3: material (Rgba8Unorm)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 4: depth
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                // 5: hdr resolved
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Bind Group"),
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
                    resource: wgpu::BindingResource::TextureView(&prepass.material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(hdr_resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSR Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SSR_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSR PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSR Pipeline"),
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
            reflection_texture,
            reflection_view,
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
        thickness: f32,
        max_steps: u32,
        strength: f32,
    ) {
        self.uniform.params = [max_distance, thickness, max_steps as f32, strength];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn rebuild_bind_groups(
        &mut self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
        hdr_resolved_view: &wgpu::TextureView,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&prepass.normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prepass.material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(hdr_resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssr_uniform_size() {
        assert_eq!(std::mem::size_of::<SsrUniform>(), 32);
        assert_eq!(std::mem::size_of::<SsrUniform>() % 16, 0);
    }

    #[test]
    fn test_ssr_default_params() {
        let u = SsrUniform::default();
        assert!((u.params[0] - 50.0).abs() < 1e-5);
        assert!((u.params[2] - 32.0).abs() < 1e-5);
    }
}
