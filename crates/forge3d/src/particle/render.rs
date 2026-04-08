use bytemuck::{Pod, Zeroable};
use hecs::World;
use wgpu::util::DeviceExt;

use crate::ecs::resources::Resources;
use crate::particle::emitter::ParticleEmitter;

/// パーティクルインスタンスデータ（GPU 転送用）
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ParticleInstance {
    pub position: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

impl ParticleInstance {
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ParticleInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // size
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
                // color
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// パーティクル描画用レンダリングステート（Resources に格納）
pub struct ParticleRenderState {
    pub pipeline: wgpu::RenderPipeline,
    pub instance_buffer: wgpu::Buffer,
    pub instance_count: u32,
    pub instance_buffer_capacity: usize,
}

/// パーティクル用レンダリングパイプラインを作成する
pub fn create_particle_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Particle Shader"),
        source: wgpu::ShaderSource::Wgsl(crate::shader::PARTICLE_SHADER.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Particle Pipeline Layout"),
        bind_group_layouts: &[Some(camera_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Particle Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[ParticleInstance::buffer_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
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
            cull_mode: None, // 両面描画
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false), // 深度テストのみ、書き込みなし
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

/// パーティクル描画準備: 全エミッターからインスタンスデータを収集しバッファに書き込む
pub fn particle_render_prep_system(world: &World, res: &mut Resources) {
    let mut instances: Vec<ParticleInstance> = Vec::new();

    for emitter in world.query::<&ParticleEmitter>().iter() {
        for particle in &emitter.particles {
            if particle.is_alive() {
                instances.push(ParticleInstance {
                    position: particle.position.to_array(),
                    size: particle.size,
                    color: particle.color.to_array(),
                });
            }
        }
    }

    if let Some(prs) = &mut res.particle_render_state {
        prs.instance_count = instances.len() as u32;

        if instances.is_empty() {
            return;
        }

        // バッファ再作成が必要な場合（容量不足）
        if instances.len() > prs.instance_buffer_capacity {
            let new_capacity = instances.len().next_power_of_two();
            prs.instance_buffer =
                res.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Particle Instance Buffer"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            prs.instance_buffer_capacity = new_capacity;
        } else {
            res.gpu
                .queue
                .write_buffer(&prs.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }
    }
}

/// パーティクル描画: 不透明オブジェクトの後に render pass 内で呼び出す
pub fn particle_render_system(render_pass: &mut wgpu::RenderPass<'_>, res: &Resources) {
    if let Some(prs) = &res.particle_render_state {
        if prs.instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&prs.pipeline);
        render_pass.set_bind_group(0, &res.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, prs.instance_buffer.slice(..));
        // 6 頂点 (2 三角形のクアッド) x instance_count
        render_pass.draw(0..6, 0..prs.instance_count);
    }
}
