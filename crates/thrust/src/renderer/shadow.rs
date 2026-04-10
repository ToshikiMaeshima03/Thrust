//! 方向光カスケードシャドウマップ (Round 5)
//!
//! 3 カスケード、各 2048² Depth32Float、テクスチャ配列。
//! カメラフラスタムを 3 つに分割し、各カスケードに最適なライト VP を計算する。
//! フラグメントシェーダー側で view-space 深度から正しいカスケードを選ぶ。

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::camera::camera::Camera;
use crate::math::Aabb;
use crate::mesh::vertex::Vertex;

/// シャドウマップ解像度
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// カスケード数
pub const NUM_CASCADES: usize = 3;
/// カスケード分割係数 (PSSM 風: 対数 + 線形ブレンド)
pub const CASCADE_SPLITS: [f32; NUM_CASCADES] = [0.05, 0.20, 1.0];

/// シャドウマップリソース (CSM 対応)
pub struct ShadowMap {
    pub texture: wgpu::Texture,
    /// 各カスケード用の個別 view (描画ターゲット用)
    pub layer_views: Vec<wgpu::TextureView>,
    /// 配列全体の view (フラグメントシェーダーでサンプル用)
    pub array_view: wgpu::TextureView,
    /// 比較サンプラー (PCF 用)
    pub sampler_cmp: wgpu::Sampler,
    /// ライト空間 VP マトリクス用 uniform
    pub light_vp_buffer: wgpu::Buffer,
    pub light_vp: CsmUniform,
    /// シャドウパス用バインドグループレイアウト (cascade index uniform)
    pub cascade_layout: wgpu::BindGroupLayout,
    /// 各カスケード用 cascade_index uniform バッファとバインドグループ
    pub cascade_uniforms: Vec<wgpu::Buffer>,
    pub cascade_bind_groups: Vec<wgpu::BindGroup>,
    pub pipeline: wgpu::RenderPipeline,
}

/// CSM uniform: 全カスケードの行列 + 分割距離
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CsmUniform {
    /// 各カスケードの light VP マトリクス
    pub matrices: [[[f32; 4]; 4]; NUM_CASCADES],
    /// x = cascade 0 split, y = cascade 1 split, z = cascade 2 split, w = enabled flag
    pub splits: [f32; 4],
}

impl Default for CsmUniform {
    fn default() -> Self {
        Self {
            matrices: [Mat4::IDENTITY.to_cols_array_2d(); NUM_CASCADES],
            splits: [10.0, 30.0, 100.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CascadeIndexUniform {
    /// x = cascade index, yzw = padding
    index: [u32; 4],
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device, model_layout: &wgpu::BindGroupLayout) -> Self {
        // テクスチャ配列 (3 layer)
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("CSM Shadow Map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: NUM_CASCADES as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // 各カスケード用の単層 view (描画ターゲット用)
        let mut layer_views = Vec::with_capacity(NUM_CASCADES);
        for i in 0..NUM_CASCADES {
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("CSM Layer {i}")),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            });
            layer_views.push(view);
        }

        // 配列全体の view (シェーダーサンプル用)
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("CSM Array View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_array_layer: 0,
            array_layer_count: Some(NUM_CASCADES as u32),
            ..Default::default()
        });

        let sampler_cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Comparison Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let light_vp = CsmUniform::default();
        let light_vp_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CSM Uniform Buffer"),
            contents: bytemuck::cast_slice(&[light_vp]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // シャドウパス用バインドグループレイアウト: CSM uniform + cascade index
        let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Cascade Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // 各カスケード用 cascade_index uniform + bind group
        let mut cascade_uniforms = Vec::with_capacity(NUM_CASCADES);
        let mut cascade_bind_groups = Vec::with_capacity(NUM_CASCADES);
        for i in 0..NUM_CASCADES {
            let idx_uniform = CascadeIndexUniform {
                index: [i as u32, 0, 0, 0],
            };
            let idx_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Cascade {i} Index Uniform")),
                contents: bytemuck::cast_slice(&[idx_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Cascade {i} Bind Group")),
                layout: &cascade_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: light_vp_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: idx_buffer.as_entire_binding(),
                    },
                ],
            });
            cascade_uniforms.push(idx_buffer);
            cascade_bind_groups.push(bg);
        }

        // シャドウパイプライン (cascade index で正しい行列を選ぶ)
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shader::SHADOW_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &[Some(&cascade_layout), Some(model_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::buffer_layout()],
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

        Self {
            texture,
            layer_views,
            array_view,
            sampler_cmp,
            light_vp_buffer,
            light_vp,
            cascade_layout,
            cascade_uniforms,
            cascade_bind_groups,
            pipeline,
        }
    }

    /// カメラフラスタムを分割して各カスケードの light VP を計算する
    ///
    /// PSSM (Practical Split Shadow Maps) 風: 対数 + 線形ブレンド。
    /// 各カスケード用にカメラフラスタムスライスのワールド空間コーナーを計算し、
    /// それを覆う最小の orthographic 投影を構築する。
    pub fn compute_cascades(camera: &Camera, light_dir: Vec3) -> CsmUniform {
        let mut uniform = CsmUniform::default();
        let dir = light_dir.normalize_or(Vec3::Y);

        let near = camera.z_near;
        let far = camera.z_far;
        let view_proj = camera.view_projection_matrix();
        let inv_view_proj = view_proj.inverse();

        let mut prev_split = 0.0;
        for (i, split_ratio) in CASCADE_SPLITS.iter().enumerate() {
            let next_split = *split_ratio;
            let split_near = near + (far - near) * prev_split;
            let split_far = near + (far - near) * next_split;

            // NDC 空間で 8 corners (Z=0 が near, Z=1 が far、wgpu/d3d 規約)
            // → 各 corner を world に逆射影
            // → split 範囲に対応する z 値を計算
            // 近似: NDC の Z は深度バッファの z (0..1)
            // しかし perspective なので、直接 frustum 8 corners を計算する方が正確
            let frustum_corners =
                compute_frustum_corners(&inv_view_proj, &view_proj, split_near, split_far, camera);

            // ライト空間 view を構築 (フラスタム中心から逆光方向に置く)
            let center: Vec3 = frustum_corners.iter().sum::<Vec3>() / 8.0;
            let radius = frustum_corners
                .iter()
                .map(|c| (*c - center).length())
                .fold(0.0_f32, f32::max);

            // up が view direction と平行な場合の保護
            let up = if dir.cross(Vec3::Y).length() < 1e-3 {
                Vec3::Z
            } else {
                Vec3::Y
            };

            let light_view = Mat4::look_at_rh(center - dir * radius * 2.0, center, up);

            // ライト空間で AABB を計算
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for corner in &frustum_corners {
                let light_space = light_view.transform_point3(*corner);
                min = min.min(light_space);
                max = max.max(light_space);
            }

            // テクセルスナップ (シャドウのちらつき防止)
            let world_units_per_texel = (max.x - min.x) / SHADOW_MAP_SIZE as f32;
            min.x = (min.x / world_units_per_texel).floor() * world_units_per_texel;
            max.x = (max.x / world_units_per_texel).floor() * world_units_per_texel;
            min.y = (min.y / world_units_per_texel).floor() * world_units_per_texel;
            max.y = (max.y / world_units_per_texel).floor() * world_units_per_texel;

            // Z 範囲を少し拡張 (near plane の外のオブジェクトもキャストするため)
            let z_padding = 50.0;
            let proj = Mat4::orthographic_rh(
                min.x,
                max.x,
                min.y,
                max.y,
                -max.z - z_padding,
                -min.z + z_padding,
            );
            uniform.matrices[i] = (proj * light_view).to_cols_array_2d();
            uniform.splits[i] = split_far;
            prev_split = next_split;
        }
        uniform.splits[3] = 1.0; // enabled

        uniform
    }

    /// シーン AABB が空のときのフォールバック (旧 API 互換)
    pub fn compute_light_vp(scene_aabb: &Aabb, light_dir: Vec3) -> Mat4 {
        let dir = light_dir.normalize_or(Vec3::Y);
        let center = (scene_aabb.min + scene_aabb.max) * 0.5;
        let extents = (scene_aabb.max - scene_aabb.min) * 0.5;
        let radius = extents.length().max(1.0);

        let eye = center - dir * radius * 2.0;
        let up = if dir.cross(Vec3::Y).length() < 1e-3 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(eye, center, up);
        let proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.1, radius * 4.0 + 0.1);
        proj * view
    }
}

/// カメラフラスタムスライスのワールド空間 8 corners を計算する
fn compute_frustum_corners(
    _inv_view_proj: &Mat4,
    _view_proj: &Mat4,
    split_near: f32,
    split_far: f32,
    camera: &Camera,
) -> [Vec3; 8] {
    let view = camera.view_matrix();
    let inv_view = view.inverse();

    let aspect = camera.aspect;
    let fov = camera.fov_y.to_radians();
    let tan_half_fov = (fov * 0.5).tan();

    let near_h = split_near * tan_half_fov;
    let near_w = near_h * aspect;
    let far_h = split_far * tan_half_fov;
    let far_w = far_h * aspect;

    // ビュー空間で 8 corners (右手系: -Z forward)
    let view_corners = [
        Vec3::new(-near_w, -near_h, -split_near),
        Vec3::new(near_w, -near_h, -split_near),
        Vec3::new(near_w, near_h, -split_near),
        Vec3::new(-near_w, near_h, -split_near),
        Vec3::new(-far_w, -far_h, -split_far),
        Vec3::new(far_w, -far_h, -split_far),
        Vec3::new(far_w, far_h, -split_far),
        Vec3::new(-far_w, far_h, -split_far),
    ];

    let mut world_corners = [Vec3::ZERO; 8];
    for (i, vc) in view_corners.iter().enumerate() {
        world_corners[i] = inv_view.transform_point3(*vc);
    }
    world_corners
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csm_uniform_size() {
        // 3 mat4x4 (192 B) + vec4 splits (16 B) = 208 B
        assert_eq!(std::mem::size_of::<CsmUniform>(), 208);
        assert_eq!(std::mem::size_of::<CsmUniform>() % 16, 0);
    }

    #[test]
    fn test_csm_uniform_default() {
        let u = CsmUniform::default();
        assert_eq!(u.matrices.len(), NUM_CASCADES);
    }

    #[test]
    fn test_compute_cascades_centered_camera() {
        let mut camera = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::ZERO, 16.0 / 9.0);
        camera.z_near = 0.1;
        camera.z_far = 100.0;
        let uniform = ShadowMap::compute_cascades(&camera, Vec3::new(0.0, -1.0, 0.0));
        // 各カスケードの行列が finite
        for cascade in &uniform.matrices {
            for col in cascade {
                for v in col {
                    assert!(v.is_finite(), "CSM matrix contains NaN/Inf");
                }
            }
        }
        // 分割距離が単調増加
        assert!(uniform.splits[0] < uniform.splits[1]);
        assert!(uniform.splits[1] < uniform.splits[2]);
    }

    #[test]
    fn test_num_cascades_constant() {
        assert_eq!(NUM_CASCADES, 3);
        assert_eq!(CASCADE_SPLITS.len(), NUM_CASCADES);
    }

    #[test]
    fn test_compute_light_vp_legacy() {
        let aabb = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        let vp = ShadowMap::compute_light_vp(&aabb, Vec3::new(0.0, -1.0, 0.0));
        assert!(vp.x_axis.x.is_finite());
        assert!(vp != Mat4::IDENTITY);
    }

    #[test]
    fn test_frustum_corners_count() {
        let camera = Camera::new(Vec3::ZERO, Vec3::NEG_Z, 1.0);
        let inv_vp = camera.view_projection_matrix().inverse();
        let vp = camera.view_projection_matrix();
        let corners = compute_frustum_corners(&inv_vp, &vp, 0.1, 10.0, &camera);
        assert_eq!(corners.len(), 8);
        for c in &corners {
            assert!(c.x.is_finite() && c.y.is_finite() && c.z.is_finite());
        }
    }
}
