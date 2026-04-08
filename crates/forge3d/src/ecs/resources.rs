use std::sync::Arc;

use crate::asset::AssetManager;
use crate::audio::AudioManager;
use crate::camera::uniform::CameraUniform;
use crate::event::Events;
use crate::input::Input;
use crate::light::light::LightUniform;
use crate::particle::ParticleRenderState;
use crate::renderer::context::GpuContext;
use crate::renderer::pipeline::ForgeBindGroupLayouts;
use crate::renderer::render_pass::DepthTexture;
use crate::renderer::texture::ForgeTexture;
use crate::time::Time;

/// エンティティに属さないグローバルリソース
pub struct Resources {
    pub gpu: GpuContext,
    pub time: Time,
    pub input: Input,
    pub events: Events,
    pub assets: AssetManager,
    pub audio: Option<AudioManager>,
    pub bind_group_layouts: ForgeBindGroupLayouts,
    pub fallback_texture: Arc<ForgeTexture>,

    pub(crate) camera_uniform: CameraUniform,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) light_uniform: LightUniform,
    pub(crate) light_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) depth_texture: DepthTexture,
    pub(crate) particle_render_state: Option<ParticleRenderState>,
}
