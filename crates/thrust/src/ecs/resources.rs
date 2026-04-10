use std::sync::Arc;

use crate::asset::AssetManager;
use crate::audio::AudioManager;
use crate::camera::uniform::CameraUniform;
use crate::debug::DebugStats;
use crate::event::Events;
use crate::input::Input;
use crate::light::light::LightsHeader;
use crate::particle::ParticleRenderState;
use crate::physics::PhysicsWorld;
use crate::renderer::context::GpuContext;
use crate::renderer::decal::DecalRenderer;
use crate::renderer::fog::Fog;
use crate::renderer::ibl::IblEnvironment;
use crate::renderer::pipeline::ThrustBindGroupLayouts;
use crate::renderer::post::{
    BloomChain, ColorGrading, DepthOfField, HdrTargets, MotionBlur, PostComposite,
    PostProcessPipelines,
};
use crate::renderer::prepass::GeometryPrepass;
use crate::renderer::render_pass::DepthTexture;
use crate::renderer::shadow::ShadowMap;
use crate::renderer::shadow_atlas::ShadowAtlas;
use crate::renderer::skybox::Skybox;
use crate::renderer::ssao::Ssao;
use crate::renderer::ssr::Ssr;
use crate::renderer::texture::ThrustTexture;
use crate::renderer::volumetric::VolumetricLight;
use crate::time::Time;

/// エンティティに属さないグローバルリソース
pub struct Resources {
    pub gpu: GpuContext,
    pub time: Time,
    pub input: Input,
    pub events: Events,
    pub assets: AssetManager,
    pub audio: Option<AudioManager>,
    pub debug_stats: DebugStats,
    pub bind_group_layouts: ThrustBindGroupLayouts,
    pub fallback_texture: Arc<ThrustTexture>,
    /// ノーマルマップ用のフラットフォールバック (RGB = 128,128,255)
    pub fallback_normal: Arc<ThrustTexture>,
    /// MR/AO 用のフラットフォールバック (RGB = 0,255,0 → roughness=1, metallic=0)
    pub fallback_mr: Arc<ThrustTexture>,
    /// 物理ワールド (rapier3d)
    pub physics: PhysicsWorld,

    pub(crate) camera_uniform: CameraUniform,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) lights_header: LightsHeader,
    pub(crate) lights_header_buffer: wgpu::Buffer,
    pub(crate) lights_storage_buffer: wgpu::Buffer,
    pub(crate) lights_storage_capacity: usize,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) pipeline: wgpu::RenderPipeline,
    /// Round 5: GPU インスタンシング用パイプライン
    pub(crate) instanced_pipeline: wgpu::RenderPipeline,
    pub(crate) depth_texture: DepthTexture,
    pub(crate) shadow_map: ShadowMap,
    /// Round 4 後半: HDR + MSAA レンダーターゲット
    pub(crate) hdr_targets: HdrTargets,
    /// Round 4 後半: Bloom チェーン
    pub(crate) bloom_chain: BloomChain,
    /// Round 4 後半: ポストプロセスパイプライン (bloom + tonemap + FXAA)
    pub(crate) post: PostProcessPipelines,
    /// Round 4 後半: Skybox (プロシージャル sky / cubemap)
    pub skybox: Skybox,
    /// Round 4 後半: IBL 環境 (irradiance + prefilter + BRDF LUT)
    pub(crate) ibl: IblEnvironment,
    /// Round 5: ボリュメトリックフォグ
    pub fog: Fog,

    // Round 7: スクリーン空間エフェクト基盤
    /// Geometry G-buffer prepass (depth + normal + material + motion)
    pub(crate) prepass: GeometryPrepass,
    /// SSAO リソース
    pub(crate) ssao: Ssao,
    /// SSR リソース
    pub(crate) ssr: Ssr,
    /// デカール描画リソース
    pub(crate) decal_renderer: DecalRenderer,
    /// ボリュメトリックライト (god rays)
    pub(crate) volumetric: VolumetricLight,
    /// 点光源/スポットライトシャドウアトラス
    pub(crate) shadow_atlas: ShadowAtlas,
    /// PBR シェーダーで使うシャドウアトラスバインドグループ (group 3)
    pub(crate) shadow_atlas_bind_group: wgpu::BindGroup,
    /// HDR ポストエフェクト合成 (SSAO + SSR + Volumetric)
    pub(crate) post_composite: PostComposite,
    /// 被写界深度
    pub(crate) dof: DepthOfField,
    /// モーションブラー
    pub(crate) motion_blur: MotionBlur,
    /// カラーグレーディング + ヴィネット
    pub(crate) color_grading: ColorGrading,

    /// Round 4 後半: render_system が一時的に保留する surface texture (egui 用)
    pub(crate) pending_surface: Option<wgpu::SurfaceTexture>,
    pub(crate) particle_render_state: Option<ParticleRenderState>,
    pub(crate) clear_color: wgpu::Color,
}
