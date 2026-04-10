#![allow(clippy::module_inception)]

pub mod ai;
pub mod animation;
pub mod app;
pub mod asset;
pub mod audio;
pub mod camera;
pub mod config;
pub mod debug;
pub mod ecs;
pub mod editor;
pub mod error;
pub mod event;
pub mod input;
pub mod input_action;
pub mod jobs;
pub mod light;
pub mod material;
pub mod math;
pub mod mesh;
pub mod particle;
pub mod physics;
pub mod renderer;
pub mod save;
pub mod scene;
pub mod scripting;
pub mod shader;
pub mod time;

// エラー
pub use error::{ThrustError, ThrustResult};

// 設定
pub use config::EngineConfig;

// アプリケーション
pub use app::{ThrustAppHandler, run, run_with_config};

// ECS
pub use ecs::components::{
    ActiveAmbientLight, ActiveCamera, ActiveDirectionalLight, MeshHandle, Name, RenderState,
    Visible,
};
pub use ecs::resources::Resources;
pub use ecs::spawn::{
    despawn, spawn_child, spawn_cube, spawn_model, spawn_object, spawn_plane, spawn_sphere,
};
pub use hecs::{Entity, World};

// カメラ
pub use camera::camera::Camera;
pub use camera::controller::OrbitalController;

// 入力
pub use input::Input;
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

// ライティング (Round 4: マルチライト対応)
pub use light::light::{
    AmbientLight, DirectionalLight, GpuLight, LightsHeader, MAX_DIR_LIGHTS, MAX_LIGHTS_TOTAL,
    MAX_POINT_LIGHTS, MAX_SPOT_LIGHTS, PointLight, SpotLight,
};

// レンダリング
pub use renderer::frustum::{BoundingVolume, Frustum};
pub use renderer::texture::ThrustTexture;

// マテリアル (Round 4: PBR)
pub use material::material::{Material, MaterialUniform};

// 数学
pub use math::{
    Aabb,
    BoundingSphere,
    SimpleRng,
    // ジオメトリ
    barycentric_coords,
    closest_point_on_line_segment,
    // 行列
    decompose,
    // 角度
    deg_to_rad,
    extract_max_scale,
    extract_scale,
    extract_translation,
    // 数値
    inverse_lerp,
    // クォータニオン
    look_rotation,
    move_towards,
    nearly_equal,
    normalize_angle,
    point_to_line_distance,
    rad_to_deg,
    ray_plane_intersection,
    ray_triangle_intersection,
    remap,
    signed_angle,
    smootherstep,
    smoothstep,
    spherical_to_cartesian,
    triangle_area,
    triangle_normal,
    wrap,
};

// 物理 (Round 4: rapier3d 統合 + Round 7: joints, character controller)
pub use physics::{
    CharacterController, Collider, ColliderShape, CollisionEvent, CollisionPair, JointDescriptor,
    JointHandle, JointKind, PhysicsHandle, PhysicsWorld, Ray, RayHit, RigidBody, RigidBodyType,
    Velocity, character_controller_system, collision_system, joint_init_system,
    physics_init_system, physics_step_system, physics_sync_from_system, ray_cast, screen_to_ray,
    velocity_system,
};

// メッシュ
pub use mesh::gltf_loader::{GltfAnimationData, GltfLoadResult, load_gltf};
pub use mesh::mesh::Mesh;
pub use mesh::model_loader::{ModelLoadResult, load_model};
pub use mesh::obj_loader::load_obj;
pub use mesh::primitives::{create_cube, create_plane, create_quad, create_sphere};
pub use mesh::stl_loader::load_stl;

// シーン
pub use scene::hierarchy::{Children, GlobalTransform, Parent, set_parent};
pub use scene::transform::Transform;

// パーティクル
pub use particle::{Particle, ParticleEmitter, particle_system};

// アニメーション (Round 4: スケルタルアニメ、Round 6: ステートマシン、Round 7: IK + morph)
pub use animation::{
    AnimationStateMachine, BlendTree1D, Condition as AnimCondition, EaseFunction, IkResult, Joint,
    KeyframeAnimation, KeyframeTrack, KeyframeValues, MorphController, MorphTarget, ParamValue,
    Skeleton, SkinnedMesh, TransformAnimation, TwoBoneIk, animation_system, ease, ik_system,
    keyframe_animation_system, morph_system, skin_system, skin_upload_system, solve_two_bone_ik,
    state_machine_system,
};

// AI / Navmesh (Round 5) + Behavior Tree (Round 6)
pub use ai::{
    AgentMover, BehaviorTree, Blackboard, BtContext, BtNode, NavCell, NavMesh, NavMeshBuilder,
    Status as BtStatus, action as bt_action, agent_movement_system, behavior_tree_system,
    condition as bt_condition, find_path, selector as bt_selector, sequence as bt_sequence,
    smooth_path,
};

// シーンシリアライゼーション (Round 5)
pub use scene::serialize::SerScene;

// インスタンシング (Round 5)
pub use renderer::instancing::{InstanceData, InstancedMesh};

// フォグ (Round 5)
pub use renderer::fog::FogUniform;

// Round 6: 地形
pub use mesh::terrain::{create_terrain_from_heightmap, noise_heightmap, sine_heightmap};

// Round 6: LOD
pub use mesh::lod::{LodLevel, MeshLod, lod_system};

// Round 6: マテリアルインスタンス
pub use material::instance::{MaterialInstance, MaterialTemplate};

// Round 6: トリガーボリューム
pub use physics::{TriggerEnter, TriggerExit, TriggerStay, TriggerVolume, trigger_system};

// Round 6: SSAO
pub use renderer::ssao::{Ssao, SsaoUniform};

// Round 7: スクリーン空間エフェクト + ポストエフェクト + シャドウアトラス
pub use renderer::decal::{Decal, DecalRenderer, DecalUniform};
pub use renderer::post::{
    ColorGrading, ColorGradingUniform, DepthOfField, DofUniform, MotionBlur, MotionBlurUniform,
    PostComposite, PostCompositeUniform,
};
pub use renderer::prepass::GeometryPrepass;
pub use renderer::shadow_atlas::{
    MAX_POINT_SHADOWS, MAX_SPOT_SHADOWS, ShadowAtlas, ShadowAtlasUniform,
};
pub use renderer::ssr::{Ssr, SsrUniform};
pub use renderer::volumetric::{VolumetricLight, VolumetricUniform};

// Round 7: スクリプティング (Rhai)
pub use scripting::{ScriptEngine, SharedScript};

// Round 8: 拡張 PBR + 追加レンダリング機能
pub use renderer::auto_exposure::{AutoExposure, ExposureUniform};
pub use renderer::clouds::{CloudUniform, VolumetricClouds};
pub use renderer::foliage::{
    FoliagePatch, FoliageRenderer, FoliageUniform, grid_foliage_instances,
};
pub use renderer::gpu_particles::{GpuParticle, GpuParticleSimParams, GpuParticleSystem};
pub use renderer::lens_flare::{FlareGhost, LensFlareInstance, LensFlareSource, default_ghosts};
pub use renderer::reflection_probe::{
    ReflectionProbe, ReflectionProbeUniform, cube_face_views, init_probe_resources,
};
pub use renderer::taa::{Taa, TaaUniform};
pub use renderer::trail::{TrailPoint, TrailRenderer, TrailVertex, trail_sample_system};
pub use renderer::water::{Water, WaterRenderer, WaterUniform};

// Round 8: 物理拡張
pub use physics::{
    Cloth, ClothConstraint, ClothNode, RagdollBone, RagdollBuilder, RagdollDimensions, Vehicle,
    cloth_system, vehicle_system,
};

// Round 8: 入力アクションマップ + セーブ/ロード + ジョブ
pub use input_action::{ActionBinding, AxisBinding, InputActionMap};
pub use jobs::{num_threads as job_num_threads, parallel_for, parallel_map, parallel_range};
pub use save::SaveData;

// Round 8: スプライン
pub use math::{CatmullRomSpline, CubicBezier};

// Round 8: シーンストリーミング
pub use scene::streaming::{ChunkCoord, StreamingWorld};

// Round 9: ゲーム内エディタ
pub use editor::{Editor, GizmoMode, TransformGizmo};

// オーディオ (Round 4: kira ベース、空間音響対応)
pub use audio::{
    AudioEmitter, AudioListener, AudioManager, AudioSource, SoundHandle, audio_emitter_system,
    audio_listener_system,
};

// アセット
pub use asset::AssetManager;

// イベント
pub use event::Events;

// タイム
pub use time::Time;

// デバッグ
pub use debug::DebugStats;

// egui の再エクスポート (UI フック用、Round 4 後半)
pub use egui;
