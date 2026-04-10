//! GPU 駆動カリング (Round 9)
//!
//! Compute shader を使ってインスタンスの可視性をフラスタム判定し、
//! 可視のものだけ DrawIndirect 引数に書き込む。Nanite 風のクラスタカリングの
//! 基礎部分を実装。
//!
//! 用途: 巨大なフォリッジ/瓦礫/群衆の高速描画

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::shader;

/// インスタンスの bounding 情報 + draw 引数
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct InstanceBound {
    /// xyz = center, w = radius
    pub center_radius: [f32; 4],
    /// xyz = aabb min, w = _
    pub aabb_min: [f32; 4],
    /// xyz = aabb max, w = _
    pub aabb_max: [f32; 4],
    /// x = mesh_id, y = first_index, z = index_count, w = base_vertex
    pub draw_info: [u32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct DrawIndirectArgs {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CullingParams {
    /// 6 frustum planes (Hessian 形式)
    pub planes: [[f32; 4]; 6],
    /// x = total_instances, yzw = _
    pub counts: [u32; 4],
    /// xyz = camera_position, w = _
    pub camera: [f32; 4],
}

impl Default for CullingParams {
    fn default() -> Self {
        Self {
            planes: [[0.0; 4]; 6],
            counts: [0; 4],
            camera: [0.0; 4],
        }
    }
}

/// GPU カリングシステム
pub struct GpuCulling {
    pub instance_buffer: wgpu::Buffer,
    pub draw_args_buffer: wgpu::Buffer,
    pub visible_count_buffer: wgpu::Buffer,
    pub params_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::ComputePipeline,
    pub max_instances: u32,
}

impl GpuCulling {
    pub fn new(device: &wgpu::Device, max_instances: u32) -> Self {
        let instances: Vec<InstanceBound> = vec![InstanceBound::zeroed(); max_instances as usize];
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Culling Instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let draw_args: Vec<DrawIndirectArgs> = vec![
            DrawIndirectArgs {
                index_count: 0,
                instance_count: 0,
                first_index: 0,
                base_vertex: 0,
                first_instance: 0,
            };
            max_instances as usize
        ];
        let draw_args_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Culling Draw Args"),
            contents: bytemuck::cast_slice(&draw_args),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
        });

        let visible_count_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Culling Visible Count"),
            contents: bytemuck::cast_slice(&[0u32]),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });

        let params = CullingParams::default();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Culling Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU Culling BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Culling BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: draw_args_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: visible_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Culling Compute"),
            source: wgpu::ShaderSource::Wgsl(shader::GPU_CULLING_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Culling PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GPU Culling Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("cs_cull"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            instance_buffer,
            draw_args_buffer,
            visible_count_buffer,
            params_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            max_instances,
        }
    }

    pub fn upload_instances(&self, queue: &wgpu::Queue, instances: &[InstanceBound]) {
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    pub fn upload_params(&self, queue: &wgpu::Queue, params: &CullingParams) {
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[*params]));
    }

    pub fn reset_visible_count(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.visible_count_buffer, 0, bytemuck::cast_slice(&[0u32]));
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder, count: u32) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GPU Culling Pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let groups = count.div_ceil(64);
        pass.dispatch_workgroups(groups, 1, 1);
    }

    /// view-projection 行列から 6 個のフラスタム平面を抽出する
    pub fn extract_planes(view_proj: glam::Mat4) -> [[f32; 4]; 6] {
        let m = view_proj.to_cols_array_2d();
        // Gribb-Hartmann method (column-major glam Mat4)
        // Row vectors:
        let row = |i: usize| [m[0][i], m[1][i], m[2][i], m[3][i]];
        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let add_row =
            |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub_row =
            |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
        let normalize = |p: [f32; 4]| {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-5);
            [p[0] / len, p[1] / len, p[2] / len, p[3] / len]
        };

        [
            normalize(add_row(r3, r0)), // left
            normalize(sub_row(r3, r0)), // right
            normalize(add_row(r3, r1)), // bottom
            normalize(sub_row(r3, r1)), // top
            normalize(add_row(r3, r2)), // near
            normalize(sub_row(r3, r2)), // far
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_bound_size() {
        // 4 vec4 = 64 B
        assert_eq!(std::mem::size_of::<InstanceBound>(), 64);
    }

    #[test]
    fn test_draw_args_size() {
        // 5 u32 = 20 B (note: not vec4 aligned, but indirect args don't need it)
        assert_eq!(std::mem::size_of::<DrawIndirectArgs>(), 20);
    }

    #[test]
    fn test_culling_params_size() {
        // 6 vec4 + 1 vec4 (counts) + 1 vec4 (camera) = 8 vec4 = 128 B
        assert_eq!(std::mem::size_of::<CullingParams>(), 128);
    }

    #[test]
    fn test_extract_planes_count() {
        let vp = glam::Mat4::IDENTITY;
        let planes = GpuCulling::extract_planes(vp);
        assert_eq!(planes.len(), 6);
    }

    #[test]
    fn test_extract_planes_normalized() {
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        let proj = glam::Mat4::perspective_rh(45_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        let planes = GpuCulling::extract_planes(proj * view);
        for plane in &planes {
            let len = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
            assert!((len - 1.0).abs() < 0.01, "正規化されていない: {len}");
        }
    }
}
