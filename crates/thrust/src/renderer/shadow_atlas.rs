//! 点光源・スポットライト用シャドウマップアトラス (Round 7)
//!
//! - 点光源: 6 面 cubemap × 最大 4 灯 (cube_array)
//! - スポット光: 2D shadow map × 最大 4 灯 (D2 array)
//!
//! PBR シェーダーの bind group 0 に追加され、各ライトのシャドウを参照する。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::shader;

/// 同時にシャドウを計算できる点光源数
pub const MAX_POINT_SHADOWS: usize = 4;
/// 同時にシャドウを計算できるスポット光源数
pub const MAX_SPOT_SHADOWS: usize = 4;
/// 各シャドウマップの解像度
pub const SHADOW_ATLAS_SIZE: u32 = 1024;
/// Cube shadow の z_far
pub const POINT_SHADOW_FAR: f32 = 50.0;

/// 点光源シャドウ用 view-proj matrices (6 面)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PointShadowVp {
    pub face_vp: [[[f32; 4]; 4]; 6],
    pub world_pos: [f32; 4],
    pub far_active: [f32; 4],
}

/// アトラス全体の uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ShadowAtlasUniform {
    pub point_shadows: [PointShadowVp; MAX_POINT_SHADOWS],
    pub spot_vp: [[[f32; 4]; 4]; MAX_SPOT_SHADOWS],
    /// xyz = world pos, w = active
    pub spot_pos: [[f32; 4]; MAX_SPOT_SHADOWS],
    /// 各種カウント x = num_point, y = num_spot, zw = _
    pub counts: [u32; 4],
}

impl Default for ShadowAtlasUniform {
    fn default() -> Self {
        let id4 = glam::Mat4::IDENTITY.to_cols_array_2d();
        Self {
            point_shadows: [PointShadowVp {
                face_vp: [id4; 6],
                world_pos: [0.0; 4],
                far_active: [POINT_SHADOW_FAR, 0.0, 0.0, 0.0],
            }; MAX_POINT_SHADOWS],
            spot_vp: [id4; MAX_SPOT_SHADOWS],
            spot_pos: [[0.0; 4]; MAX_SPOT_SHADOWS],
            counts: [0; 4],
        }
    }
}

impl ShadowAtlasUniform {
    /// 点光源 cube shadow の 6 面 view-proj を計算する
    ///
    /// `position` をライトの world 位置、`far` を最大照射距離として cubemap 用の
    /// 6 方向 view × perspective(90°) を生成する。
    pub fn build_point(position: glam::Vec3, far: f32) -> PointShadowVp {
        let proj = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, far);
        // wgpu のキューブマップ面順: +X, -X, +Y, -Y, +Z, -Z
        let dirs_ups: [(glam::Vec3, glam::Vec3); 6] = [
            (glam::Vec3::X, glam::Vec3::NEG_Y),
            (glam::Vec3::NEG_X, glam::Vec3::NEG_Y),
            (glam::Vec3::Y, glam::Vec3::Z),
            (glam::Vec3::NEG_Y, glam::Vec3::NEG_Z),
            (glam::Vec3::Z, glam::Vec3::NEG_Y),
            (glam::Vec3::NEG_Z, glam::Vec3::NEG_Y),
        ];
        let mut face_vp = [glam::Mat4::IDENTITY.to_cols_array_2d(); 6];
        for (i, (dir, up)) in dirs_ups.iter().enumerate() {
            let view = glam::Mat4::look_at_rh(position, position + *dir, *up);
            face_vp[i] = (proj * view).to_cols_array_2d();
        }
        PointShadowVp {
            face_vp,
            world_pos: [position.x, position.y, position.z, 0.0],
            far_active: [far, 1.0, 0.0, 0.0],
        }
    }

    /// スポットライト用の view-proj
    pub fn build_spot(
        position: glam::Vec3,
        direction: glam::Vec3,
        outer_angle_rad: f32,
        far: f32,
    ) -> ([[f32; 4]; 4], [f32; 4]) {
        let dir = direction.normalize_or_zero();
        let dir = if dir.length_squared() < 1e-5 {
            glam::Vec3::NEG_Y
        } else {
            dir
        };
        let target = position + dir;
        let up = if dir.dot(glam::Vec3::Y).abs() > 0.99 {
            glam::Vec3::Z
        } else {
            glam::Vec3::Y
        };
        let view = glam::Mat4::look_at_rh(position, target, up);
        let fov = (outer_angle_rad * 2.0).clamp(0.1, std::f32::consts::PI - 0.1);
        let proj = glam::Mat4::perspective_rh(fov, 1.0, 0.1, far);
        let vp = (proj * view).to_cols_array_2d();
        let pos = [position.x, position.y, position.z, 1.0];
        (vp, pos)
    }
}

/// 点光源・スポットライト用シャドウマップアトラス全体
pub struct ShadowAtlas {
    pub uniform: ShadowAtlasUniform,
    pub uniform_buffer: wgpu::Buffer,
    /// 点光源 cubemap array (depth32float)
    pub cube_texture: wgpu::Texture,
    /// 全体ビュー (cube_array)
    pub cube_array_view: wgpu::TextureView,
    /// 各面ごとの 2D ビュー (描画用)
    pub cube_face_views: Vec<wgpu::TextureView>,
    /// スポットライト用 D2Array
    pub spot_texture: wgpu::Texture,
    pub spot_array_view: wgpu::TextureView,
    pub spot_layer_views: Vec<wgpu::TextureView>,
    pub sampler_cmp: wgpu::Sampler,
    pub pipeline: wgpu::RenderPipeline,
    /// シャドウ render の cascade BG (1 つの uniform を共有、cascade 番号は push の代わりに immediate)
    pub bind_groups: Vec<wgpu::BindGroup>,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub light_vp_buffers: Vec<wgpu::Buffer>,
}

impl ShadowAtlas {
    pub fn new(device: &wgpu::Device, layouts: &ThrustBindGroupLayouts) -> Self {
        let uniform = ShadowAtlasUniform::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shadow Atlas Uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 点光源 cubemap array (6 * MAX_POINT_SHADOWS layers)
        let cube_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Point Shadow Cubemap Array"),
            size: wgpu::Extent3d {
                width: SHADOW_ATLAS_SIZE,
                height: SHADOW_ATLAS_SIZE,
                depth_or_array_layers: 6 * MAX_POINT_SHADOWS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let cube_array_view = cube_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Point Cubemap Array View"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });

        let mut cube_face_views = Vec::with_capacity(6 * MAX_POINT_SHADOWS);
        for layer in 0..(6 * MAX_POINT_SHADOWS as u32) {
            cube_face_views.push(cube_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Point Cube Face View {layer}")),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            }));
        }

        // スポットライト D2Array
        let spot_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Spot Shadow Array"),
            size: wgpu::Extent3d {
                width: SHADOW_ATLAS_SIZE,
                height: SHADOW_ATLAS_SIZE,
                depth_or_array_layers: MAX_SPOT_SHADOWS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let spot_array_view = spot_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Spot Array View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let mut spot_layer_views = Vec::with_capacity(MAX_SPOT_SHADOWS);
        for layer in 0..(MAX_SPOT_SHADOWS as u32) {
            spot_layer_views.push(spot_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Spot Layer View {layer}")),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            }));
        }

        let sampler_cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        // shadow render は existing CSM shader を再利用 (1 cascade matrix を 1 face VP として書く)
        // 別のシャドウ pipeline (atlas用) を作る
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Atlas Render BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Atlas Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SHADOW_ATLAS_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Atlas PL"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&layouts.model)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Atlas Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[crate::mesh::vertex::Vertex::buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 各 face/layer ごとに individual VP buffer + bind group を作る
        // (6*MAX_POINT_SHADOWS + MAX_SPOT_SHADOWS)
        let total = 6 * MAX_POINT_SHADOWS + MAX_SPOT_SHADOWS;
        let mut light_vp_buffers = Vec::with_capacity(total);
        let mut bind_groups = Vec::with_capacity(total);
        for i in 0..total {
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Shadow Atlas VP Buffer {i}")),
                contents: bytemuck::cast_slice(&[glam::Mat4::IDENTITY.to_cols_array_2d()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Shadow Atlas BG {i}")),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            light_vp_buffers.push(buf);
            bind_groups.push(bg);
        }

        Self {
            uniform,
            uniform_buffer,
            cube_texture,
            cube_array_view,
            cube_face_views,
            spot_texture,
            spot_array_view,
            spot_layer_views,
            sampler_cmp,
            pipeline,
            bind_groups,
            bind_group_layout,
            light_vp_buffers,
        }
    }

    /// 点光源の face VP バッファインデックス
    pub fn point_face_index(point_idx: usize, face: usize) -> usize {
        point_idx * 6 + face
    }

    /// スポットライトの VP バッファインデックス
    pub fn spot_index(spot_idx: usize) -> usize {
        6 * MAX_POINT_SHADOWS + spot_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_constants() {
        assert_eq!(MAX_POINT_SHADOWS, 4);
        assert_eq!(MAX_SPOT_SHADOWS, 4);
    }

    #[test]
    fn test_atlas_uniform_alignment() {
        // ShadowAtlasUniform は 16 バイトアラインの倍数
        let size = std::mem::size_of::<ShadowAtlasUniform>();
        assert_eq!(size % 16, 0, "ShadowAtlasUniform size = {size}");
    }

    #[test]
    fn test_build_point_view() {
        let pos = glam::Vec3::new(0.0, 5.0, 0.0);
        let p = ShadowAtlasUniform::build_point(pos, 50.0);
        assert!((p.world_pos[1] - 5.0).abs() < 1e-5);
        assert!((p.far_active[0] - 50.0).abs() < 1e-5);
        assert!(p.far_active[1] > 0.5);
    }

    #[test]
    fn test_build_spot_view() {
        let pos = glam::Vec3::new(0.0, 10.0, 0.0);
        let dir = glam::Vec3::NEG_Y;
        let (vp, p) = ShadowAtlasUniform::build_spot(pos, dir, 0.5, 50.0);
        assert!((p[1] - 10.0).abs() < 1e-5);
        let mat = glam::Mat4::from_cols_array_2d(&vp);
        // 中心点を投影してみる: 10m 下にある点 (0,0,0) は near plane より奥にある
        let test = mat.project_point3(glam::Vec3::ZERO);
        assert!(test.z >= 0.0 && test.z <= 1.0, "投影 z = {}", test.z);
    }

    #[test]
    fn test_face_index() {
        assert_eq!(ShadowAtlas::point_face_index(0, 0), 0);
        assert_eq!(ShadowAtlas::point_face_index(0, 5), 5);
        assert_eq!(ShadowAtlas::point_face_index(1, 0), 6);
        assert_eq!(ShadowAtlas::point_face_index(3, 5), 23);
    }

    #[test]
    fn test_spot_index() {
        assert_eq!(ShadowAtlas::spot_index(0), 24);
        assert_eq!(ShadowAtlas::spot_index(3), 27);
    }
}
