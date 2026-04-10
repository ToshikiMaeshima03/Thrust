//! GPU パーティクルシステム (Round 8)
//!
//! Compute shader でパーティクルを更新する。
//! 数千〜数万パーティクルを CPU 介さずに動かせる。
//! 既存の CPU 系 ParticleEmitter と並行して使える。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuParticle {
    /// xyz = position, w = lifetime
    pub pos_life: [f32; 4],
    /// xyz = velocity, w = age
    pub vel_age: [f32; 4],
    /// rgba
    pub color: [f32; 4],
    /// x = size, y = active, z = _, w = _
    pub misc: [f32; 4],
}

impl Default for GpuParticle {
    fn default() -> Self {
        Self {
            pos_life: [0.0, 0.0, 0.0, 0.0],
            vel_age: [0.0; 4],
            color: [1.0, 1.0, 1.0, 0.0],
            misc: [0.1, 0.0, 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuParticleSimParams {
    /// xyz = gravity, w = dt
    pub gravity_dt: [f32; 4],
    /// xyz = wind, w = drag
    pub wind_drag: [f32; 4],
    /// x = num_particles, y = seed, z = emit_per_step, w = _
    pub counts: [f32; 4],
}

impl Default for GpuParticleSimParams {
    fn default() -> Self {
        Self {
            gravity_dt: [0.0, -9.81, 0.0, 0.016],
            wind_drag: [0.0, 0.0, 0.0, 0.5],
            counts: [1024.0, 0.0, 16.0, 0.0],
        }
    }
}

pub struct GpuParticleSystem {
    pub particles: Vec<GpuParticle>,
    pub particle_buffer: wgpu::Buffer,
    pub params: GpuParticleSimParams,
    pub params_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::ComputePipeline,
    pub num_particles: u32,
}

impl GpuParticleSystem {
    pub fn new(device: &wgpu::Device, num_particles: u32) -> Self {
        let particles = vec![GpuParticle::default(); num_particles as usize];
        let particle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Particles Buffer"),
            contents: bytemuck::cast_slice(&particles),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::VERTEX,
        });

        let mut params = GpuParticleSimParams::default();
        params.counts[0] = num_particles as f32;
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU Particles Sim Params"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU Particles BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
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
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Particles BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Particle Compute"),
            source: wgpu::ShaderSource::Wgsl(shader::GPU_PARTICLE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Particle PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GPU Particle Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("cs_update"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            particles,
            particle_buffer,
            params,
            params_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            num_particles,
        }
    }

    /// シミュレーションパラメータを更新
    pub fn set_dt(&mut self, queue: &wgpu::Queue, dt: f32, frame: u32) {
        self.params.gravity_dt[3] = dt;
        self.params.counts[1] = frame as f32;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    pub fn set_gravity(&mut self, queue: &wgpu::Queue, gravity: glam::Vec3) {
        self.params.gravity_dt[0] = gravity.x;
        self.params.gravity_dt[1] = gravity.y;
        self.params.gravity_dt[2] = gravity.z;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    pub fn set_wind(&mut self, queue: &wgpu::Queue, wind: glam::Vec3, drag: f32) {
        self.params.wind_drag = [wind.x, wind.y, wind.z, drag];
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    /// dispatch をエンコードする (workgroup_size=64)
    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GPU Particle Compute Pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let groups = self.num_particles.div_ceil(64);
        pass.dispatch_workgroups(groups, 1, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_particle_size() {
        // 4 vec4 = 64 B
        assert_eq!(std::mem::size_of::<GpuParticle>(), 64);
    }

    #[test]
    fn test_sim_params_size() {
        assert_eq!(std::mem::size_of::<GpuParticleSimParams>(), 48);
    }

    #[test]
    fn test_default_particle_inactive() {
        let p = GpuParticle::default();
        assert_eq!(p.misc[1], 0.0); // inactive
    }
}
