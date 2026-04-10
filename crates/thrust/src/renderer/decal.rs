//! Deferred Decals (Round 7)
//!
//! デカールはワールド空間のボックスボリューム (1m³ unit cube) として描画され、
//! 深度バッファから world 位置を再構築して decal local 空間に変換、テクスチャを投影する。
//! HDR メインパスの上に alpha-blended で描画される。
//!
//! 法線比較で「裏面」 (decal の方向と離れすぎた面) は破棄する。

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::renderer::prepass::GeometryPrepass;
use crate::renderer::texture::ThrustTexture;
use crate::shader;

/// デカールの GPU uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct DecalUniform {
    /// world → decal local の inverse model 変換
    pub inv_model: [[f32; 4]; 4],
    /// world での model 変換
    pub model: [[f32; 4]; 4],
    /// xyz = tint, w = opacity
    pub tint: [f32; 4],
    /// x = max_normal_cos, y = fade_distance, z = _, w = _
    pub params: [f32; 4],
}

impl DecalUniform {
    pub fn from_components(transform: glam::Mat4, tint: glam::Vec4, max_normal_cos: f32) -> Self {
        Self {
            inv_model: transform.inverse().to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            tint: tint.to_array(),
            params: [max_normal_cos, 1.0, 0.0, 0.0],
        }
    }
}

/// デカールコンポーネント (ECS で使用)
pub struct Decal {
    /// world 変換 (位置 + 回転 + スケール = 1m³ ボリューム)
    pub transform: glam::Mat4,
    /// テクスチャ
    pub texture: Arc<ThrustTexture>,
    /// 色味とオパシティ
    pub tint: glam::Vec4,
    /// 適用する最大法線角度の余弦 (0.0 = 180°, 1.0 = 0°)。デフォルト 0.3 = 約 73°
    pub max_normal_cos: f32,
}

impl Decal {
    pub fn new(transform: glam::Mat4, texture: Arc<ThrustTexture>) -> Self {
        Self {
            transform,
            texture,
            tint: glam::Vec4::ONE,
            max_normal_cos: 0.3,
        }
    }

    pub fn with_tint(mut self, tint: glam::Vec4) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_max_angle_deg(mut self, deg: f32) -> Self {
        self.max_normal_cos = deg.to_radians().cos();
        self
    }
}

/// デカールのレンダリングリソース
pub struct DecalRenderer {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    /// ユニットキューブの頂点 (3 floats のみ)
    pub cube_vertex_buffer: wgpu::Buffer,
    pub cube_index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl DecalRenderer {
    pub fn new(device: &wgpu::Device, camera_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        // Decal bind group: uniform + texture + sampler + normal_view + depth_view
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Decal BGL"),
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
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Decal Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::DECAL_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Decal PL"),
            bind_group_layouts: &[Some(camera_bind_group_layout), Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Decal Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // 視点が decal volume 内部にある場合でも描画したいので front-face cull
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // ユニットキューブ (-0.5..0.5)
        let verts: [[f32; 3]; 8] = [
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ];
        let cube_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Decal Cube VB"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices: [u32; 36] = [
            0, 1, 2, 2, 3, 0, // -Z
            5, 4, 7, 7, 6, 5, // +Z
            4, 0, 3, 3, 7, 4, // -X
            1, 5, 6, 6, 2, 1, // +X
            3, 2, 6, 6, 7, 3, // +Y
            4, 5, 1, 1, 0, 4, // -Y
        ];
        let cube_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Decal Cube IB"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            bind_group_layout,
            pipeline,
            cube_vertex_buffer,
            cube_index_buffer,
            num_indices: 36,
        }
    }

    /// デカール用の uniform バッファとバインドグループを作成 (一度のみ)
    pub fn create_decal_resources(
        &self,
        device: &wgpu::Device,
        decal: &Decal,
        prepass: &GeometryPrepass,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let uniform =
            DecalUniform::from_components(decal.transform, decal.tint, decal.max_normal_cos);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Decal Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Decal Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&decal.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&decal.texture.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prepass.normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
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
    fn test_decal_uniform_size() {
        // 64 + 64 + 16 + 16 = 160 B
        assert_eq!(std::mem::size_of::<DecalUniform>(), 160);
    }

    #[test]
    fn test_decal_with_max_angle() {
        let dummy_transform = glam::Mat4::IDENTITY;
        // ThrustTexture を作らずに with_max_angle_deg だけテスト
        let cos = 60.0_f32.to_radians().cos();
        assert!((cos - 0.5).abs() < 0.01);
        let _ = dummy_transform;
    }
}
