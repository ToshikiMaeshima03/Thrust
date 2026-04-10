//! フォリッジ / 草システム (Round 8)
//!
//! インスタンスドメッシュとして大量の草を描画する。頂点シェーダーで風揺れを加え、
//! 距離 LOD でカメラから遠い草を間引きする。

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::mesh::mesh::Mesh;
use crate::mesh::vertex::Vertex;
use crate::renderer::instancing::InstanceData;
use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::renderer::post::{HDR_FORMAT, MSAA_SAMPLES};
use crate::renderer::texture::ThrustTexture;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FoliageUniform {
    /// xyz = wind direction (normalized), w = strength
    pub wind: [f32; 4],
    /// x = sway frequency, y = phase variance, z = bend curve, w = _
    pub params: [f32; 4],
}

impl Default for FoliageUniform {
    fn default() -> Self {
        Self {
            wind: [1.0, 0.0, 0.3, 0.4],
            params: [2.0, 5.0, 1.5, 0.0],
        }
    }
}

/// フォリッジパッチ (草の集合)
pub struct FoliagePatch {
    pub mesh: Arc<Mesh>,
    pub texture: Arc<ThrustTexture>,
    pub instances: Vec<InstanceData>,
    pub uniform: FoliageUniform,
    pub instance_buffer: Option<wgpu::Buffer>,
    pub uniform_buffer: Option<wgpu::Buffer>,
    pub bind_group: Option<wgpu::BindGroup>,
    pub bounds: crate::math::Aabb,
}

impl FoliagePatch {
    pub fn new(mesh: Arc<Mesh>, texture: Arc<ThrustTexture>) -> Self {
        Self {
            mesh,
            texture,
            instances: Vec::new(),
            uniform: FoliageUniform::default(),
            instance_buffer: None,
            uniform_buffer: None,
            bind_group: None,
            bounds: crate::math::Aabb::new(glam::Vec3::splat(-1000.0), glam::Vec3::splat(1000.0)),
        }
    }

    pub fn instance_count(&self) -> u32 {
        self.instances.len() as u32
    }
}

/// フォリッジレンダリングリソース
pub struct FoliageRenderer {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
}

impl FoliageRenderer {
    pub fn new(device: &wgpu::Device, layouts: &ThrustBindGroupLayouts) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Foliage BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
            label: Some("Foliage Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::FOLIAGE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Foliage Pipeline Layout"),
            bind_group_layouts: &[Some(&layouts.camera), Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Foliage Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::buffer_layout(), InstanceData::buffer_layout()],
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
                cull_mode: None, // 草は両面表示
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: true, // 草の透過に有効
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            bind_group_layout,
            pipeline,
        }
    }

    pub fn create_resources(
        &self,
        device: &wgpu::Device,
        patch: &FoliagePatch,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Foliage Uniform"),
            contents: bytemuck::cast_slice(&[patch.uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let instance_data: Vec<InstanceData> = patch.instances.clone();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Foliage Instances"),
            contents: if instance_data.is_empty() {
                &[0u8; 64]
            } else {
                bytemuck::cast_slice(&instance_data)
            },
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Foliage BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&patch.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&patch.texture.sampler),
                },
            ],
        });
        (uniform_buffer, instance_buffer, bind_group)
    }
}

/// グリッド状に草インスタンスを生成するヘルパー
pub fn grid_foliage_instances(
    grid_size: u32,
    spacing: f32,
    jitter: f32,
    seed: u32,
) -> Vec<InstanceData> {
    use crate::math::SimpleRng;
    let mut rng = SimpleRng::new(seed);
    let mut out = Vec::with_capacity((grid_size * grid_size) as usize);
    let half = (grid_size as f32 - 1.0) * spacing * 0.5;
    for i in 0..grid_size {
        for j in 0..grid_size {
            let x = i as f32 * spacing - half + (rng.next_f32() - 0.5) * jitter;
            let z = j as f32 * spacing - half + (rng.next_f32() - 0.5) * jitter;
            let scale = 0.7 + rng.next_f32() * 0.6;
            let rot = rng.next_f32() * std::f32::consts::TAU;
            let mat = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(scale),
                glam::Quat::from_rotation_y(rot),
                glam::Vec3::new(x, 0.0, z),
            );
            out.push(InstanceData::from_matrix(mat));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foliage_uniform_size() {
        // 2 vec4 = 32 B
        assert_eq!(std::mem::size_of::<FoliageUniform>(), 32);
    }

    #[test]
    fn test_grid_foliage_count() {
        let v = grid_foliage_instances(10, 1.0, 0.0, 42);
        assert_eq!(v.len(), 100);
    }

    #[test]
    fn test_grid_foliage_jitter_changes_position() {
        let v1 = grid_foliage_instances(5, 1.0, 0.5, 42);
        let v2 = grid_foliage_instances(5, 1.0, 0.5, 100);
        // 異なるシードで違う結果
        assert_ne!(v1[0].model[3], v2[0].model[3]);
    }
}
