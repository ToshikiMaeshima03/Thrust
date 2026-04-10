//! Temporal Anti-Aliasing (Round 8)
//!
//! Motion vector + history buffer + neighborhood clamp。
//! 前フレームの結果を motion vector で再投影し、現フレームと α=0.9 でブレンドする。
//!
//! ピンポンテクスチャ history_a / history_b を持ち、毎フレーム交互に書き込む。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::renderer::prepass::GeometryPrepass;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TaaUniform {
    /// x = blend_factor (0..1, history wt), y = clamp_strength, z = enabled, w = _
    pub params: [f32; 4],
}

impl Default for TaaUniform {
    fn default() -> Self {
        Self {
            params: [0.9, 1.0, 1.0, 0.0],
        }
    }
}

pub struct Taa {
    pub uniform: TaaUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub history_a: wgpu::Texture,
    pub history_a_view: wgpu::TextureView,
    pub history_b: wgpu::Texture,
    pub history_b_view: wgpu::TextureView,
    /// 現在の write target index (0 = a, 1 = b)
    pub current_index: u32,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl Taa {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let _ = surface_format;
        let w = width.max(1);
        let h = height.max(1);

        let make_history = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        let history_a = make_history("TAA History A");
        let history_a_view = history_a.create_view(&wgpu::TextureViewDescriptor::default());
        let history_b = make_history("TAA History B");
        let history_b_view = history_b.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = TaaUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("TAA Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("TAA BGL"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("TAA Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::TAA_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("TAA PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("TAA Pipeline"),
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
            history_a,
            history_a_view,
            history_b,
            history_b_view,
            current_index: 0,
            bind_group_layout,
            pipeline,
            width: w,
            height: h,
        }
    }

    /// 今フレームで write 先に使う view を返す
    pub fn current_target_view(&self) -> &wgpu::TextureView {
        if self.current_index == 0 {
            &self.history_a_view
        } else {
            &self.history_b_view
        }
    }

    pub fn prev_history_view(&self) -> &wgpu::TextureView {
        if self.current_index == 0 {
            &self.history_b_view
        } else {
            &self.history_a_view
        }
    }

    pub fn swap(&mut self) {
        self.current_index = 1 - self.current_index;
    }

    pub fn set_enabled(&mut self, queue: &wgpu::Queue, enabled: bool) {
        self.uniform.params[2] = if enabled { 1.0 } else { 0.0 };
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        current_view: &wgpu::TextureView,
        prepass: &GeometryPrepass,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TAA BG"),
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
                    resource: wgpu::BindingResource::TextureView(current_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(self.prev_history_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&prepass.motion_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taa_uniform_size() {
        assert_eq!(std::mem::size_of::<TaaUniform>(), 16);
    }

    #[test]
    fn test_taa_default_blend_high() {
        let u = TaaUniform::default();
        assert!(u.params[0] > 0.5); // history weight should be high
    }
}
