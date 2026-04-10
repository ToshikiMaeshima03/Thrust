//! ポストプロセスパイプライン (Round 4)
//!
//! - HDR Rgba16Float メインターゲット (MSAA 4× resolve)
//! - Bloom: threshold + 5 段 downsample + upsample
//! - Tonemap: ACES Filmic + sRGB ガンマ + bloom 加算
//! - FXAA: 3.11 簡易移植

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::shader;

/// MSAA サンプル数
pub const MSAA_SAMPLES: u32 = 4;
/// HDR フォーマット
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Bloom チェーンの段数 (mip)
pub const BLOOM_MIPS: u32 = 5;

/// HDR レンダーターゲット (MSAA + 解決後 + LDR 中間バッファ)
pub struct HdrTargets {
    pub width: u32,
    pub height: u32,
    /// MSAA 4× の HDR カラーターゲット
    pub color_msaa: wgpu::TextureView,
    /// MSAA 解決先 (1× HDR)
    pub color_resolved: wgpu::Texture,
    pub color_resolved_view: wgpu::TextureView,
    /// MSAA 4× の深度バッファ
    pub depth_msaa: wgpu::TextureView,
    /// LDR 中間バッファ (tonemap → FXAA の入力)
    pub ldr_intermediate: wgpu::Texture,
    pub ldr_intermediate_view: wgpu::TextureView,
    /// LDR 中間バッファのフォーマット (surface_format と一致)
    pub ldr_format: wgpu::TextureFormat,
}

impl HdrTargets {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };

        let color_msaa_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Color MSAA"),
            size,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_msaa = color_msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let color_resolved = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Color Resolved"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_resolved_view =
            color_resolved.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_msaa_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Depth MSAA"),
            size,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_msaa = depth_msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let ldr_intermediate = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("LDR Intermediate"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ldr_intermediate_view =
            ldr_intermediate.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            width: w,
            height: h,
            color_msaa,
            color_resolved,
            color_resolved_view,
            depth_msaa,
            ldr_intermediate,
            ldr_intermediate_view,
            ldr_format: surface_format,
        }
    }
}

/// Bloom チェーン (downsample mips + 1 段の upsample 結果)
pub struct BloomChain {
    pub mips: Vec<BloomMip>,
    /// 最終 upsample 結果 (フル解像度の半分等)
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
}

pub struct BloomMip {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl BloomChain {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mut mips = Vec::with_capacity(BLOOM_MIPS as usize);
        let mut w = (width / 2).max(1);
        let mut h = (height / 2).max(1);
        for i in 0..BLOOM_MIPS {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Bloom Mip {i}")),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            mips.push(BloomMip {
                texture: tex,
                view,
                width: w,
                height: h,
            });
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }

        // 最終 output (フル解像度の半分)
        let output_w = (width / 2).max(1);
        let output_h = (height / 2).max(1);
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Bloom Output"),
            size: wgpu::Extent3d {
                width: output_w,
                height: output_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            mips,
            output_texture,
            output_view,
        }
    }
}

/// Bloom uniform (threshold, soft_knee, filter_radius)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BloomUniform {
    pub params: [f32; 4],
}

impl Default for BloomUniform {
    fn default() -> Self {
        Self {
            params: [1.0, 0.5, 1.0, 0.0],
        }
    }
}

/// Tonemap uniform (exposure, bloom_strength, enable_bloom, _)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PostUniform {
    pub params: [f32; 4],
}

impl Default for PostUniform {
    fn default() -> Self {
        Self {
            params: [1.0, 0.04, 1.0, 0.0],
        }
    }
}

/// ポストプロセスパイプライン全体
pub struct PostProcessPipelines {
    /// Bloom 用バインドグループレイアウト
    pub bloom_layout: wgpu::BindGroupLayout,
    pub bloom_threshold: wgpu::RenderPipeline,
    pub bloom_downsample: wgpu::RenderPipeline,
    pub bloom_upsample: wgpu::RenderPipeline,
    pub bloom_uniform_buffer: wgpu::Buffer,
    pub bloom_sampler: wgpu::Sampler,

    /// Tonemap 用
    pub tonemap_layout: wgpu::BindGroupLayout,
    pub tonemap: wgpu::RenderPipeline,
    pub post_uniform_buffer: wgpu::Buffer,

    /// FXAA 用
    pub fxaa_layout: wgpu::BindGroupLayout,
    pub fxaa: wgpu::RenderPipeline,
}

impl PostProcessPipelines {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        // Bloom レイアウト: input texture + sampler + bloom uniform
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom BGL"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::BLOOM_SHADER.into()),
        });

        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom PL"),
            bind_group_layouts: &[Some(&bloom_layout)],
            immediate_size: 0,
        });

        let make_fullscreen_pipeline =
            |label: &str,
             shader_module: &wgpu::ShaderModule,
             fs: &str,
             target: wgpu::TextureFormat,
             layout: &wgpu::PipelineLayout| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: shader_module,
                        entry_point: Some("vs_fullscreen"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader_module,
                        entry_point: Some(fs),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target,
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
                })
            };

        let bloom_threshold = make_fullscreen_pipeline(
            "Bloom Threshold",
            &bloom_shader,
            "fs_threshold",
            HDR_FORMAT,
            &bloom_pl,
        );
        let bloom_downsample = make_fullscreen_pipeline(
            "Bloom Downsample",
            &bloom_shader,
            "fs_downsample",
            HDR_FORMAT,
            &bloom_pl,
        );
        let bloom_upsample = make_fullscreen_pipeline(
            "Bloom Upsample",
            &bloom_shader,
            "fs_upsample",
            HDR_FORMAT,
            &bloom_pl,
        );

        let bloom_uniform = BloomUniform::default();
        let bloom_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Bloom Uniform"),
            contents: bytemuck::cast_slice(&[bloom_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bloom_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // Tonemap レイアウト: hdr + sampler + bloom + post uniform
        let tonemap_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tonemap BGL"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let tonemap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tonemap Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::TONEMAP_SHADER.into()),
        });
        let tonemap_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tonemap PL"),
            bind_group_layouts: &[Some(&tonemap_layout)],
            immediate_size: 0,
        });
        // トーンマップは LDR (sRGB surface) に出力
        let tonemap = make_fullscreen_pipeline(
            "Tonemap",
            &tonemap_shader,
            "fs_main",
            surface_format,
            &tonemap_pl,
        );

        let post_uniform = PostUniform::default();
        let post_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Post Uniform"),
            contents: bytemuck::cast_slice(&[post_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // FXAA レイアウト: input + sampler のみ
        let fxaa_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("FXAA BGL"),
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
        });
        let fxaa_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("FXAA Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::FXAA_SHADER.into()),
        });
        let fxaa_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("FXAA PL"),
            bind_group_layouts: &[Some(&fxaa_layout)],
            immediate_size: 0,
        });
        let fxaa =
            make_fullscreen_pipeline("FXAA", &fxaa_shader, "fs_main", surface_format, &fxaa_pl);

        Self {
            bloom_layout,
            bloom_threshold,
            bloom_downsample,
            bloom_upsample,
            bloom_uniform_buffer,
            bloom_sampler,
            tonemap_layout,
            tonemap,
            post_uniform_buffer,
            fxaa_layout,
            fxaa,
        }
    }
}

// =============================================================================
// Round 7: 追加ポストエフェクト (PostComposite, DOF, MotionBlur, ColorGrading)
// =============================================================================

/// HDR ポストエフェクト合成 (SSAO + SSR + Volumetric)
///
/// HDR 入力 + AO + SSR + Volumetric を 1 パスで合成して新しい HDR テクスチャを出力する。
pub struct PostComposite {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform: PostCompositeUniform,
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PostCompositeUniform {
    /// x = ssao_strength, y = ssr_enabled, z = volumetric_enabled, w = ao_ambient_only
    pub params: [f32; 4],
}

impl Default for PostCompositeUniform {
    fn default() -> Self {
        Self {
            params: [0.7, 1.0, 1.0, 1.0],
        }
    }
}

impl PostComposite {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Post Composite HDR"),
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
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Post Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let uniform = PostCompositeUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Post Composite Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Post Composite BGL"),
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
                // 1: HDR input
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
                // 2: SSAO
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 3: SSR
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 4: Volumetric
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader_source = r#"
struct PostCompositeUniform {
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: PostCompositeUniform;
@group(0) @binding(1) var t_hdr: texture_2d<f32>;
@group(0) @binding(2) var t_ao: texture_2d<f32>;
@group(0) @binding(3) var t_ssr: texture_2d<f32>;
@group(0) @binding(4) var t_volumetric: texture_2d<f32>;
@group(0) @binding(5) var s: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var color = textureSampleLevel(t_hdr, s, in.uv, 0.0);
    let ao = textureSampleLevel(t_ao, s, in.uv, 0.0).r;
    let ssr = textureSampleLevel(t_ssr, s, in.uv, 0.0);
    let vol = textureSampleLevel(t_volumetric, s, in.uv, 0.0).rgb;

    // SSAO: 0..1 で乗算 (strength で減衰)
    let ssao_strength = u.params.x;
    let ao_factor = mix(1.0, ao, ssao_strength);
    color = vec4<f32>(color.rgb * ao_factor, color.a);

    // SSR: 加算
    if u.params.y > 0.5 {
        color = vec4<f32>(color.rgb + ssr.rgb, color.a);
    }

    // Volumetric: 加算
    if u.params.z > 0.5 {
        color = vec4<f32>(color.rgb + vol, color.a);
    }

    return color;
}
"#;

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Post Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Post Composite PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Post Composite Pipeline"),
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
            bind_group_layout,
            pipeline,
            uniform_buffer,
            uniform,
            output_texture,
            output_view,
            sampler,
        }
    }

    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        hdr_view: &wgpu::TextureView,
        ao_view: &wgpu::TextureView,
        ssr_view: &wgpu::TextureView,
        vol_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Post Composite BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ssr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(vol_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

/// 被写界深度 (DOF) ポストエフェクト
pub struct DepthOfField {
    pub uniform: DofUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct DofUniform {
    /// x = focus_distance, y = focus_range, z = max_blur_radius_px, w = enabled
    pub params: [f32; 4],
}

impl Default for DofUniform {
    fn default() -> Self {
        Self {
            params: [10.0, 5.0, 8.0, 0.0],
        }
    }
}

impl DepthOfField {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DOF Output"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = DofUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("DOF Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("DOF Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("DOF BGL"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("DOF Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::DOF_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("DOF PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("DOF Pipeline"),
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
                    format: surface_format,
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
            bind_group_layout,
            pipeline,
            output_texture,
            output_view,
            sampler,
        }
    }

    pub fn set_focus(
        &mut self,
        queue: &wgpu::Queue,
        distance: f32,
        range: f32,
        max_radius: f32,
        enabled: bool,
    ) {
        self.uniform.params = [distance, range, max_radius, if enabled { 1.0 } else { 0.0 }];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DOF BG"),
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
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

/// モーションブラーポストエフェクト
pub struct MotionBlur {
    pub uniform: MotionBlurUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MotionBlurUniform {
    /// x = strength, y = max_pixel_offset, z = enabled, w = _
    pub params: [f32; 4],
}

impl Default for MotionBlurUniform {
    fn default() -> Self {
        Self {
            params: [1.0, 16.0, 0.0, 0.0],
        }
    }
}

impl MotionBlur {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Motion Blur Output"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = MotionBlurUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Motion Blur Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Motion Blur Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Motion Blur BGL"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Motion Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::MOTION_BLUR_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Motion Blur PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Motion Blur Pipeline"),
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
                    format: surface_format,
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
            bind_group_layout,
            pipeline,
            output_texture,
            output_view,
            sampler,
        }
    }

    pub fn set_params(
        &mut self,
        queue: &wgpu::Queue,
        strength: f32,
        max_offset_px: f32,
        enabled: bool,
    ) {
        self.uniform.params = [
            strength,
            max_offset_px,
            if enabled { 1.0 } else { 0.0 },
            0.0,
        ];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        color_view: &wgpu::TextureView,
        motion_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Motion Blur BG"),
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
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(motion_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

/// カラーグレーディング + ヴィネット + 色収差
pub struct ColorGrading {
    pub uniform: ColorGradingUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ColorGradingUniform {
    /// rgb = lift, w = enabled
    pub lift: [f32; 4],
    /// rgb = gamma, w = _
    pub gamma: [f32; 4],
    /// rgb = gain, w = _
    pub gain: [f32; 4],
    /// x = saturation, y = contrast, z = exposure_offset, w = vignette_strength
    pub misc: [f32; 4],
    /// x = vignette_radius, y = chromatic_aberration_strength, z = _, w = _
    pub misc2: [f32; 4],
}

impl Default for ColorGradingUniform {
    fn default() -> Self {
        Self {
            lift: [0.0, 0.0, 0.0, 0.0],
            gamma: [1.0, 1.0, 1.0, 0.0],
            gain: [1.0, 1.0, 1.0, 0.0],
            misc: [1.0, 1.0, 0.0, 0.5],
            misc2: [0.6, 0.0, 0.0, 0.0],
        }
    }
}

impl ColorGrading {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let uniform = ColorGradingUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Color Grading Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Color Grading Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color Grading BGL"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Color Grading Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::COLOR_GRADING_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Color Grading PL"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Color Grading Pipeline"),
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
                    format: surface_format,
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
            bind_group_layout,
            pipeline,
            sampler,
        }
    }

    pub fn set_params(&mut self, queue: &wgpu::Queue, uniform: ColorGradingUniform) {
        self.uniform = uniform;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        color_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color Grading BG"),
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
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_uniform_size() {
        // 16 B
        assert_eq!(std::mem::size_of::<PostUniform>(), 16);
    }

    #[test]
    fn test_bloom_uniform_size() {
        assert_eq!(std::mem::size_of::<BloomUniform>(), 16);
    }

    #[test]
    fn test_bloom_uniform_default() {
        let u = BloomUniform::default();
        assert!((u.params[0] - 1.0).abs() < 1e-5);
        assert!((u.params[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_post_uniform_default() {
        let u = PostUniform::default();
        assert!((u.params[0] - 1.0).abs() < 1e-5);
        assert!((u.params[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_bloom_mips_constant() {
        assert_eq!(BLOOM_MIPS, 5);
    }

    #[test]
    fn test_msaa_samples() {
        assert_eq!(MSAA_SAMPLES, 4);
    }

    #[test]
    fn test_post_composite_uniform_size() {
        assert_eq!(std::mem::size_of::<PostCompositeUniform>(), 16);
    }

    #[test]
    fn test_dof_uniform_size() {
        assert_eq!(std::mem::size_of::<DofUniform>(), 16);
    }

    #[test]
    fn test_motion_blur_uniform_size() {
        assert_eq!(std::mem::size_of::<MotionBlurUniform>(), 16);
    }

    #[test]
    fn test_color_grading_uniform_size() {
        // 5 vec4 = 80 B
        assert_eq!(std::mem::size_of::<ColorGradingUniform>(), 80);
    }
}
