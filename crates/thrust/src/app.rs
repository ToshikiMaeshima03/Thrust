use std::sync::Arc;

use hecs::World;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::camera::camera::Camera;
use crate::camera::controller::OrbitalController;
use crate::camera::uniform::CameraUniform;
use crate::config::EngineConfig;
use crate::ecs::components::{ActiveAmbientLight, ActiveCamera};
use crate::ecs::resources::Resources;
use crate::ecs::systems;
use crate::error::{ThrustError, ThrustResult};
use crate::light::light::{
    AmbientLight, DirectionalLight, GpuLight, LightsHeader, MAX_LIGHTS_TOTAL,
};
use crate::physics::PhysicsWorld;
use crate::renderer::buffer::create_uniform_buffer;
use crate::renderer::context::GpuContext;
use crate::renderer::decal::DecalRenderer;
use crate::renderer::fog::Fog;
use crate::renderer::ibl::IblEnvironment;
use crate::renderer::pipeline;
use crate::renderer::post::{
    BloomChain, ColorGrading, DepthOfField, HdrTargets, MotionBlur, PostComposite,
    PostProcessPipelines,
};
use crate::renderer::prepass::GeometryPrepass;
use crate::renderer::render_pass::{DepthTexture, RenderResult};
use crate::renderer::shadow::ShadowMap;
use crate::renderer::shadow_atlas::ShadowAtlas;
use crate::renderer::skybox::Skybox;
use crate::renderer::ssao::Ssao;
use crate::renderer::ssr::Ssr;
use crate::renderer::texture::ThrustTexture;
use crate::renderer::volumetric::VolumetricLight;
use crate::time::Time;
use wgpu::util::DeviceExt;

pub trait ThrustAppHandler {
    /// GPU 初期化後に一度呼ばれる。エンティティをここで生成する。
    fn init(&mut self, world: &mut World, res: &mut Resources);

    /// 毎フレーム、システム実行前に呼ばれる。
    #[allow(unused_variables)]
    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {}

    /// 毎フレーム、egui UI を描画する場所 (Round 4 後半)
    ///
    /// デフォルト実装は何もしない。HUD やデバッグパネルを表示するには
    /// このメソッドを実装する。例:
    /// ```ignore
    /// fn ui(&mut self, ctx: &egui::Context, world: &mut World, res: &mut Resources) {
    ///     egui::Window::new("デバッグ").show(ctx, |ui| {
    ///         ui.label(format!("FPS: {:.1}", res.debug_stats.fps));
    ///     });
    /// }
    /// ```
    #[allow(unused_variables)]
    fn ui(&mut self, ctx: &egui::Context, world: &mut World, res: &mut Resources) {}
}

struct ThrustAppInner {
    window: Option<Arc<Window>>,
    world: Option<World>,
    resources: Option<Resources>,
    handler: Box<dyn ThrustAppHandler>,
    config: EngineConfig,
    /// GPU 初期化やウィンドウ作成のエラーを保持（resumed() は () を返すため）
    error: Option<ThrustError>,
    /// Round 4 後半: egui コンテキストと renderer
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
}

impl ApplicationHandler for ThrustAppInner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let (w, h) = self.config.window_size;
        let attrs = WindowAttributes::default()
            .with_title(&self.config.window_title)
            .with_inner_size(PhysicalSize::new(w, h));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(ThrustError::WindowCreation(e));
                event_loop.exit();
                return;
            }
        };
        let gpu = match pollster::block_on(GpuContext::new(window.clone(), &self.config)) {
            Ok(g) => g,
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
                return;
            }
        };

        // Resources を構築
        let bind_group_layouts = pipeline::create_bind_group_layouts(&gpu.device);
        let render_pipeline =
            pipeline::create_render_pipeline(&gpu.device, gpu.config.format, &bind_group_layouts);
        let instanced_pipeline = crate::renderer::instancing::create_instanced_pipeline(
            &gpu.device,
            &bind_group_layouts,
        );

        let camera_uniform = CameraUniform::new();
        let camera_buffer = create_uniform_buffer(&gpu.device, &camera_uniform, "Camera Buffer");

        // ライト系初期化
        let lights_header = LightsHeader::new(&AmbientLight::default(), 0, 0, 0);
        let lights_header_buffer =
            create_uniform_buffer(&gpu.device, &lights_header, "Lights Header Buffer");

        let initial_lights: Vec<GpuLight> = vec![GpuLight::directional(
            glam::Vec3::ONE.normalize(),
            glam::Vec3::ZERO,
            0.0,
        )];
        let lights_storage_capacity = MAX_LIGHTS_TOTAL;
        let mut padded = initial_lights.clone();
        padded.resize(lights_storage_capacity, padded[0]);
        let lights_storage_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Lights Storage Buffer"),
                    contents: bytemuck::cast_slice(&padded),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        // シャドウマップを作成 (Round 4)
        let shadow_map = ShadowMap::new(&gpu.device, &bind_group_layouts.model);

        // HDR レンダーターゲット + Bloom チェーン + post-process パイプライン (Round 4 後半)
        let hdr_targets = HdrTargets::new(
            &gpu.device,
            gpu.size.width,
            gpu.size.height,
            gpu.config.format,
        );
        let bloom_chain = BloomChain::new(&gpu.device, gpu.size.width, gpu.size.height);
        let post = PostProcessPipelines::new(&gpu.device, gpu.config.format);
        let skybox = Skybox::new(&gpu.device, &gpu.queue, &camera_buffer);
        let ibl = IblEnvironment::new(&gpu.device, &gpu.queue);
        let fog = Fog::new(&gpu.device);

        // Round 7: Geometry prepass + screen-space effects
        let prepass = GeometryPrepass::new(
            &gpu.device,
            &bind_group_layouts,
            gpu.size.width,
            gpu.size.height,
        );
        let ssao = Ssao::new(
            &gpu.device,
            &camera_buffer,
            &prepass,
            gpu.size.width,
            gpu.size.height,
        );
        let ssr = Ssr::new(
            &gpu.device,
            &camera_buffer,
            &prepass,
            &hdr_targets.color_resolved_view,
            gpu.size.width,
            gpu.size.height,
        );
        let decal_renderer = DecalRenderer::new(&gpu.device, &bind_group_layouts.camera);
        let volumetric = VolumetricLight::new(
            &gpu.device,
            &camera_buffer,
            &prepass,
            &hdr_targets.color_resolved_view,
            gpu.size.width,
            gpu.size.height,
        );
        let shadow_atlas = ShadowAtlas::new(&gpu.device, &bind_group_layouts);
        let shadow_atlas_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Atlas Bind Group"),
            layout: &bind_group_layouts.shadow_atlas,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_atlas.cube_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_atlas.spot_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_atlas.sampler_cmp),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: shadow_atlas.uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let post_composite = PostComposite::new(&gpu.device, gpu.size.width, gpu.size.height);
        let dof = DepthOfField::new(
            &gpu.device,
            gpu.config.format,
            gpu.size.width,
            gpu.size.height,
        );
        let motion_blur = MotionBlur::new(
            &gpu.device,
            gpu.config.format,
            gpu.size.width,
            gpu.size.height,
        );
        let color_grading = ColorGrading::new(&gpu.device, gpu.config.format);

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &bind_group_layouts.camera,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights_header_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights_storage_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_map.array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&shadow_map.sampler_cmp),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: shadow_map.light_vp_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&ibl.irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&ibl.prefilter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&ibl.brdf_lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&ibl.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: fog.buffer.as_entire_binding(),
                },
            ],
        });

        let depth_texture = DepthTexture::new(&gpu.device, gpu.size.width, gpu.size.height);
        let fallback_texture = Arc::new(ThrustTexture::white_pixel(&gpu.device, &gpu.queue));
        let fallback_normal = Arc::new(ThrustTexture::flat_normal_pixel(&gpu.device, &gpu.queue));
        let fallback_mr = Arc::new(ThrustTexture::flat_mr_pixel(&gpu.device, &gpu.queue));

        let aspect = gpu.aspect_ratio();

        // パーティクルパイプライン作成
        let particle_texture_layout =
            crate::particle::create_particle_texture_bind_group_layout(&gpu.device);
        let particle_pipeline_untextured = crate::particle::create_particle_pipeline(
            &gpu.device,
            gpu.config.format,
            &bind_group_layouts.camera,
        );
        let particle_pipeline_textured = crate::particle::create_particle_textured_pipeline(
            &gpu.device,
            gpu.config.format,
            &bind_group_layouts.camera,
            &particle_texture_layout,
        );
        let particle_instance_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Particle Instance Buffer"),
                    contents: &[0u8; 32], // 最小バッファ（1パーティクル分）
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
        let particle_render_state = crate::particle::ParticleRenderState {
            pipeline_untextured: particle_pipeline_untextured,
            pipeline_textured: particle_pipeline_textured,
            texture_bind_group_layout: particle_texture_layout,
            instance_buffer: particle_instance_buffer,
            instance_buffer_capacity: 1,
            batches: Vec::new(),
            cached_instances: Vec::new(),
        };

        // オーディオ初期化（デバイス未検出時は None）
        let audio = crate::audio::AudioManager::new();

        // 物理ワールド (rapier3d)
        let physics = PhysicsWorld::new();

        let mut resources = Resources {
            gpu,
            time: Time::new(),
            input: crate::input::Input::new(),
            events: crate::event::Events::new(),
            assets: crate::asset::AssetManager::new(),
            audio,
            debug_stats: crate::debug::DebugStats::new(),
            bind_group_layouts,
            fallback_texture,
            fallback_normal,
            fallback_mr,
            physics,
            camera_uniform,
            camera_buffer,
            lights_header,
            lights_header_buffer,
            lights_storage_buffer,
            lights_storage_capacity,
            camera_bind_group,
            pipeline: render_pipeline,
            instanced_pipeline,
            depth_texture,
            shadow_map,
            hdr_targets,
            bloom_chain,
            post,
            skybox,
            ibl,
            fog,
            prepass,
            ssao,
            ssr,
            decal_renderer,
            volumetric,
            shadow_atlas,
            shadow_atlas_bind_group,
            post_composite,
            dof,
            motion_blur,
            color_grading,
            pending_surface: None,
            particle_render_state: Some(particle_render_state),
            clear_color: {
                let c = self.config.clear_color;
                wgpu::Color {
                    r: c[0] as f64,
                    g: c[1] as f64,
                    b: c[2] as f64,
                    a: c[3] as f64,
                }
            },
        };

        // World を構築し、デフォルトカメラ・ライトエンティティを生成
        let mut world = World::new();

        let camera = Camera::new(glam::Vec3::new(0.0, 1.0, 3.0), glam::Vec3::ZERO, aspect);
        let controller = OrbitalController::new(3.0, glam::Vec3::ZERO);
        world.spawn((camera, controller, ActiveCamera));

        // Round 4: マルチライト対応 - directional は単独で spawn (マーカー不要)
        world.spawn((DirectionalLight::default(),));
        world.spawn((AmbientLight::default(), ActiveAmbientLight));

        // egui 初期化 (Round 4 後半)
        let egui_renderer = egui_wgpu::Renderer::new(
            &resources.gpu.device,
            resources.gpu.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let viewport_id = self.egui_ctx.viewport_id();
        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            viewport_id,
            window.as_ref(),
            None,
            None,
            None,
        );

        // ユーザー初期化
        self.handler.init(&mut world, &mut resources);

        self.window = Some(window);
        self.world = Some(world);
        self.resources = Some(resources);
        self.egui_winit = Some(egui_winit);
        self.egui_renderer = Some(egui_renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let (Some(world), Some(res)) = (self.world.as_mut(), self.resources.as_mut()) else {
            return;
        };

        // egui に先にイベントを渡す (Round 4 後半)
        let egui_consumed =
            if let (Some(state), Some(window)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
                let response = state.on_window_event(window.as_ref(), &event);
                response.consumed
            } else {
                false
            };

        // 入力システム (egui に消費されたものは除く)
        if !egui_consumed {
            res.input.process_event(&event);
        }

        // 軌道カメラコントローラにイベントを転送
        let mut consumed = false;
        if !egui_consumed {
            for controller in world.query_mut::<&mut OrbitalController>() {
                if controller.process_event(&event) {
                    consumed = true;
                    break;
                }
            }
        }
        if consumed {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                res.gpu.resize(new_size);
                res.depth_texture =
                    DepthTexture::new(&res.gpu.device, new_size.width, new_size.height);

                // Round 4 後半: HDR + Bloom チェーンも再生成
                res.hdr_targets = HdrTargets::new(
                    &res.gpu.device,
                    new_size.width,
                    new_size.height,
                    res.gpu.config.format,
                );
                res.bloom_chain = BloomChain::new(&res.gpu.device, new_size.width, new_size.height);

                // Round 7: G-buffer prepass + 依存テクスチャ群を再生成
                res.prepass = GeometryPrepass::new(
                    &res.gpu.device,
                    &res.bind_group_layouts,
                    new_size.width,
                    new_size.height,
                );
                res.ssao = Ssao::new(
                    &res.gpu.device,
                    &res.camera_buffer,
                    &res.prepass,
                    new_size.width,
                    new_size.height,
                );
                res.ssr = Ssr::new(
                    &res.gpu.device,
                    &res.camera_buffer,
                    &res.prepass,
                    &res.hdr_targets.color_resolved_view,
                    new_size.width,
                    new_size.height,
                );
                res.volumetric = VolumetricLight::new(
                    &res.gpu.device,
                    &res.camera_buffer,
                    &res.prepass,
                    &res.hdr_targets.color_resolved_view,
                    new_size.width,
                    new_size.height,
                );
                res.post_composite =
                    PostComposite::new(&res.gpu.device, new_size.width, new_size.height);
                res.dof = DepthOfField::new(
                    &res.gpu.device,
                    res.gpu.config.format,
                    new_size.width,
                    new_size.height,
                );
                res.motion_blur = MotionBlur::new(
                    &res.gpu.device,
                    res.gpu.config.format,
                    new_size.width,
                    new_size.height,
                );

                // ActiveCamera のアスペクト比を更新
                let aspect = res.gpu.aspect_ratio();
                for (camera, _) in world.query_mut::<(&mut Camera, &ActiveCamera)>() {
                    camera.aspect = aspect;
                }
            }
            WindowEvent::RedrawRequested => {
                res.time.tick();
                let dt = res.time.delta();
                res.debug_stats.update(dt);

                // 1. ユーザーロジック
                self.handler.update(world, res, dt);

                // 2. エンジンシステム（固定順序）
                crate::animation::animation_system(world, dt);
                crate::animation::keyframe_animation_system(world, dt);
                crate::physics::velocity_system(world, dt);
                crate::physics::physics_init_system(world, &mut res.physics);
                crate::physics::joint_init_system(world, &mut res.physics);
                crate::physics::physics_step_system(&mut res.physics, dt);
                crate::physics::physics_sync_from_system(world, &res.physics);
                crate::physics::character_controller_system(world, &mut res.physics, dt);
                crate::particle::particle_system(world, dt);
                systems::camera_system(world, res);
                systems::light_system(world, res);
                crate::scene::hierarchy::propagate_transforms(world);
                crate::animation::ik_system(world);
                crate::animation::skin_system(world);
                crate::animation::morph_system(world, res);
                crate::physics::collision_system(world, &mut res.events);
                crate::particle::particle_render_prep_system(world, res);
                systems::render_prep_system(world, res);
                crate::animation::skin_upload_system(world, res);

                // egui UI フレームを実行 (Round 4 後半)
                let egui_paint = if let (Some(state), Some(window)) =
                    (self.egui_winit.as_mut(), self.window.as_ref())
                {
                    let raw_input = state.take_egui_input(window.as_ref());
                    let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
                        self.handler.ui(ctx, world, res);
                    });
                    state.handle_platform_output(window.as_ref(), full_output.platform_output);
                    let ppp = full_output.pixels_per_point;
                    let paint_jobs = self.egui_ctx.tessellate(full_output.shapes, ppp);
                    Some((paint_jobs, full_output.textures_delta, ppp))
                } else {
                    None
                };

                // 3. 描画 (egui がある場合は present 保留)
                let needs_egui = egui_paint
                    .as_ref()
                    .map(|(jobs, _, _)| !jobs.is_empty())
                    .unwrap_or(false);
                match systems::render_system(world, res, !needs_egui) {
                    RenderResult::Ok => {}
                    RenderResult::SurfaceLost => {
                        let size = res.gpu.size;
                        res.gpu.resize(size);
                        res.depth_texture =
                            DepthTexture::new(&res.gpu.device, size.width, size.height);
                    }
                    RenderResult::Error => {
                        log::error!("致命的レンダリングエラー");
                        event_loop.exit();
                    }
                }

                // 4. egui 描画 (Round 4 後半)
                if let (Some(paint), Some(renderer), Some(surface)) = (
                    egui_paint,
                    self.egui_renderer.as_mut(),
                    res.pending_surface.take(),
                ) {
                    let (paint_jobs, textures_delta, ppp) = paint;
                    let view = surface
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    for (id, image_delta) in &textures_delta.set {
                        renderer.update_texture(&res.gpu.device, &res.gpu.queue, *id, image_delta);
                    }

                    let screen_descriptor = egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [res.gpu.size.width, res.gpu.size.height],
                        pixels_per_point: ppp,
                    };

                    let mut encoder =
                        res.gpu
                            .device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("egui Encoder"),
                            });
                    renderer.update_buffers(
                        &res.gpu.device,
                        &res.gpu.queue,
                        &mut encoder,
                        &paint_jobs,
                        &screen_descriptor,
                    );
                    {
                        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("egui Render Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
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
                        renderer.render(
                            &mut pass.forget_lifetime(),
                            &paint_jobs,
                            &screen_descriptor,
                        );
                    }
                    res.gpu.queue.submit(std::iter::once(encoder.finish()));
                    surface.present();

                    for id in &textures_delta.free {
                        renderer.free_texture(id);
                    }
                }

                res.input.begin_frame();
                res.events.clear();
                if let Some(audio) = &mut res.audio {
                    crate::audio::audio_listener_system(world, audio);
                    crate::audio::audio_emitter_system(world, audio);
                    audio.cleanup_finished();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// デフォルト設定でエンジンを起動する
pub fn run(handler: impl ThrustAppHandler + 'static) -> ThrustResult<()> {
    run_with_config(handler, EngineConfig::default())
}

/// 指定した設定でエンジンを起動する
pub fn run_with_config(
    handler: impl ThrustAppHandler + 'static,
    config: EngineConfig,
) -> ThrustResult<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = ThrustAppInner {
        window: None,
        world: None,
        resources: None,
        handler: Box::new(handler),
        config,
        error: None,
        egui_ctx: egui::Context::default(),
        egui_winit: None,
        egui_renderer: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.error {
        return Err(err);
    }

    Ok(())
}
