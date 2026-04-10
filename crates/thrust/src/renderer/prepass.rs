//! Geometry G-Buffer Prepass (Round 7)
//!
//! 1× 非 MSAA で全ジオメトリを再描画し、後段のスクリーン空間エフェクトが
//! 必要とする情報を G-buffer に書き込む。
//!
//! 出力:
//! - **depth** (Depth32Float): 深度
//! - **normal_depth** (Rgba16Float): view 空間法線 (xyz, [0..1] エンコード) + linear depth (w)
//! - **material** (Rgba8Unorm): metallic (r) + roughness (g) + spec_weight (b) + id (a)
//! - **motion** (Rg16Float): モーションベクトル (xy, current_uv - prev_uv)
//!
//! このプリパスは SSAO/SSR/decal/motion blur で共有される。

use crate::mesh::vertex::Vertex;
use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::shader;

/// G-buffer フォーマット
pub const GBUFFER_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const GBUFFER_MATERIAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const GBUFFER_MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
pub const GBUFFER_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// G-buffer リソース
pub struct GeometryPrepass {
    pub width: u32,
    pub height: u32,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub normal_texture: wgpu::Texture,
    pub normal_view: wgpu::TextureView,
    pub material_texture: wgpu::Texture,
    pub material_view: wgpu::TextureView,
    pub motion_texture: wgpu::Texture,
    pub motion_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub pipeline: wgpu::RenderPipeline,
}

impl GeometryPrepass {
    pub fn new(
        device: &wgpu::Device,
        layouts: &ThrustBindGroupLayouts,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };

        let make_color_tex = |label: &str, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Prepass Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GBUFFER_DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let normal_texture = make_color_tex("Prepass Normal+Depth", GBUFFER_NORMAL_FORMAT);
        let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let material_texture = make_color_tex("Prepass Material", GBUFFER_MATERIAL_FORMAT);
        let material_view = material_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let motion_texture = make_color_tex("Prepass Motion", GBUFFER_MOTION_FORMAT);
        let motion_view = motion_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Prepass Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Geometry Prepass Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::PREPASS_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Prepass Pipeline Layout"),
            bind_group_layouts: &[
                Some(&layouts.camera),
                Some(&layouts.model),
                Some(&layouts.material),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Geometry Prepass Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: GBUFFER_NORMAL_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: GBUFFER_MATERIAL_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: GBUFFER_MOTION_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
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
                format: GBUFFER_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            width: w,
            height: h,
            depth_texture,
            depth_view,
            normal_texture,
            normal_view,
            material_texture,
            material_view,
            motion_texture,
            motion_view,
            sampler,
            pipeline,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_gbuffer_format_constants() {
        assert_eq!(
            super::GBUFFER_NORMAL_FORMAT,
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            super::GBUFFER_DEPTH_FORMAT,
            wgpu::TextureFormat::Depth32Float
        );
        assert_eq!(super::GBUFFER_MOTION_FORMAT, wgpu::TextureFormat::Rg16Float);
    }
}
