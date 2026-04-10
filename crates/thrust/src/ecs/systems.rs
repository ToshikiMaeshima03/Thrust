use hecs::World;

use crate::ecs::components::{ActiveAmbientLight, ActiveCamera, MeshHandle, RenderState, Visible};
use crate::ecs::resources::Resources;
use crate::light::light::{
    AmbientLight, DirectionalLight, GpuLight, LightsHeader, MAX_DIR_LIGHTS, MAX_POINT_LIGHTS,
    MAX_SPOT_LIGHTS, PointLight, SpotLight,
};
use crate::material::material::{Material, MaterialUniform};
use crate::renderer::render_pass::RenderResult;
use crate::renderer::shadow::{NUM_CASCADES, ShadowMap};
use crate::renderer::shadow_atlas::{
    MAX_POINT_SHADOWS, MAX_SPOT_SHADOWS, POINT_SHADOW_FAR, ShadowAtlas, ShadowAtlasUniform,
};
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::scene::ModelUniform;
use crate::scene::transform::Transform;

/// カメラシステム: OrbitalController でカメラを更新し、GPU バッファに書き込む
pub fn camera_system(world: &mut World, res: &mut Resources) {
    use crate::camera::camera::Camera;
    use crate::camera::controller::OrbitalController;

    // OrbitalController を持つカメラを更新
    for (camera, controller) in world.query_mut::<(&mut Camera, &mut OrbitalController)>() {
        controller.update_camera(camera);
    }

    // ActiveCamera のビュー行列を GPU にアップロード
    if let Some((camera, _)) = world
        .query_mut::<(&Camera, &ActiveCamera)>()
        .into_iter()
        .next()
    {
        res.camera_uniform.update(camera);
        // Round 7: ビューポートサイズ + 時間情報を更新
        res.camera_uniform.update_viewport_time(
            res.gpu.size.width,
            res.gpu.size.height,
            res.time.elapsed(),
            res.time.delta(),
            res.time.frame_count() as u32,
        );
        res.gpu.queue.write_buffer(
            &res.camera_buffer,
            0,
            bytemuck::cast_slice(&[res.camera_uniform]),
        );
    }
}

/// ライトシステム (Round 4): すべての DirectionalLight / PointLight / SpotLight を収集し
/// LightsHeader uniform + lights storage buffer に詰める。
pub fn light_system(world: &World, res: &mut Resources) {
    use wgpu::util::DeviceExt;

    // 環境光: 最初の ActiveAmbientLight を採用、なければデフォルト
    let amb_light = world
        .query::<(&AmbientLight, &ActiveAmbientLight)>()
        .iter()
        .next()
        .map(|(l, _)| AmbientLight {
            color: l.color,
            intensity: l.intensity,
        })
        .unwrap_or_default();

    let mut gpu_lights: Vec<GpuLight> = Vec::with_capacity(64);
    let mut dir_count = 0u32;
    let mut point_count = 0u32;
    let mut spot_count = 0u32;

    // Directional lights
    for light in world.query::<&DirectionalLight>().iter() {
        if dir_count >= MAX_DIR_LIGHTS as u32 {
            break;
        }
        gpu_lights.push(GpuLight::directional(
            light.direction,
            light.color,
            light.intensity,
        ));
        dir_count += 1;
    }

    // Point lights — GlobalTransform.translation を位置に使う
    for (light, transform, gt) in world
        .query::<(&PointLight, &Transform, Option<&GlobalTransform>)>()
        .iter()
    {
        if point_count >= MAX_POINT_LIGHTS as u32 {
            break;
        }
        let pos = match gt {
            Some(g) => g.0.w_axis.truncate(),
            None => transform.translation,
        };
        gpu_lights.push(GpuLight::point(
            pos,
            light.color,
            light.intensity,
            light.range,
        ));
        point_count += 1;
    }

    // Spot lights
    for (light, transform, gt) in world
        .query::<(&SpotLight, &Transform, Option<&GlobalTransform>)>()
        .iter()
    {
        if spot_count >= MAX_SPOT_LIGHTS as u32 {
            break;
        }
        let (pos, dir) = match gt {
            Some(g) => {
                let p = g.0.w_axis.truncate();
                let rot_mat = glam::Mat3::from_cols(
                    g.0.x_axis.truncate(),
                    g.0.y_axis.truncate(),
                    g.0.z_axis.truncate(),
                );
                (p, rot_mat * light.direction)
            }
            None => (transform.translation, transform.rotation * light.direction),
        };
        gpu_lights.push(GpuLight::spot(
            pos,
            dir,
            light.color,
            light.intensity,
            light.range,
            light.inner_angle,
            light.outer_angle,
        ));
        spot_count += 1;
    }

    // 最低 1 要素 (空でも GPU バッファは確保)
    if gpu_lights.is_empty() {
        gpu_lights.push(GpuLight::directional(
            glam::Vec3::ONE.normalize(),
            glam::Vec3::ZERO,
            0.0,
        ));
    }

    // CSM カスケードを計算 (1 つ目の directional light を使用)
    let mut shadow_enabled = false;
    let mut shadow_dir = glam::Vec3::Y;
    if let Some(light) = world.query::<&DirectionalLight>().iter().next() {
        shadow_dir = light.direction;
        shadow_enabled = true;
    }

    // ActiveCamera を取得
    let camera_opt = world
        .query::<(&crate::camera::camera::Camera, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(c, _)| {
            (
                c.position, c.target, c.up, c.fov_y, c.aspect, c.z_near, c.z_far,
            )
        });

    if let Some((pos, target, up, fov_y, aspect, z_near, z_far)) = camera_opt {
        let camera = crate::camera::camera::Camera {
            position: pos,
            target,
            up,
            fov_y,
            aspect,
            z_near,
            z_far,
        };
        res.shadow_map.light_vp = ShadowMap::compute_cascades(&camera, shadow_dir);
        if !shadow_enabled {
            res.shadow_map.light_vp.splits[3] = 0.0;
        }
    } else {
        res.shadow_map.light_vp = crate::renderer::shadow::CsmUniform::default();
    }
    res.gpu.queue.write_buffer(
        &res.shadow_map.light_vp_buffer,
        0,
        bytemuck::cast_slice(&[res.shadow_map.light_vp]),
    );

    // Round 7: 点光源/スポットシャドウアトラスを更新
    {
        let mut atlas_uniform = ShadowAtlasUniform::default();
        let mut atlas_point_idx = 0usize;
        let mut atlas_spot_idx = 0usize;
        for (light, transform, gt) in world
            .query::<(&PointLight, &Transform, Option<&GlobalTransform>)>()
            .iter()
        {
            if atlas_point_idx >= MAX_POINT_SHADOWS {
                break;
            }
            let pos = match gt {
                Some(g) => g.0.w_axis.truncate(),
                None => transform.translation,
            };
            // ライトの range が極端に小さい場合は cap しておく
            let far = if light.range > 0.0 {
                light.range.min(POINT_SHADOW_FAR)
            } else {
                POINT_SHADOW_FAR
            };
            atlas_uniform.point_shadows[atlas_point_idx] =
                ShadowAtlasUniform::build_point(pos, far);
            // 各 face の VP buffer も更新
            for face in 0..6 {
                let buf_idx = ShadowAtlas::point_face_index(atlas_point_idx, face);
                let vp = atlas_uniform.point_shadows[atlas_point_idx].face_vp[face];
                res.gpu.queue.write_buffer(
                    &res.shadow_atlas.light_vp_buffers[buf_idx],
                    0,
                    bytemuck::cast_slice(&[vp]),
                );
            }
            atlas_point_idx += 1;
        }
        for (light, transform, gt) in world
            .query::<(&SpotLight, &Transform, Option<&GlobalTransform>)>()
            .iter()
        {
            if atlas_spot_idx >= MAX_SPOT_SHADOWS {
                break;
            }
            let (pos, dir) = match gt {
                Some(g) => {
                    let p = g.0.w_axis.truncate();
                    let rot_mat = glam::Mat3::from_cols(
                        g.0.x_axis.truncate(),
                        g.0.y_axis.truncate(),
                        g.0.z_axis.truncate(),
                    );
                    (p, rot_mat * light.direction)
                }
                None => (transform.translation, transform.rotation * light.direction),
            };
            let far = if light.range > 0.0 {
                light.range.min(POINT_SHADOW_FAR)
            } else {
                POINT_SHADOW_FAR
            };
            let (vp, p) = ShadowAtlasUniform::build_spot(pos, dir, light.outer_angle, far);
            atlas_uniform.spot_vp[atlas_spot_idx] = vp;
            atlas_uniform.spot_pos[atlas_spot_idx] = p;
            let buf_idx = ShadowAtlas::spot_index(atlas_spot_idx);
            res.gpu.queue.write_buffer(
                &res.shadow_atlas.light_vp_buffers[buf_idx],
                0,
                bytemuck::cast_slice(&[vp]),
            );
            atlas_spot_idx += 1;
        }
        atlas_uniform.counts = [atlas_point_idx as u32, atlas_spot_idx as u32, 0, 0];
        res.shadow_atlas.uniform = atlas_uniform;
        res.gpu.queue.write_buffer(
            &res.shadow_atlas.uniform_buffer,
            0,
            bytemuck::cast_slice(&[atlas_uniform]),
        );
    }

    // ヘッダー更新
    res.lights_header = LightsHeader::new(&amb_light, dir_count, point_count, spot_count);
    res.gpu.queue.write_buffer(
        &res.lights_header_buffer,
        0,
        bytemuck::cast_slice(&[res.lights_header]),
    );

    // storage buffer 容量チェック (足りない場合は再作成)
    if gpu_lights.len() > res.lights_storage_capacity {
        let new_cap = gpu_lights.len().next_power_of_two().max(8);
        res.lights_storage_buffer =
            res.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Lights Storage Buffer"),
                    contents: bytemuck::cast_slice(&gpu_lights),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });
        res.lights_storage_capacity = new_cap;

        // bind group の再構築が必要
        res.camera_bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Camera Bind Group"),
                layout: &res.bind_group_layouts.camera,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: res.camera_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: res.lights_header_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: res.lights_storage_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&res.shadow_map.array_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&res.shadow_map.sampler_cmp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: res.shadow_map.light_vp_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&res.ibl.irradiance_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&res.ibl.prefilter_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::TextureView(&res.ibl.brdf_lut_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::Sampler(&res.ibl.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: res.fog.buffer.as_entire_binding(),
                    },
                ],
            });
    } else {
        res.gpu.queue.write_buffer(
            &res.lights_storage_buffer,
            0,
            bytemuck::cast_slice(&gpu_lights),
        );
    }
}

/// レンダリング準備システム: 新規エンティティの GPU バッファ作成 + ダーティバッファの更新
pub fn render_prep_system(world: &mut World, res: &mut Resources) {
    use wgpu::util::DeviceExt;

    // Step 1: RenderState を持たない描画エンティティを検出
    let needs_init: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &MeshHandle, &Transform, &Material)>()
        .without::<&RenderState>()
        .iter()
        .map(|(entity, _mh, _t, _m)| entity)
        .collect();

    // Step 2: 各新規エンティティに RenderState を作成して挿入
    for entity in needs_init {
        let (model_uniform, material_uniform, base_tex, mr_tex, normal_tex, ao_tex, em_tex) = {
            let Ok(entity_ref) = world.entity(entity) else {
                log::warn!("エンティティが見つかりません（削除済みの可能性）");
                continue;
            };
            let Some(t) = entity_ref.get::<&Transform>() else {
                log::warn!("Transform コンポーネントが見つかりません");
                continue;
            };
            let Some(m) = entity_ref.get::<&Material>() else {
                log::warn!("Material コンポーネントが見つかりません");
                continue;
            };
            let base = m
                .base_color_map
                .clone()
                .unwrap_or_else(|| res.fallback_texture.clone());
            let mr = m
                .metallic_roughness_map
                .clone()
                .unwrap_or_else(|| res.fallback_mr.clone());
            let normal = m
                .normal_map
                .clone()
                .unwrap_or_else(|| res.fallback_normal.clone());
            let ao = m
                .occlusion_map
                .clone()
                .unwrap_or_else(|| res.fallback_mr.clone());
            let em = m
                .emissive_map
                .clone()
                .unwrap_or_else(|| res.fallback_texture.clone());
            (
                ModelUniform::from_transform(&t),
                MaterialUniform::from_material(&m),
                base,
                mr,
                normal,
                ao,
                em,
            )
        };

        let model_buffer = res
            .gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Model Uniform Buffer"),
                contents: bytemuck::cast_slice(&[model_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        // SkinnedMesh があれば固有 joint buffer、なければフォールバック
        let (joint_buffer, owns_joint_buffer) = {
            if let Ok(sm_ref) = world.entity(entity)
                && let Some(sm) = sm_ref.get::<&crate::animation::SkinnedMesh>()
            {
                let initial: Vec<[[f32; 4]; 4]> = sm
                    .joint_matrices
                    .iter()
                    .map(|m| m.to_cols_array_2d())
                    .collect();
                let buf = res
                    .gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Skinned Joint Buffer"),
                        contents: bytemuck::cast_slice(&initial),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
                (buf, true)
            } else {
                let buf = res
                    .gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Identity Joint Buffer"),
                        contents: bytemuck::cast_slice(&[glam::Mat4::IDENTITY.to_cols_array_2d()]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
                (buf, false)
            }
        };

        let model_bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Model Bind Group"),
                layout: &res.bind_group_layouts.model,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: model_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: joint_buffer.as_entire_binding(),
                    },
                ],
            });

        let material_buffer =
            res.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Material Uniform Buffer"),
                    contents: bytemuck::cast_slice(&[material_uniform]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        let material_bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("PBR Material Bind Group"),
                layout: &res.bind_group_layouts.material,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&base_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&base_tex.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&mr_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&normal_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&ao_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&em_tex.view),
                    },
                ],
            });

        let render_state = RenderState {
            model_buffer,
            model_bind_group,
            material_buffer,
            material_bind_group,
            joint_buffer,
            owns_joint_buffer,
        };

        if let Err(e) = world.insert_one(entity, render_state) {
            log::warn!("RenderState 挿入失敗（エンティティ削除済みの可能性）: {e}");
        }
    }

    // Step 3: BoundingVolume を持たないメッシュエンティティに自動挿入
    let needs_bv: Vec<(hecs::Entity, crate::math::Aabb)> = world
        .query::<(hecs::Entity, &MeshHandle)>()
        .without::<&crate::renderer::frustum::BoundingVolume>()
        .iter()
        .map(|(entity, mesh)| (entity, mesh.0.local_aabb))
        .collect();

    for (entity, local_aabb) in needs_bv {
        let _ = world.insert_one(entity, crate::renderer::frustum::BoundingVolume(local_aabb));
    }

    // Step 4: 全 Transform/Material の GPU バッファを更新
    for (transform, material, render_state, global_transform) in world.query_mut::<(
        &Transform,
        &Material,
        &RenderState,
        Option<&GlobalTransform>,
    )>() {
        let matrix = match global_transform {
            Some(gt) => gt.0,
            None => transform.to_matrix(),
        };
        let normal_matrix = matrix.inverse().transpose();
        let uniform = ModelUniform {
            model: matrix.to_cols_array_2d(),
            normal_matrix: normal_matrix.to_cols_array_2d(),
        };
        res.gpu.queue.write_buffer(
            &render_state.model_buffer,
            0,
            bytemuck::cast_slice(&[uniform]),
        );

        let mat_uniform = MaterialUniform::from_material(material);
        res.gpu.queue.write_buffer(
            &render_state.material_buffer,
            0,
            bytemuck::cast_slice(&[mat_uniform]),
        );
    }

    // Step 5: BoundingVolume をワールド空間に変換更新
    for (mesh, bv, transform, global_transform) in world.query_mut::<(
        &MeshHandle,
        &mut crate::renderer::frustum::BoundingVolume,
        &Transform,
        Option<&GlobalTransform>,
    )>() {
        let matrix = match global_transform {
            Some(gt) => gt.0,
            None => transform.to_matrix(),
        };
        bv.0 = mesh.0.local_aabb.transformed(&matrix);
    }

    // Step 6: InstancedMesh のマテリアルバインドグループを生成 (Round 5)
    let instanced_needs_init: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &crate::renderer::instancing::InstancedMesh)>()
        .iter()
        .filter(|(_e, im)| im.material_bind_group.is_none())
        .map(|(e, _im)| e)
        .collect();

    for entity in instanced_needs_init {
        let material_uniform =
            match world.get::<&crate::renderer::instancing::InstancedMesh>(entity) {
                Ok(im) => MaterialUniform::from_material(&im.material),
                Err(_) => continue,
            };
        let material_buffer =
            res.gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instanced Material Buffer"),
                    contents: bytemuck::cast_slice(&[material_uniform]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        // テクスチャは fallback で揃える (instanced はシンプル PBR 想定)
        let base = res.fallback_texture.clone();
        let mr = res.fallback_mr.clone();
        let normal = res.fallback_normal.clone();
        let ao = res.fallback_mr.clone();
        let em = res.fallback_texture.clone();

        let material_bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Instanced PBR Material Bind Group"),
                layout: &res.bind_group_layouts.material,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&base.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&base.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&mr.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&normal.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&ao.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&em.view),
                    },
                ],
            });

        if let Ok(mut im) = world.get::<&mut crate::renderer::instancing::InstancedMesh>(entity) {
            im.material_buffer = Some(material_buffer);
            im.material_bind_group = Some(material_bind_group);
        }
    }
}

/// レンダリングシステム (Round 4 後半):
/// 1. シャドウパス (深度のみ)
/// 2. HDR メインパス (PBR + skybox + particles, MSAA 4× → resolve)
/// 3. Bloom 抽出 + downsample chain + upsample chain
/// 4. Tonemap composite (HDR + bloom → LDR ping)
/// 5. FXAA → surface
///
/// `present_immediately = true` なら surface を直接 present する。
/// `false` なら present せず、surface_texture を Resources に保留する
/// (egui パスを後段で追加するため)。
pub fn render_system(
    world: &World,
    res: &mut Resources,
    present_immediately: bool,
) -> RenderResult {
    let output = res.gpu.surface.get_current_texture();

    let surface_texture = match output {
        wgpu::CurrentSurfaceTexture::Success(tex)
        | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Outdated
        | wgpu::CurrentSurfaceTexture::Lost => return RenderResult::SurfaceLost,
        wgpu::CurrentSurfaceTexture::Occluded => return RenderResult::Ok,
        wgpu::CurrentSurfaceTexture::Validation => {
            log::error!("サーフェスバリデーションエラー");
            return RenderResult::Error;
        }
    };

    let surface_view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = res
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

    // ===== 0. Geometry Prepass (Round 7: depth + normal + material + motion) =====
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Geometry Prepass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &res.prepass.normal_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.5,
                            b: 0.5,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &res.prepass.material_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &res.prepass.motion_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &res.prepass.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&res.prepass.pipeline);
        pass.set_bind_group(0, &res.camera_bind_group, &[]);

        let frustum = crate::renderer::frustum::Frustum::from_view_projection(
            &glam::Mat4::from_cols_array_2d(&res.camera_uniform.view_proj),
        );

        for (mesh, render_state, visible, bounding_volume) in world
            .query::<(
                &MeshHandle,
                &RenderState,
                Option<&Visible>,
                Option<&crate::renderer::frustum::BoundingVolume>,
            )>()
            .iter()
        {
            if let Some(vis) = visible
                && !vis.0
            {
                continue;
            }
            if let Some(bv) = bounding_volume
                && !frustum.intersects_aabb(&bv.0)
            {
                continue;
            }
            pass.set_bind_group(1, &render_state.model_bind_group, &[]);
            pass.set_bind_group(2, &render_state.material_bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.0.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.0.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.0.num_indices, 0, 0..1);
        }
    }

    // ===== 1. シャドウパス (CSM 3 カスケード) =====
    if res.shadow_map.light_vp.splits[3] >= 0.5 {
        for cascade in 0..NUM_CASCADES {
            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CSM Shadow Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &res.shadow_map.layer_views[cascade],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            shadow_pass.set_pipeline(&res.shadow_map.pipeline);
            shadow_pass.set_bind_group(0, &res.shadow_map.cascade_bind_groups[cascade], &[]);

            for (mesh, render_state, visible) in world
                .query::<(&MeshHandle, &RenderState, Option<&Visible>)>()
                .iter()
            {
                if let Some(vis) = visible
                    && !vis.0
                {
                    continue;
                }
                shadow_pass.set_bind_group(1, &render_state.model_bind_group, &[]);
                shadow_pass.set_vertex_buffer(0, mesh.0.vertex_buffer.slice(..));
                shadow_pass
                    .set_index_buffer(mesh.0.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                shadow_pass.draw_indexed(0..mesh.0.num_indices, 0, 0..1);
            }
        }
    } else {
        // Disable: clear all cascades
        for cascade in 0..NUM_CASCADES {
            let _shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CSM Shadow Pass (cleared)"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &res.shadow_map.layer_views[cascade],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
    }

    // ===== 1.5 シャドウアトラスパス (Round 7: 点光源 cube + スポット 2D) =====
    {
        let num_points = res.shadow_atlas.uniform.counts[0] as usize;
        let num_spots = res.shadow_atlas.uniform.counts[1] as usize;
        // 点光源: 各 light の 6 face を順に描画
        for point_idx in 0..MAX_POINT_SHADOWS {
            let active = point_idx < num_points;
            for face in 0..6 {
                let buf_idx = ShadowAtlas::point_face_index(point_idx, face);
                let view_idx = point_idx * 6 + face;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Point Shadow Face Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &res.shadow_atlas.cube_face_views[view_idx],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if !active {
                    continue;
                }
                pass.set_pipeline(&res.shadow_atlas.pipeline);
                pass.set_bind_group(0, &res.shadow_atlas.bind_groups[buf_idx], &[]);
                for (mesh, render_state, visible) in world
                    .query::<(&MeshHandle, &RenderState, Option<&Visible>)>()
                    .iter()
                {
                    if let Some(vis) = visible
                        && !vis.0
                    {
                        continue;
                    }
                    pass.set_bind_group(1, &render_state.model_bind_group, &[]);
                    pass.set_vertex_buffer(0, mesh.0.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.0.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.0.num_indices, 0, 0..1);
                }
            }
        }
        // スポットライト: 各 light の 2D shadow を描画
        for spot_idx in 0..MAX_SPOT_SHADOWS {
            let active = spot_idx < num_spots;
            let buf_idx = ShadowAtlas::spot_index(spot_idx);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Spot Shadow Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &res.shadow_atlas.spot_layer_views[spot_idx],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !active {
                continue;
            }
            pass.set_pipeline(&res.shadow_atlas.pipeline);
            pass.set_bind_group(0, &res.shadow_atlas.bind_groups[buf_idx], &[]);
            for (mesh, render_state, visible) in world
                .query::<(&MeshHandle, &RenderState, Option<&Visible>)>()
                .iter()
            {
                if let Some(vis) = visible
                    && !vis.0
                {
                    continue;
                }
                pass.set_bind_group(1, &render_state.model_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.0.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.0.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.0.num_indices, 0, 0..1);
            }
        }
    }

    // ===== 2. HDR メインパス (MSAA 4×) =====
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HDR Main Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.hdr_targets.color_msaa,
                depth_slice: None,
                resolve_target: Some(&res.hdr_targets.color_resolved_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &res.hdr_targets.depth_msaa,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // PBR メッシュ描画
        render_pass.set_pipeline(&res.pipeline);
        render_pass.set_bind_group(0, &res.camera_bind_group, &[]);
        render_pass.set_bind_group(3, &res.shadow_atlas_bind_group, &[]);

        let frustum = crate::renderer::frustum::Frustum::from_view_projection(
            &glam::Mat4::from_cols_array_2d(&res.camera_uniform.view_proj),
        );

        for (mesh, render_state, visible, bounding_volume) in world
            .query::<(
                &MeshHandle,
                &RenderState,
                Option<&Visible>,
                Option<&crate::renderer::frustum::BoundingVolume>,
            )>()
            .iter()
        {
            if let Some(vis) = visible
                && !vis.0
            {
                continue;
            }
            if let Some(bv) = bounding_volume
                && !frustum.intersects_aabb(&bv.0)
            {
                continue;
            }
            render_pass.set_bind_group(1, &render_state.model_bind_group, &[]);
            render_pass.set_bind_group(2, &render_state.material_bind_group, &[]);
            render_pass.set_vertex_buffer(0, mesh.0.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.0.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.0.num_indices, 0, 0..1);
        }

        // インスタンスドメッシュ描画 (Round 5)
        render_pass.set_pipeline(&res.instanced_pipeline);
        render_pass.set_bind_group(0, &res.camera_bind_group, &[]);
        for im in world
            .query::<&crate::renderer::instancing::InstancedMesh>()
            .iter()
        {
            if im.instance_count == 0 {
                continue;
            }
            let Some(mat_bg) = &im.material_bind_group else {
                continue;
            };
            // フラスタムカリング (全インスタンスの bounds)
            if !frustum.intersects_aabb(&im.bounds) {
                continue;
            }
            render_pass.set_bind_group(2, mat_bg, &[]);
            render_pass.set_vertex_buffer(0, im.mesh.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, im.instance_buffer.slice(..));
            render_pass.set_index_buffer(im.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..im.mesh.num_indices, 0, 0..im.instance_count);
        }

        // 通常 PBR パイプラインに戻す
        render_pass.set_pipeline(&res.pipeline);
        render_pass.set_bind_group(0, &res.camera_bind_group, &[]);

        // Skybox 描画 (深度 LessEqual で奥に配置)
        render_pass.set_pipeline(&res.skybox.pipeline);
        render_pass.set_bind_group(0, &res.skybox.bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        // パーティクル描画
        crate::particle::particle_render_system(&mut render_pass, res);
    }

    // ===== 2.5 デカールパス (Round 7: 既存 HDR resolved に投影描画) =====
    {
        let decal_query: Vec<hecs::Entity> = world
            .query::<(hecs::Entity, &crate::renderer::decal::Decal)>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        if !decal_query.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Decal Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &res.hdr_targets.color_resolved_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&res.decal_renderer.pipeline);
            pass.set_bind_group(0, &res.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, res.decal_renderer.cube_vertex_buffer.slice(..));
            pass.set_index_buffer(
                res.decal_renderer.cube_index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );

            // 各 decal の bind group をオンザフライで作成 (次回最適化対象)
            for entity in decal_query {
                if let Ok(decal) = world.get::<&crate::renderer::decal::Decal>(entity) {
                    let (_buf, bg) = res.decal_renderer.create_decal_resources(
                        &res.gpu.device,
                        &decal,
                        &res.prepass,
                    );
                    pass.set_bind_group(1, &bg, &[]);
                    pass.draw_indexed(0..res.decal_renderer.num_indices, 0, 0..1);
                }
            }
        }
    }

    // ===== 2.6 SSAO 計算 + ブラー =====
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSAO Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.ssao.ao_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.ssao.pipeline);
        pass.set_bind_group(0, &res.ssao.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSAO Blur Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.ssao.ao_blurred_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.ssao.blur_pipeline);
        pass.set_bind_group(0, &res.ssao.blur_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 2.7 SSR 計算 =====
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSR Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.ssr.reflection_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.ssr.pipeline);
        pass.set_bind_group(0, &res.ssr.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 2.8 ボリュメトリックライト (god rays) =====
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Volumetric Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.volumetric.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.volumetric.pipeline);
        pass.set_bind_group(0, &res.volumetric.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 2.9 ポストエフェクト合成 (HDR + AO + SSR + Volumetric) =====
    {
        let bg = res.post_composite.create_bind_group(
            &res.gpu.device,
            &res.hdr_targets.color_resolved_view,
            &res.ssao.ao_blurred_view,
            &res.ssr.reflection_view,
            &res.volumetric.view,
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Post Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.post_composite.output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post_composite.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 3. Bloom: threshold + downsample chain =====
    let bloom_dev = &res.gpu.device;
    {
        // Threshold: post_composite.output → bloom_chain.mips[0] (Round 7: 合成済み HDR)
        let bg = bloom_dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Threshold BG"),
            layout: &res.post.bloom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&res.post_composite.output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.post.bloom_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: res.post.bloom_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Threshold"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.bloom_chain.mips[0].view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post.bloom_threshold);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // Downsample mips[0] → mips[1] → ... → mips[N-1]
    for i in 1..res.bloom_chain.mips.len() {
        let bg = bloom_dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample BG"),
            layout: &res.post.bloom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&res.bloom_chain.mips[i - 1].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.post.bloom_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: res.post.bloom_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Downsample"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.bloom_chain.mips[i].view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post.bloom_downsample);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // Upsample mips[N-1] → ... → mips[0]
    // ここでは tent upsample で各段を加算的にブレンド
    for i in (1..res.bloom_chain.mips.len()).rev() {
        let bg = bloom_dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample BG"),
            layout: &res.post.bloom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&res.bloom_chain.mips[i].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.post.bloom_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: res.post.bloom_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Upsample"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.bloom_chain.mips[i - 1].view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post.bloom_upsample);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 4. Tonemap (HDR + bloom → ldr_intermediate) =====
    {
        let bg = bloom_dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tonemap BG"),
            layout: &res.post.tonemap_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&res.post_composite.output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.post.bloom_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&res.bloom_chain.mips[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: res.post.post_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tonemap Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.hdr_targets.ldr_intermediate_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post.tonemap);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 5. FXAA (ldr_intermediate → dof.output) =====
    {
        let bg = bloom_dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FXAA BG"),
            layout: &res.post.fxaa_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &res.hdr_targets.ldr_intermediate_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.post.bloom_sampler),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("FXAA Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.dof.output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(res.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.post.fxaa);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 6. DOF (dof.output → motion_blur.output) =====
    {
        let bg = res.dof.create_bind_group(
            &res.gpu.device,
            &res.camera_buffer,
            &res.dof.output_view,
            &res.prepass.depth_view,
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("DOF Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.motion_blur.output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.dof.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 7. Motion Blur (motion_blur.output → dof.output reuse) =====
    {
        let bg = res.motion_blur.create_bind_group(
            &res.gpu.device,
            &res.camera_buffer,
            &res.motion_blur.output_view,
            &res.prepass.motion_view,
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Motion Blur Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &res.dof.output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.motion_blur.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    // ===== 8. Color Grading + Vignette (dof.output → surface) =====
    {
        let bg = res.color_grading.create_bind_group(
            &res.gpu.device,
            &res.camera_buffer,
            &res.dof.output_view,
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Color Grading Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(res.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.color_grading.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    res.gpu.queue.submit(std::iter::once(encoder.finish()));

    if present_immediately {
        surface_texture.present();
    } else {
        res.pending_surface = Some(surface_texture);
    }

    RenderResult::Ok
}
