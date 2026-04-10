//! ボリュメトリッククラウド (Round 8)
//!
//! 簡易 ray-march + fbm ノイズ + Henyey-Greenstein 散乱で雲を描画する。
//! プリパスの depth を考慮して地形遮蔽もサポート。HDR に半透明加算合成する。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::post::HDR_FORMAT;
use crate::renderer::prepass::GeometryPrepass;
use crate::shader;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CloudUniform {
    pub sun_dir: [f32; 4],
    pub sun_color: [f32; 4],
    /// x = base_height, y = top_height, z = density, w = coverage
    pub params: [f32; 4],
    /// x = scale, y = wind_speed, z = scattering_g, w = step_size
    pub params2: [f32; 4],
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            sun_dir: [0.5, -1.0, 0.3, 1.0],
            sun_color: [1.0, 0.95, 0.85, 4.0],
            params: [50.0, 200.0, 1.5, 0.55],
            params2: [0.01, 5.0, 0.7, 8.0],
        }
    }
}

pub struct VolumetricClouds {
    pub uniform: CloudUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl VolumetricClouds {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Volumetric Clouds Output"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = CloudUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cloud BGL"),
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
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud BG"),
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
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&prepass.sampler),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::CLOUDS_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cloud PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Pipeline"),
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
            texture,
            view,
            bind_group_layout,
            bind_group,
            pipeline,
            width: w,
            height: h,
        }
    }

    pub fn set_sun(
        &mut self,
        queue: &wgpu::Queue,
        dir: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        enabled: bool,
    ) {
        self.uniform.sun_dir = [dir.x, dir.y, dir.z, if enabled { 1.0 } else { 0.0 }];
        self.uniform.sun_color = [color.x, color.y, color.z, intensity];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn set_params(
        &mut self,
        queue: &wgpu::Queue,
        base_height: f32,
        top_height: f32,
        density: f32,
        coverage: f32,
    ) {
        self.uniform.params = [base_height, top_height, density, coverage];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn rebuild_bind_groups(
        &mut self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        prepass: &GeometryPrepass,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud BG"),
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
                    resource: wgpu::BindingResource::TextureView(&prepass.depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
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
    fn test_cloud_uniform_size() {
        // 4 vec4 = 64 B
        assert_eq!(std::mem::size_of::<CloudUniform>(), 64);
        assert_eq!(std::mem::size_of::<CloudUniform>() % 16, 0);
    }

    #[test]
    fn test_cloud_default() {
        let u = CloudUniform::default();
        assert!(u.params[0] < u.params[1]); // base < top
        assert!(u.params[3] > 0.0); // coverage
    }
}
