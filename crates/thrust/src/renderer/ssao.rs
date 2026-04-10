//! Screen-Space Ambient Occlusion (Round 7)
//!
//! Geometry prepass の depth + normal を入力として、view 空間で半球サンプリングし
//! 16 サンプル + ハッシュ回転で AO を計算する。
//! 4×4 ボックスブラーでノイズ除去後、PBR のアンビエント項に乗算される。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::prepass::GeometryPrepass;
use crate::shader;

/// SSAO パラメータ
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SsaoUniform {
    /// x = radius, y = bias, z = intensity, w = _
    pub params: [f32; 4],
    /// x = noise_scale, y = max_distance, z = _, w = _
    pub extra: [f32; 4],
}

impl Default for SsaoUniform {
    fn default() -> Self {
        Self {
            params: [0.5, 0.025, 1.0, 0.0],
            extra: [4.0, 50.0, 0.0, 0.0],
        }
    }
}

/// SSAO リソース一式
pub struct Ssao {
    pub uniform: SsaoUniform,
    pub uniform_buffer: wgpu::Buffer,
    /// SSAO 出力テクスチャ (R8Unorm、occlusion factor)
    pub ao_texture: wgpu::Texture,
    pub ao_view: wgpu::TextureView,
    /// ブラー後の AO
    pub ao_blurred_texture: wgpu::Texture,
    pub ao_blurred_view: wgpu::TextureView,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub blur_layout: wgpu::BindGroupLayout,
    pub blur_bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub blur_pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl Ssao {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        // SSAO AO テクスチャ
        let ao_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSAO AO Texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ao_view = ao_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let ao_blurred_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSAO AO Blurred"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ao_blurred_view =
            ao_blurred_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = SsaoUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSAO Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // SSAO bind group layout: camera + ssao_params + normal (Rgba16F) + depth (Depth32F) + sampler
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO BGL"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Bind Group"),
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
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });

        // ブラーパス bind group layout
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Blur BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Blur Bind Group"),
            layout: &blur_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });

        // パイプライン
        let ssao_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSAO Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SSAO_SHADER.into()),
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSAO Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SSAO_BLUR_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Blur Pipeline Layout"),
            bind_group_layouts: &[Some(&blur_layout)],
            immediate_size: 0,
        });

        let make_fullscreen =
            |label: &str, shader_module: &wgpu::ShaderModule, pl: &wgpu::PipelineLayout| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(pl),
                    vertex: wgpu::VertexState {
                        module: shader_module,
                        entry_point: Some("vs_fullscreen"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader_module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R8Unorm,
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
                })
            };

        let pipeline = make_fullscreen("SSAO Pipeline", &ssao_shader, &pipeline_layout);
        let blur_pipeline =
            make_fullscreen("SSAO Blur Pipeline", &blur_shader, &blur_pipeline_layout);

        Self {
            uniform,
            uniform_buffer,
            ao_texture,
            ao_view,
            ao_blurred_texture,
            ao_blurred_view,
            bind_group_layout,
            bind_group,
            blur_layout,
            blur_bind_group,
            pipeline,
            blur_pipeline,
            width: w,
            height: h,
        }
    }

    /// パラメータを更新して GPU に書き込む
    pub fn set_params(&mut self, queue: &wgpu::Queue, radius: f32, bias: f32, intensity: f32) {
        self.uniform.params = [radius, bias, intensity, 0.0];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    /// プリパスのリソースに合わせて bind group を再構築する (リサイズ時)
    pub fn rebuild_bind_groups(
        &mut self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Bind Group"),
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
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });
        self.blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Blur Bind Group"),
            layout: &self.blur_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
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
    fn test_ssao_uniform_size() {
        // 16 + 16 = 32 B
        assert_eq!(std::mem::size_of::<SsaoUniform>(), 32);
        assert_eq!(std::mem::size_of::<SsaoUniform>() % 16, 0);
    }

    #[test]
    fn test_ssao_uniform_default() {
        let u = SsaoUniform::default();
        assert!((u.params[0] - 0.5).abs() < 1e-5);
        assert!((u.params[2] - 1.0).abs() < 1e-5);
    }
}
