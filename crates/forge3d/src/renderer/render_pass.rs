use crate::scene::scene::Scene;

pub struct DepthTexture {
    pub view: wgpu::TextureView,
}

impl DepthTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { view }
    }
}

/// フレーム描画を実行。成功時はtrue、リサイズが必要な場合はfalse
pub fn render_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface: &wgpu::Surface,
    pipeline: &wgpu::RenderPipeline,
    depth_texture: &DepthTexture,
    camera_bind_group: &wgpu::BindGroup,
    scene: &Scene,
) -> RenderResult {
    let output = surface.get_current_texture();

    let surface_texture = match output {
        wgpu::CurrentSurfaceTexture::Success(tex) => tex,
        wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
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

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                view: &depth_texture.view,
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

        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);

        for obj in &scene.objects {
            render_pass.set_bind_group(1, &obj.model_bind_group, &[]);
            render_pass.set_vertex_buffer(0, obj.mesh.vertex_buffer.slice(..));
            render_pass
                .set_index_buffer(obj.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..obj.mesh.num_indices, 0, 0..1);
        }
    }

    queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();

    RenderResult::Ok
}

pub enum RenderResult {
    Ok,
    SurfaceLost,
    Error,
}
