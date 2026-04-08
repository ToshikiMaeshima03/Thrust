use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::camera::camera::Camera;
use crate::camera::controller::OrbitalController;
use crate::camera::uniform::CameraUniform;
use crate::renderer::buffer::create_uniform_buffer;
use crate::renderer::context::GpuContext;
use crate::renderer::pipeline::{self, ForgeBindGroupLayouts};
use crate::renderer::render_pass::{self, DepthTexture, RenderResult};
use crate::scene::scene::Scene;

pub struct AppContext {
    pub gpu: GpuContext,
    pub camera: Camera,
    pub controller: OrbitalController,
    pub scene: Scene,
    pub bind_group_layouts: ForgeBindGroupLayouts,

    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    depth_texture: DepthTexture,
}

impl AppContext {
    fn new(gpu: GpuContext) -> Self {
        let bind_group_layouts = pipeline::create_bind_group_layouts(&gpu.device);

        let render_pipeline =
            pipeline::create_render_pipeline(&gpu.device, gpu.config.format, &bind_group_layouts);

        let mut camera = Camera::new(
            glam::Vec3::new(0.0, 1.0, 3.0),
            glam::Vec3::ZERO,
            gpu.aspect_ratio(),
        );

        let controller = OrbitalController::new(3.0, glam::Vec3::ZERO);

        let mut camera_uniform = CameraUniform::new();
        controller.update_camera(&mut camera);
        camera_uniform.update(&camera);

        let camera_buffer = create_uniform_buffer(&gpu.device, &camera_uniform, "Camera Buffer");

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &bind_group_layouts.camera,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let depth_texture = DepthTexture::new(&gpu.device, gpu.size.width, gpu.size.height);

        Self {
            gpu,
            camera,
            controller,
            scene: Scene::new(),
            bind_group_layouts,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            pipeline: render_pipeline,
            depth_texture,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.gpu.resize(new_size);
        self.camera.aspect = self.gpu.aspect_ratio();
        self.depth_texture =
            DepthTexture::new(&self.gpu.device, new_size.width, new_size.height);
    }

    fn update(&mut self) {
        self.controller.update_camera(&mut self.camera);
        self.camera_uniform.update(&self.camera);
        self.gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
        self.scene.update_transforms(&self.gpu.queue);
    }

    fn render(&self) -> RenderResult {
        render_pass::render_frame(
            &self.gpu.device,
            &self.gpu.queue,
            &self.gpu.surface,
            &self.pipeline,
            &self.depth_texture,
            &self.camera_bind_group,
            &self.scene,
        )
    }
}

pub trait ForgeAppHandler {
    fn init(&mut self, ctx: &mut AppContext);
}

struct ForgeAppInner {
    window: Option<Arc<Window>>,
    ctx: Option<AppContext>,
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
        let mut ctx = AppContext::new(gpu);

        self.handler.init(&mut ctx);

        self.window = Some(window);
        self.ctx = Some(ctx);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(ctx) = self.ctx.as_mut() else {
            return;
        };

        if ctx.controller.process_event(&event) {
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
                ctx.resize(new_size);
            }
            WindowEvent::RedrawRequested => {
                ctx.update();
                match ctx.render() {
                    RenderResult::Ok => {}
                    RenderResult::SurfaceLost => {
                        let size = ctx.gpu.size;
                        ctx.resize(size);
                    }
                    RenderResult::Error => {
                        log::error!("致命的レンダリングエラー");
                        event_loop.exit();
                    }
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
        ctx: None,
        handler: Box::new(handler),
    };

    event_loop.run_app(&mut app).expect("EventLoop 実行失敗");
}
