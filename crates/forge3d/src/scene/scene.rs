use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use super::transform::Transform;
use crate::material::material::{Material, MaterialUniform};
use crate::mesh::mesh::Mesh;
use crate::renderer::pipeline::ForgeBindGroupLayouts;
use crate::renderer::texture::ForgeTexture;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(u64);

pub struct SceneObject {
    pub id: EntityId,
    pub mesh: Mesh,
    pub transform: Transform,
    pub material: Material,
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
    pub material_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
}

pub struct Scene {
    objects: HashMap<EntityId, SceneObject>,
    render_order: Vec<EntityId>,
    next_id: u64,
    fallback_texture: Arc<ForgeTexture>,
}

impl Scene {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            objects: HashMap::new(),
            render_order: Vec::new(),
            next_id: 0,
            fallback_texture: Arc::new(ForgeTexture::white_pixel(device, queue)),
        }
    }

    pub fn add_object(
        &mut self,
        mesh: Mesh,
        transform: Transform,
        material: Material,
        device: &wgpu::Device,
        layouts: &ForgeBindGroupLayouts,
    ) -> EntityId {
        use wgpu::util::DeviceExt;

        let id = EntityId(self.next_id);
        self.next_id += 1;

        // モデルユニフォーム
        let model_uniform = ModelUniform::from_transform(&transform);
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Uniform Buffer"),
            contents: bytemuck::cast_slice(&[model_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout: &layouts.model,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: model_buffer.as_entire_binding(),
            }],
        });

        // マテリアルユニフォーム + テクスチャ
        let material_uniform = MaterialUniform::from_material(&material);
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Uniform Buffer"),
            contents: bytemuck::cast_slice(&[material_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let tex = material.texture.as_ref().unwrap_or(&self.fallback_texture);

        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &layouts.material,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&tex.sampler),
                },
            ],
        });

        self.render_order.push(id);
        self.objects.insert(
            id,
            SceneObject {
                id,
                mesh,
                transform,
                material,
                model_buffer,
                model_bind_group,
                material_buffer,
                material_bind_group,
            },
        );

        id
    }

    pub fn remove_object(&mut self, id: EntityId) -> bool {
        if self.objects.remove(&id).is_some() {
            self.render_order.retain(|oid| *oid != id);
            true
        } else {
            false
        }
    }

    pub fn get(&self, id: EntityId) -> Option<&SceneObject> {
        self.objects.get(&id)
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut SceneObject> {
        self.objects.get_mut(&id)
    }

    pub fn set_material(&mut self, id: EntityId, material: Material, queue: &wgpu::Queue) {
        if let Some(obj) = self.objects.get_mut(&id) {
            let uniform = MaterialUniform::from_material(&material);
            queue.write_buffer(&obj.material_buffer, 0, bytemuck::cast_slice(&[uniform]));
            obj.material = material;
        }
    }

    pub fn objects_iter(&self) -> impl Iterator<Item = &SceneObject> {
        self.render_order
            .iter()
            .filter_map(|id| self.objects.get(id))
    }

    pub fn update_transforms(&self, queue: &wgpu::Queue) {
        for obj in self.objects.values() {
            let uniform = ModelUniform::from_transform(&obj.transform);
            queue.write_buffer(&obj.model_buffer, 0, bytemuck::cast_slice(&[uniform]));
        }
    }
}
