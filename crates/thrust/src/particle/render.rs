use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use hecs::World;
use wgpu::util::DeviceExt;

use crate::ecs::resources::Resources;
use crate::particle::emitter::ParticleEmitter;
use crate::renderer::texture::ThrustTexture;

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

/// パーティクル描画バッチ（同一テクスチャのパーティクルをまとめる）
pub struct ParticleBatch {
    pub instance_offset: u32,
    pub instance_count: u32,
    pub texture_bind_group: Option<wgpu::BindGroup>,
}

/// パーティクル描画用レンダリングステート（Resources に格納）
pub struct ParticleRenderState {
    pub pipeline_untextured: wgpu::RenderPipeline,
    pub pipeline_textured: wgpu::RenderPipeline,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub instance_buffer: wgpu::Buffer,
    pub instance_buffer_capacity: usize,
    pub batches: Vec<ParticleBatch>,
    /// フレーム毎のヒープ割り当てを避けるための再利用バッファ
    pub(crate) cached_instances: Vec<ParticleInstance>,
}

/// パーティクルテクスチャ用 Bind Group レイアウトを作成する
pub fn create_particle_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Particle Texture Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// テクスチャなしパーティクル用レンダリングパイプラインを作成する
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

    create_particle_render_pipeline(device, surface_format, &pipeline_layout, &shader)
}

/// テクスチャ付きパーティクル用レンダリングパイプラインを作成する
pub fn create_particle_textured_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Particle Textured Shader"),
        source: wgpu::ShaderSource::Wgsl(crate::shader::PARTICLE_TEXTURED_SHADER.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Particle Textured Pipeline Layout"),
        bind_group_layouts: &[Some(camera_layout), Some(texture_layout)],
        immediate_size: 0,
    });

    create_particle_render_pipeline(device, surface_format, &pipeline_layout, &shader)
}

/// パーティクル描画パイプラインの共通設定 (Round 4: HDR + MSAA)
fn create_particle_render_pipeline(
    device: &wgpu::Device,
    _surface_format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    use crate::renderer::post::{HDR_FORMAT, MSAA_SAMPLES};
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Particle Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[ParticleInstance::buffer_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
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
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

/// パーティクル描画準備: 全エミッターからインスタンスデータを収集しバッファに書き込む
pub fn particle_render_prep_system(world: &World, res: &mut Resources) {
    let mut untextured_instances: Vec<ParticleInstance> = Vec::new();
    let mut textured_groups: HashMap<
        *const ThrustTexture,
        (Arc<ThrustTexture>, Vec<ParticleInstance>),
    > = HashMap::new();

    for emitter in world.query::<&ParticleEmitter>().iter() {
        for particle in &emitter.particles {
            if !particle.is_alive() {
                continue;
            }
            let inst = ParticleInstance {
                position: particle.position.to_array(),
                size: particle.size,
                color: particle.color.to_array(),
            };
            match &emitter.texture {
                None => untextured_instances.push(inst),
                Some(tex) => {
                    let key = Arc::as_ptr(tex);
                    textured_groups
                        .entry(key)
                        .or_insert_with(|| (tex.clone(), Vec::new()))
                        .1
                        .push(inst);
                }
            }
        }
    }

    let Some(prs) = &mut res.particle_render_state else {
        return;
    };

    // バッチ構築（cached_instances を再利用してヒープ割り当てを削減）
    prs.batches.clear();
    prs.cached_instances.clear();

    // テクスチャなしバッチ
    if !untextured_instances.is_empty() {
        let offset = prs.cached_instances.len() as u32;
        let count = untextured_instances.len() as u32;
        prs.cached_instances
            .extend_from_slice(&untextured_instances);
        prs.batches.push(ParticleBatch {
            instance_offset: offset,
            instance_count: count,
            texture_bind_group: None,
        });
    }

    // テクスチャ付きバッチ
    for (tex, instances) in textured_groups.values() {
        if instances.is_empty() {
            continue;
        }
        let offset = prs.cached_instances.len() as u32;
        let count = instances.len() as u32;
        prs.cached_instances.extend_from_slice(instances);

        let bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Particle Texture Bind Group"),
                layout: &prs.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&tex.sampler),
                    },
                ],
            });

        prs.batches.push(ParticleBatch {
            instance_offset: offset,
            instance_count: count,
            texture_bind_group: Some(bind_group),
        });
    }

    if prs.cached_instances.is_empty() {
        return;
    }

    // バッファ再作成が必要な場合（容量不足）
    if prs.cached_instances.len() > prs.instance_buffer_capacity {
        let new_capacity = prs.cached_instances.len().next_power_of_two();
        prs.instance_buffer =
            res.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Particle Instance Buffer"),
                    contents: bytemuck::cast_slice(&prs.cached_instances),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
        prs.instance_buffer_capacity = new_capacity;
    } else {
        res.gpu.queue.write_buffer(
            &prs.instance_buffer,
            0,
            bytemuck::cast_slice(&prs.cached_instances),
        );
    }
}

/// パーティクル描画: 不透明オブジェクトの後に render pass 内で呼び出す
pub fn particle_render_system(render_pass: &mut wgpu::RenderPass<'_>, res: &Resources) {
    let Some(prs) = &res.particle_render_state else {
        return;
    };

    for batch in &prs.batches {
        if batch.instance_count == 0 {
            continue;
        }

        if let Some(tex_bg) = &batch.texture_bind_group {
            render_pass.set_pipeline(&prs.pipeline_textured);
            render_pass.set_bind_group(0, &res.camera_bind_group, &[]);
            render_pass.set_bind_group(1, tex_bg, &[]);
        } else {
            render_pass.set_pipeline(&prs.pipeline_untextured);
            render_pass.set_bind_group(0, &res.camera_bind_group, &[]);
        }

        render_pass.set_vertex_buffer(0, prs.instance_buffer.slice(..));
        render_pass.draw(
            0..6,
            batch.instance_offset..batch.instance_offset + batch.instance_count,
        );
    }
}
