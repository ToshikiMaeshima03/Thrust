use bytemuck::{Pod, Zeroable};

use super::transform::Transform;
use crate::mesh::mesh::Mesh;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ModelUniform {
    pub model: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
}

impl ModelUniform {
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            model: transform.to_matrix().to_cols_array_2d(),
            normal_matrix: transform.normal_matrix().to_cols_array_2d(),
        }
    }
}

pub struct SceneObject {
    pub mesh: Mesh,
    pub transform: Transform,
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
}

pub struct Scene {
    pub objects: Vec<SceneObject>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    pub fn add_object(
        &mut self,
        mesh: Mesh,
        transform: Transform,
        device: &wgpu::Device,
        model_bind_group_layout: &wgpu::BindGroupLayout,
    ) {
        use wgpu::util::DeviceExt;

        let uniform = ModelUniform::from_transform(&transform);
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout: model_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: model_buffer.as_entire_binding(),
            }],
        });

        self.objects.push(SceneObject {
            mesh,
            transform,
            model_buffer,
            model_bind_group,
        });
    }

    pub fn update_transforms(&self, queue: &wgpu::Queue) {
        for obj in &self.objects {
            let uniform = ModelUniform::from_transform(&obj.transform);
            queue.write_buffer(&obj.model_buffer, 0, bytemuck::cast_slice(&[uniform]));
        }
    }
}
