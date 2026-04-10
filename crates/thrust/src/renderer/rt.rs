//! ソフトウェアレイトレーシング (Round 9)
//!
//! Compute shader でシーン (球の集合) に対してレイトレーシングを行う。
//! 影レイ + 1 バウンス反射 + ambient + 太陽光をサポート。
//! BVH なしの brute-force 実装で、~256 球まで実用的。
//!
//! 用途: スクリーン全体の RT、AO、反射の追加サンプル、参考実装

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RtSphere {
    /// xyz = center, w = radius
    pub center_radius: [f32; 4],
    /// rgba = albedo
    pub albedo: [f32; 4],
    /// x = metallic, y = roughness, z = emission, w = _
    pub material: [f32; 4],
}

impl RtSphere {
    pub fn new(center: glam::Vec3, radius: f32, albedo: glam::Vec3) -> Self {
        Self {
            center_radius: [center.x, center.y, center.z, radius],
            albedo: [albedo.x, albedo.y, albedo.z, 1.0],
            material: [0.0, 0.5, 0.0, 0.0],
        }
    }

    pub fn metallic(mut self) -> Self {
        self.material[0] = 1.0;
        self.material[1] = 0.1;
        self
    }

    pub fn emissive(mut self, intensity: f32) -> Self {
        self.material[2] = intensity;
        self
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RtParams {
    /// xyz = sun direction, w = num_spheres
    pub sun_count: [f32; 4],
    /// rgb = sun color, a = sky intensity
    pub sun_sky: [f32; 4],
    /// x = max_bounces, y = max_t, z = enabled, w = _
    pub misc: [f32; 4],
}

impl Default for RtParams {
    fn default() -> Self {
        Self {
            sun_count: [0.5, -1.0, 0.3, 0.0],
            sun_sky: [1.0, 0.95, 0.85, 1.0],
            misc: [1.0, 1000.0, 0.0, 0.0], // disabled by default
        }
    }
}

pub struct SoftwareRayTracer {
    pub params: RtParams,
    pub params_buffer: wgpu::Buffer,
    pub spheres: Vec<RtSphere>,
    pub sphere_buffer: wgpu::Buffer,
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::ComputePipeline,
    pub width: u32,
    pub height: u32,
    pub max_spheres: u32,
}

impl SoftwareRayTracer {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        max_spheres: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RT Output"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let params = RtParams::default();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("RT Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let spheres: Vec<RtSphere> = vec![RtSphere::zeroed(); max_spheres as usize];
        let sphere_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("RT Spheres"),
            contents: bytemuck::cast_slice(&spheres),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RT BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: HDR_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RT BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sphere_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&output_view),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RT Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::RT_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RT PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("RT Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("cs_trace"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            params,
            params_buffer,
            spheres: Vec::new(),
            sphere_buffer,
            output_texture,
            output_view,
            bind_group_layout,
            bind_group,
            pipeline,
            width: w,
            height: h,
            max_spheres,
        }
    }

    pub fn upload_spheres(&mut self, queue: &wgpu::Queue, spheres: &[RtSphere]) {
        self.spheres = spheres.to_vec();
        let count = spheres.len().min(self.max_spheres as usize);
        if count > 0 {
            queue.write_buffer(
                &self.sphere_buffer,
                0,
                bytemuck::cast_slice(&spheres[..count]),
            );
        }
        self.params.sun_count[3] = count as f32;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    pub fn set_sun(&mut self, queue: &wgpu::Queue, dir: glam::Vec3, color: glam::Vec3) {
        self.params.sun_count[0] = dir.x;
        self.params.sun_count[1] = dir.y;
        self.params.sun_count[2] = dir.z;
        self.params.sun_sky[0] = color.x;
        self.params.sun_sky[1] = color.y;
        self.params.sun_sky[2] = color.z;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    pub fn set_enabled(&mut self, queue: &wgpu::Queue, enabled: bool) {
        self.params.misc[2] = if enabled { 1.0 } else { 0.0 };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.params.misc[2] < 0.5 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("RT Compute Pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let groups_x = self.width.div_ceil(8);
        let groups_y = self.height.div_ceil(8);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_sphere_size() {
        // 3 vec4 = 48 B
        assert_eq!(std::mem::size_of::<RtSphere>(), 48);
    }

    #[test]
    fn test_rt_params_size() {
        // 3 vec4 = 48 B
        assert_eq!(std::mem::size_of::<RtParams>(), 48);
    }

    #[test]
    fn test_sphere_metallic_builder() {
        let s = RtSphere::new(glam::Vec3::ZERO, 1.0, glam::Vec3::ONE).metallic();
        assert_eq!(s.material[0], 1.0);
        assert!(s.material[1] < 0.5);
    }

    #[test]
    fn test_sphere_emissive_builder() {
        let s = RtSphere::new(glam::Vec3::ZERO, 1.0, glam::Vec3::ONE).emissive(2.0);
        assert_eq!(s.material[2], 2.0);
    }

    #[test]
    fn test_default_disabled() {
        let p = RtParams::default();
        assert!(p.misc[2] < 0.5);
    }
}
