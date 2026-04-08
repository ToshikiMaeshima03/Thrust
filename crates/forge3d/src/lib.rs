#![allow(clippy::module_inception)]

pub mod animation;
pub mod app;
pub mod asset;
pub mod audio;
pub mod camera;
pub mod ecs;
pub mod event;
pub mod input;
pub mod light;
pub mod material;
pub mod math;
pub mod mesh;
pub mod particle;
pub mod physics;
pub mod renderer;
pub mod scene;
pub mod shader;
pub mod time;

// アプリケーション
pub use app::{ForgeAppHandler, run};

// ECS
pub use ecs::components::{
    ActiveAmbientLight, ActiveCamera, ActiveDirectionalLight, DirtyFlags, MeshHandle, Name,
    RenderState, Visible,
};
pub use ecs::resources::Resources;
pub use ecs::spawn::{despawn, spawn_child, spawn_cube, spawn_object, spawn_plane, spawn_sphere};
pub use hecs::{Entity, World};

// カメラ
pub use camera::camera::Camera;
pub use camera::controller::OrbitalController;

// 入力
pub use input::Input;
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

// ライティング
pub use light::light::{AmbientLight, DirectionalLight};

// レンダリング
pub use renderer::frustum::{BoundingVolume, Frustum};
pub use renderer::texture::ForgeTexture;

// マテリアル
pub use material::material::Material;

// 数学
pub use math::{Aabb, BoundingSphere};

// 物理
pub use physics::{
    Collider, ColliderShape, CollisionEvent, CollisionPair, Ray, RayHit, Velocity,
    collision_system, ray_cast, screen_to_ray, velocity_system,
};

// メッシュ
pub use mesh::mesh::Mesh;
pub use mesh::obj_loader::load_obj;
pub use mesh::primitives::{create_cube, create_plane, create_quad, create_sphere};

// シーン
pub use scene::hierarchy::{Children, GlobalTransform, Parent, set_parent};
pub use scene::transform::Transform;

// パーティクル
pub use particle::{Particle, ParticleEmitter, particle_system};

// アニメーション
pub use animation::{EaseFunction, TransformAnimation, animation_system, ease};

// オーディオ
pub use audio::{AudioManager, AudioSource, SoundHandle};

// アセット
pub use asset::AssetManager;

// イベント
pub use event::Events;

// タイム
pub use time::Time;
