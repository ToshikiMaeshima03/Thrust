# Forge3D - Claude Code 開発ガイド

## プロジェクト概要
wgpu ベースのクロスプラットフォーム 3D ゲームエンジン。ECS (Entity Component System) アーキテクチャ採用。

## ワークスペース構成
- `crates/forge3d` — コアエンジンライブラリ
- `examples/obj_viewer` — OBJ ビューアサンプル
- `examples/primitives_demo` — プリミティブ・入力・ライティングデモ
- `assets/models/` — 3D モデルファイル

## ビルドコマンド
```bash
cargo check                    # 型チェック
cargo build                    # ビルド
cargo test                     # テスト実行
cargo fmt                      # フォーマット
cargo clippy -- -D warnings    # Lint（警告をエラーとして扱う）
```

## コーディング規約
- Rust edition 2024, MSRV 1.94.0
- エラーメッセージ・ログは日本語
- `cargo fmt` と `cargo clippy -- -D warnings` がクリーンであること
- `#![allow(clippy::module_inception)]` をクレートルートで許可済み
- GPU 互換の構造体には `#[repr(C)]` + `Pod`/`Zeroable` を使用
- WGSL の vec3 フィールドは 16 バイトアライメント（[f32; 3] + f32 パディング）

## アーキテクチャ (ECS)

### コア概念
- **World** (`hecs::World`): 全エンティティとコンポーネントを保持
- **Entity** (`hecs::Entity`): エンティティハンドル（Copy + Eq + Hash）
- **Resources**: エンティティに属さないグローバルシングルトン（GPU, Time, Input, Audio 等）
- **Systems**: World + Resources を受け取って処理する関数

### ForgeAppHandler トレイト
```rust
pub trait ForgeAppHandler {
    fn init(&mut self, world: &mut World, res: &mut Resources);
    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {}
}
```

### フレームループ（システム実行順序）
```
handler.update()           — ユーザーロジック
animation_system()         — Tween アニメーション
velocity_system()          — 速度→Transform 適用
particle_system()          — パーティクル更新（生成・移動・消滅）
camera_system()            — カメラ更新
light_system()             — ライト更新
propagate_transforms()     — 親子階層の GlobalTransform 伝播
collision_system()         — AABB コリジョン検出
particle_render_prep()     — パーティクルインスタンスバッファ更新
render_prep_system()       — GPU バッファ作成・更新
render_system()            — 描画（フラスタムカリング + パーティクル描画）
audio.cleanup_finished()   — 完了サウンドのクリーンアップ
```

### モジュール構成
```
app.rs              — イベントループ、ウィンドウ管理、ForgeAppHandler
ecs/
  components.rs     — MeshHandle, RenderState, DirtyFlags, Visible, Name, マーカー
  resources.rs      — Resources (GPU, Time, Input, Events, Assets, Audio, パイプライン)
  systems.rs        — camera/light/render_prep/render システム
  spawn.rs          — spawn_object, spawn_cube, spawn_sphere, spawn_plane, spawn_child, despawn
animation/          — TransformAnimation, EaseFunction, animation_system
asset/              — AssetManager (テクスチャ・メッシュ・音声キャッシュ)
audio/              — AudioManager, AudioSource, SoundHandle (rodio ラッパー)
camera/             — Camera, OrbitalController, CameraUniform
event/              — Events (型消去イベントキュー)
input.rs            — キーボード・マウス入力状態管理
light/              — DirectionalLight, AmbientLight, LightUniform
material/           — Material (色+テクスチャ), MaterialUniform
math/               — Aabb, BoundingSphere
mesh/               — Vertex, Mesh, OBJ ローダー, プリミティブ生成
particle/
  emitter.rs        — Particle, ParticleEmitter, particle_system
  render.rs         — ParticleInstance, ParticleRenderState, インスタンス描画
physics/
  collider.rs       — Collider, ColliderShape, Velocity, collision_system
  ray.rs            — Ray, RayHit, ray_cast, screen_to_ray
renderer/
  context.rs        — GpuContext (device, queue, surface)
  frustum.rs        — Frustum, BoundingVolume (フラスタムカリング)
  pipeline.rs       — ForgeBindGroupLayouts, RenderPipeline
  render_pass.rs    — DepthTexture, RenderResult
  texture.rs        — ForgeTexture (from_bytes, from_path, white_pixel)
scene/
  hierarchy.rs      — Parent, Children, GlobalTransform, propagate_transforms
  transform.rs      — Transform (translation, rotation, scale)
  scene.rs          — ModelUniform (GPU 用)
shader/
  default.wgsl      — メインシェーダー (Phong ライティング)
  particle.wgsl     — パーティクルシェーダー (ビルボード + 円形フェード)
time.rs             — デルタタイム、フレームカウント
```

## ECS コンポーネント一覧

### 描画関連
- `Transform` — ローカル変換 (translation, rotation, scale)
- `GlobalTransform` — ワールド空間変換（階層伝播済み）
- `MeshHandle(Mesh)` — GPU メッシュ参照
- `Material` — 色 + テクスチャ
- `RenderState` — GPU バッファ・バインドグループ（エンジン管理）
- `Visible(bool)` — 表示/非表示
- `BoundingVolume(Aabb)` — フラスタムカリング用

### マーカー
- `ActiveCamera` — アクティブカメラ
- `ActiveDirectionalLight` — アクティブ平行光源
- `ActiveAmbientLight` — アクティブ環境光

### 階層
- `Parent(Entity)` — 親エンティティ
- `Children(Vec<Entity>)` — 子エンティティリスト

### 物理
- `Collider { shape, is_trigger }` — コリジョン形状
- `Velocity { linear }` — 速度

### パーティクル
- `ParticleEmitter` — パーティクルエミッター（emission_rate, lifetime, velocity, color, gravity 等）

### アニメーション
- `TransformAnimation` — Transform 補間 (lerp/slerp)

## レイキャスト
- `Ray { origin, direction }` — レイ（スラブ法 AABB / 二次方程式 Sphere 交差判定）
- `RayHit { entity, distance, point }` — ヒット結果
- `ray_cast(world, ray, max_distance)` — World 内 Collider 全検索（距離順ソート）
- `screen_to_ray(x, y, w, h, camera)` — スクリーン座標→ワールドレイ変換

## パーティクルシステム
- CPU ベース。パーティクルは `Vec<Particle>` （ECS エンティティではない）
- `ParticleEmitter` コンポーネントを Transform 持ちエンティティにアタッチ
- インスタンス描画: ビルボードクアッド、Alpha ブレンド、深度書き込みなし
- 専用パイプライン + `particle.wgsl` シェーダー

## サウンドシステム
- `AudioManager` — rodio ラッパー（`Resources.audio: Option<AudioManager>`）
- `AudioSource` — ロード済み音声データ（`Arc<Vec<u8>>` キャッシュ）
- `play_sound()` — 効果音再生、`play_music()` — BGM ループ再生
- `AssetManager.load_audio(path)` でファイルキャッシュ対応

## Bind Group レイアウト
- Group 0: Camera (binding 0) + Light (binding 1)
- Group 1: Model Transform (binding 0)
- Group 2: Material (binding 0) + Texture (binding 1) + Sampler (binding 2)
- パーティクル: Group 0 のみ（Camera uniform を再利用）

## 主要な依存クレート
- `wgpu 29` — GPU 抽象化
- `winit 0.30` — ウィンドウ/入力
- `glam 0.29` — ベクトル/行列演算
- `hecs 0.11` — Entity Component System
- `bytemuck 1` — GPU バッファキャスト
- `tobj 4` — OBJ ファイル読み込み
- `image 0.25` — テクスチャ画像読み込み (PNG/JPEG)
- `rodio 0.20` — オーディオ再生 (効果音/BGM)
