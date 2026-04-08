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
use crate::ecs::components::{ActiveAmbientLight, ActiveCamera, ActiveDirectionalLight};
use crate::ecs::resources::Resources;
use crate::ecs::systems;
use crate::light::light::{AmbientLight, DirectionalLight, LightUniform};
use crate::renderer::buffer::create_uniform_buffer;
use crate::renderer::context::GpuContext;
use crate::renderer::pipeline;
use crate::renderer::render_pass::{DepthTexture, RenderResult};
use crate::renderer::texture::ForgeTexture;
use crate::time::Time;
use wgpu::util::DeviceExt;

pub trait ForgeAppHandler {
    /// GPU 初期化後に一度呼ばれる。エンティティをここで生成する。
    fn init(&mut self, world: &mut World, res: &mut Resources);

    /// 毎フレーム、システム実行前に呼ばれる。
    #[allow(unused_variables)]
    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {}
}

struct ForgeAppInner {
    window: Option<Arc<Window>>,
    world: Option<World>,
    resources: Option<Resources>,
    handler: Box<dyn ForgeAppHandler>,
}

impl ApplicationHandler for ForgeAppInner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("forge3d")
            .with_inner_size(PhysicalSize::new(1280u32, 720));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let gpu = pollster::block_on(GpuContext::new(window.clone()));

        // Resources を構築
        let bind_group_layouts = pipeline::create_bind_group_layouts(&gpu.device);
        let render_pipeline =
            pipeline::create_render_pipeline(&gpu.device, gpu.config.format, &bind_group_layouts);

        let camera_uniform = CameraUniform::new();
        let camera_buffer = create_uniform_buffer(&gpu.device, &camera_uniform, "Camera Buffer");

        let directional_light = DirectionalLight::default();
        let ambient_light = AmbientLight::default();
        let light_uniform = LightUniform::new(&directional_light, &ambient_light);
        let light_buffer = create_uniform_buffer(&gpu.device, &light_uniform, "Light Buffer");

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
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });

        let depth_texture = DepthTexture::new(&gpu.device, gpu.size.width, gpu.size.height);
        let fallback_texture = Arc::new(ForgeTexture::white_pixel(&gpu.device, &gpu.queue));

        let aspect = gpu.aspect_ratio();

        // パーティクルパイプライン作成
        let particle_pipeline = crate::particle::create_particle_pipeline(
            &gpu.device,
            gpu.config.format,
            &bind_group_layouts.camera,
        );
        let particle_instance_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Particle Instance Buffer"),
                    contents: &[0u8; 32], // 最小バッファ（1パーティクル分）
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
        let particle_render_state = crate::particle::ParticleRenderState {
            pipeline: particle_pipeline,
            instance_buffer: particle_instance_buffer,
            instance_count: 0,
            instance_buffer_capacity: 1,
        };

        // オーディオ初期化（デバイス未検出時は None）
        let audio = crate::audio::AudioManager::new();

        let mut resources = Resources {
            gpu,
            time: Time::new(),
            input: crate::input::Input::new(),
            events: crate::event::Events::new(),
            assets: crate::asset::AssetManager::new(),
            audio,
            bind_group_layouts,
            fallback_texture,
            camera_uniform,
            camera_buffer,
            light_uniform,
            light_buffer,
            camera_bind_group,
            pipeline: render_pipeline,
            depth_texture,
            particle_render_state: Some(particle_render_state),
        };

        // World を構築し、デフォルトカメラ・ライトエンティティを生成
        let mut world = World::new();

        let camera = Camera::new(glam::Vec3::new(0.0, 1.0, 3.0), glam::Vec3::ZERO, aspect);
        let controller = OrbitalController::new(3.0, glam::Vec3::ZERO);
        world.spawn((camera, controller, ActiveCamera));

        world.spawn((directional_light, ActiveDirectionalLight));
        world.spawn((ambient_light, ActiveAmbientLight));

        // ユーザー初期化
        self.handler.init(&mut world, &mut resources);

        self.window = Some(window);
        self.world = Some(world);
        self.resources = Some(resources);
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

        // 入力システムが全イベントを先に取得
        res.input.process_event(&event);

        // 軌道カメラコントローラにイベントを転送
        let mut consumed = false;
        for controller in world.query_mut::<&mut OrbitalController>() {
            if controller.process_event(&event) {
                consumed = true;
                break;
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

                // ActiveCamera のアスペクト比を更新
                let aspect = res.gpu.aspect_ratio();
                for (camera, _) in world.query_mut::<(&mut Camera, &ActiveCamera)>() {
                    camera.aspect = aspect;
                }
            }
            WindowEvent::RedrawRequested => {
                res.time.tick();
                let dt = res.time.delta();

                // 1. ユーザーロジック
                self.handler.update(world, res, dt);

                // 2. エンジンシステム（固定順序）
                crate::animation::animation_system(world, dt);
                crate::physics::velocity_system(world, dt);
                crate::particle::particle_system(world, dt);
                systems::camera_system(world, res);
                systems::light_system(world, res);
                crate::scene::hierarchy::propagate_transforms(world);
                crate::physics::collision_system(world, &mut res.events);
                crate::particle::particle_render_prep_system(world, res);
                systems::render_prep_system(world, res);

                // 3. 描画
                match systems::render_system(world, res) {
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

                res.input.begin_frame();
                res.events.clear();
                if let Some(audio) = &mut res.audio {
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

pub fn run(handler: impl ForgeAppHandler + 'static) {
    let event_loop = EventLoop::new().expect("EventLoop 作成失敗");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = ForgeAppInner {
        window: None,
        world: None,
        resources: None,
        handler: Box::new(handler),
    };

    event_loop.run_app(&mut app).expect("EventLoop 実行失敗");
}
