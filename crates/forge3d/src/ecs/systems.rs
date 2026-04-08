use hecs::World;

use crate::ecs::components::{
    ActiveAmbientLight, ActiveCamera, ActiveDirectionalLight, DirtyFlags, MeshHandle, RenderState,
    Visible,
};
use crate::ecs::resources::Resources;
use crate::light::light::{AmbientLight, DirectionalLight, LightUniform};
use crate::material::material::{Material, MaterialUniform};
use crate::renderer::render_pass::RenderResult;
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
        res.gpu.queue.write_buffer(
            &res.camera_buffer,
            0,
            bytemuck::cast_slice(&[res.camera_uniform]),
        );
    }
}

/// ライトシステム: DirectionalLight と AmbientLight をクエリし、GPU バッファに書き込む
pub fn light_system(world: &World, res: &mut Resources) {
    let mut dir_light = DirectionalLight::default();
    let mut amb_light = AmbientLight::default();

    if let Some((light, _)) = world
        .query::<(&DirectionalLight, &ActiveDirectionalLight)>()
        .iter()
        .next()
    {
        dir_light = DirectionalLight {
            direction: light.direction,
            color: light.color,
            intensity: light.intensity,
        };
    }

    if let Some((light, _)) = world
        .query::<(&AmbientLight, &ActiveAmbientLight)>()
        .iter()
        .next()
    {
        amb_light = AmbientLight {
            color: light.color,
            intensity: light.intensity,
        };
    }

    res.light_uniform = LightUniform::new(&dir_light, &amb_light);
    res.gpu.queue.write_buffer(
        &res.light_buffer,
        0,
        bytemuck::cast_slice(&[res.light_uniform]),
    );
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
        let (model_uniform, material_uniform, tex) = {
            let entity_ref = world.entity(entity).unwrap();
            let t = entity_ref.get::<&Transform>().unwrap();
            let m = entity_ref.get::<&Material>().unwrap();
            let tex = m
                .texture
                .clone()
                .unwrap_or_else(|| res.fallback_texture.clone());
            (
                ModelUniform::from_transform(&t),
                MaterialUniform::from_material(&m),
                tex,
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

        let model_bind_group = res
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Model Bind Group"),
                layout: &res.bind_group_layouts.model,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_buffer.as_entire_binding(),
                }],
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
                label: Some("Material Bind Group"),
                layout: &res.bind_group_layouts.material,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&tex.sampler),
                    },
                ],
            });

        let render_state = RenderState {
            model_buffer,
            model_bind_group,
            material_buffer,
            material_bind_group,
        };

        world
            .insert(entity, (render_state, DirtyFlags::default()))
            .unwrap();
    }

    // Step 3: 全 Transform/Material の GPU バッファを更新
    // GlobalTransform がある場合はそちらを優先（親子階層対応）
    for (transform, material, render_state, global_transform) in world.query_mut::<(
        &Transform,
        &Material,
        &RenderState,
        Option<&crate::scene::hierarchy::GlobalTransform>,
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
}

/// レンダリングシステム: 描画可能なエンティティをすべて描画する
pub fn render_system(world: &World, res: &Resources) -> RenderResult {
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

    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = res
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.12,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &res.depth_texture.view,
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

        render_pass.set_pipeline(&res.pipeline);
        render_pass.set_bind_group(0, &res.camera_bind_group, &[]);

        // フラスタムカリング用: カメラのビュー射影行列からフラスタムを構築
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
            // 非表示エンティティをスキップ
            if let Some(vis) = visible
                && !vis.0
            {
                continue;
            }

            // フラスタムカリング: BoundingVolume を持つエンティティのみ判定
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

        // パーティクル描画（不透明オブジェクトの後）
        crate::particle::particle_render_system(&mut render_pass, res);
    }

    res.gpu.queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();

    RenderResult::Ok
}
