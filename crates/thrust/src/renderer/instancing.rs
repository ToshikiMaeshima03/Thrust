//! GPU インスタンシング (Round 5)
//!
//! 同一メッシュ + 同一マテリアルを多数のインスタンスでまとめて描画する。
//! foliage、群衆、デブリ等の高速描画用。
//!
//! 使い方:
//! ```ignore
//! let mesh = create_cube(&res.gpu.device, 0.5);
//! let mut instances = Vec::new();
//! for i in 0..100 {
//!     instances.push(Transform::from_translation(glam::Vec3::new(i as f32, 0.0, 0.0)));
//! }
//! let instanced = InstancedMesh::new(&res.gpu.device, mesh, instances, Material::default());
//! world.spawn((instanced,));
//! ```

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::material::material::Material;
use crate::math::Aabb;
use crate::mesh::mesh::Mesh;
use crate::mesh::vertex::Vertex;
use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::renderer::post::{HDR_FORMAT, MSAA_SAMPLES};
use crate::scene::transform::Transform;
use crate::shader;

/// インスタンスデータ (4x4 マトリクス、16 floats = 64 B)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct InstanceData {
    pub model: [[f32; 4]; 4],
}

impl InstanceData {
    pub fn from_transform(t: &Transform) -> Self {
        Self {
            model: t.to_matrix().to_cols_array_2d(),
        }
    }

    pub fn from_matrix(m: glam::Mat4) -> Self {
        Self {
            model: m.to_cols_array_2d(),
        }
    }

    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// インスタンスドメッシュコンポーネント
///
/// `Transform` を持たない (各インスタンスが独自のトランスフォームを持つ)。
/// `material_bind_group` は `render_prep_system` で生成される。
pub struct InstancedMesh {
    pub mesh: Mesh,
    pub instances: Vec<Transform>,
    pub instance_buffer: wgpu::Buffer,
    pub instance_count: u32,
    pub material: Material,
    /// 全インスタンスを覆うワールド AABB (フラスタムカリング用)
    pub bounds: Aabb,
    /// レンダリングステート (`render_prep_system` が生成)
    pub material_bind_group: Option<wgpu::BindGroup>,
    pub material_buffer: Option<wgpu::Buffer>,
}

impl InstancedMesh {
    pub fn new(
        device: &wgpu::Device,
        mesh: Mesh,
        instances: Vec<Transform>,
        material: Material,
    ) -> Self {
        let instance_count = instances.len() as u32;
        let instance_data: Vec<InstanceData> =
            instances.iter().map(InstanceData::from_transform).collect();

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // インスタンス全体の AABB を計算
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        for inst in &instances {
            let world_aabb = mesh.local_aabb.transformed(&inst.to_matrix());
            min = min.min(world_aabb.min);
            max = max.max(world_aabb.max);
        }
        let bounds = if instances.is_empty() {
            Aabb::new(glam::Vec3::ZERO, glam::Vec3::ZERO)
        } else {
            Aabb::new(min, max)
        };

        Self {
            mesh,
            instances,
            instance_buffer,
            instance_count,
            material,
            bounds,
            material_bind_group: None,
            material_buffer: None,
        }
    }

    /// インスタンスデータを更新する (動的な foliage 等)
    pub fn update_instances(&mut self, queue: &wgpu::Queue, instances: Vec<Transform>) {
        self.instances = instances;
        self.instance_count = self.instances.len() as u32;
        let data: Vec<InstanceData> = self
            .instances
            .iter()
            .map(InstanceData::from_transform)
            .collect();
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&data));

        // bounds 更新
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        for inst in &self.instances {
            let world_aabb = self.mesh.local_aabb.transformed(&inst.to_matrix());
            min = min.min(world_aabb.min);
            max = max.max(world_aabb.max);
        }
        self.bounds = if self.instances.is_empty() {
            Aabb::new(glam::Vec3::ZERO, glam::Vec3::ZERO)
        } else {
            Aabb::new(min, max)
        };
    }
}

/// インスタンスドメッシュ用パイプライン
pub fn create_instanced_pipeline(
    device: &wgpu::Device,
    layouts: &ThrustBindGroupLayouts,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Instanced PBR Shader"),
        source: wgpu::ShaderSource::Wgsl(shader::INSTANCED_SHADER.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Instanced Pipeline Layout"),
        // Group 0 = camera, Group 1 (model) は使わない、Group 2 = material
        bind_group_layouts: &[Some(&layouts.camera), None, Some(&layouts.material)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Instanced Render Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::buffer_layout(), InstanceData::buffer_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
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
            cull_mode: Some(wgpu::Face::Back),
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
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_data_size() {
        // mat4x4 = 64 B
        assert_eq!(std::mem::size_of::<InstanceData>(), 64);
    }

    #[test]
    fn test_instance_data_layout() {
        let layout = InstanceData::buffer_layout();
        assert_eq!(layout.array_stride, 64);
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        assert_eq!(layout.attributes.len(), 4);
        assert_eq!(layout.attributes[0].shader_location, 6);
    }

    #[test]
    fn test_instance_from_identity_transform() {
        let t = Transform::default();
        let data = InstanceData::from_transform(&t);
        // identity マトリクス
        assert!((data.model[0][0] - 1.0).abs() < 1e-5);
        assert!((data.model[1][1] - 1.0).abs() < 1e-5);
        assert!((data.model[2][2] - 1.0).abs() < 1e-5);
        assert!((data.model[3][3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_instance_from_translation() {
        let t = Transform::from_translation(glam::Vec3::new(5.0, 0.0, 0.0));
        let data = InstanceData::from_transform(&t);
        // 並進は w_axis に入る (col-major)
        assert!((data.model[3][0] - 5.0).abs() < 1e-5);
    }
}
