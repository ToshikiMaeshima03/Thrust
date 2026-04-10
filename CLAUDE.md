# Thrust - Claude Code 開発ガイド

## プロジェクト概要
wgpu ベースのクロスプラットフォーム 3D ゲームエンジン。ECS (Entity Component System) アーキテクチャ採用。

## ワークスペース構成
- `crates/thrust` — コアエンジンライブラリ
- `examples/obj_viewer` — モデルビューアサンプル（OBJ/glTF/GLB/STL 対応）
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

## コミット運用ルール (重要)
- **基本作業 (1 つの機能追加 / バグ修正 / リファクタ等) が一区切りついたら必ずコミットする**
- コミット前に `cargo fmt && cargo clippy --workspace -- -D warnings && cargo test --workspace` が全て green であることを確認する
- コミットメッセージは日本語、HEREDOC で渡し、bullet 形式で「何を」「なぜ」を記述する
- セクション区切りで複数の変更がまとまる場合は `## サブタイトル` で構造化する (Round 4 / Round 5 等)
- ユーザーから「Push して」と言われた時点で `git push origin main` する。明示的に言われない限り push しない
- 大規模変更でも 1 機能 1 コミットを原則とする (今後)。Round 4-8 のようなまとめコミットは例外的

## コーディング規約
- Rust edition 2024, MSRV 1.94.0
- エラーメッセージ・ログは日本語
- `cargo fmt` と `cargo clippy -- -D warnings` がクリーンであること
- `#![allow(clippy::module_inception)]` をクレートルートで許可済み
- GPU 互換の構造体には `#[repr(C)]` + `Pod`/`Zeroable` を使用
- WGSL の vec3 フィールドは 16 バイトアライメント（[f32; 3] + f32 パディング）
- エラー型は `ThrustError`（`error.rs`）を使用。`Result<T, String>` は使わない
- `run()` / `run_with_config()` は `ThrustResult<()>` を返す。`EngineConfig` でウィンドウ/描画設定を指定可能
- Material (Round 4): `base_color_factor` + `metallic_factor` + `roughness_factor` + 5 PBR テクスチャ (base_color/MR/normal/AO/emissive) + `emissive_factor`
- シェーダー (Round 5): Cook-Torrance PBR (GGX + Schlick + Smith) + マルチライト (directional ×4, point ×32, spot ×16) + **Cascaded Shadow Maps (3 cascade)** (3×3 PCF) + IBL (irradiance + prefilter + BRDF LUT) + skinning + HDR Rgba16Float + MSAA 4× + ACES tonemap + bloom (5 mip) + FXAA + Skybox + **ボリュメトリックフォグ** (高さ減衰 + 散乱)
- 数学定数 (`std::f32::consts::PI`/`FRAC_PI_4` 等) は直接書き値ではなく定数を使う (`clippy::approx_constant` deny-by-default)

## hecs クエリ仕様 (重要)
- `query::<&T>().iter()` は **`&T` を直接** yield する (Entity 込みの 2-tuple ではない)
- `query::<(Entity, &T)>().iter()` は **`(Entity, &T)` 2-tuple** を yield
- `query::<(Entity, &T1, &T2, &T3)>().iter()` は **flat 4-tuple** `(Entity, &T1, &T2, &T3)` を yield (ネストしない)
- destructure 例: `for (e, _, _, _) in q.iter() { ... }` (4 components の場合)

## WGSL 深度テクスチャの読み方
- `texture_depth_2d` を post-process でサンプルするときは `textureLoad(t, pixel_coord, 0)` を使う
- これでサンプラーが不要になり、`Filtering`/`NonFiltering` の互換性問題を完全に回避できる
- 比較サンプリングが必要な場合のみ `textureSampleCompareLevel` + comparison sampler を使う

## CameraUniform 同期ルール
- `crates/thrust/src/camera/uniform.rs` の `CameraUniform` struct を変更したら、
  全 WGSL ファイル (default/instanced/skybox/particle/particle_textured/ssao/prepass/ssr/decal/volumetric/dof/motion_blur/color_grading) の同名 struct も同時に更新する必要あり
- レイアウトミスマッチは即パイプライン作成失敗 → ランタイムクラッシュ

## Round 7: G-Buffer Prepass + ポストエフェクト基盤
- `renderer/prepass.rs` が depth + view-space normal (Rgba16Float) + material (Rgba8Unorm) + motion (Rg16Float) を 1 パスで MRT 出力
- SSAO / SSR / Decal / Volumetric / Motion Blur はすべてこの prepass の結果を共有
- render_system のパス順序: prepass → CSM shadow → shadow_atlas (point/spot) → HDR main (MSAA 4×) → decal → SSAO+blur → SSR → volumetric → composite → bloom → tonemap → FXAA → DOF → motion blur → color grading → surface → egui
- PBR シェーダーは G3 (shadow_atlas) を追加バインド。instanced は G3 なしの 3-group のまま (両者とも同じレンダーパス内で描画可)

## Round 8: PBR 拡張 + 高度レンダリング + ゲームプレイ
- `MaterialUniform` は 64 → **128 B** に拡張: clearcoat/anisotropic/SSS の `extended/aniso_dir/subsurface_color/_padding2` (各 vec4) を追加。**全 13 シェーダー** (default/instanced/skybox/particle/particle_textured/ssao/prepass/ssr/decal/volumetric/dof/motion_blur/color_grading/water/clouds/foliage/taa/gpu_particle/shadow_atlas) で `MaterialUniform`/`CameraUniform` struct を同期する必要あり
- 新規ヘルパー: `Material::car_paint(color)`, `brushed_metal(color, anisotropy)`, `skin(color)`
- 新規モジュール: `renderer/{auto_exposure, clouds, foliage, gpu_particles, lens_flare, reflection_probe, taa, trail, water}.rs` + `physics/{cloth, ragdoll, vehicle}.rs` + `math/spline.rs` + `input_action.rs` + `save.rs` + `jobs.rs` + `scene/streaming.rs`
- TAA 用に history ピンポンテクスチャ (history_a/b) を保持、毎フレーム `swap()` する
- GPU パーティクルは compute shader (`@compute @workgroup_size(64)`) + storage buffer (VERTEX|STORAGE usage)
- 全ての新システムは ECS と統合済み (`cloth_system`, `vehicle_system`, `trail_sample_system` 等)

## アーキテクチャ (ECS)

### コア概念
- **World** (`hecs::World`): 全エンティティとコンポーネントを保持
- **Entity** (`hecs::Entity`): エンティティハンドル（Copy + Eq + Hash）
- **Resources**: エンティティに属さないグローバルシングルトン（GPU, Time, Input, Audio 等）
- **Systems**: World + Resources を受け取って処理する関数

### ThrustAppHandler トレイト
```rust
pub trait ThrustAppHandler {
    fn init(&mut self, world: &mut World, res: &mut Resources);
    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {}
}
```

### フレームループ（システム実行順序、Round 4 で更新）
```
handler.update()               — ユーザーロジック
animation_system()             — Tween アニメーション
keyframe_animation_system()    — キーフレームアニメーション
velocity_system()              — 速度→Transform 適用 (legacy)
physics_init_system()          — Round 4: 新規 RigidBody+Collider を rapier に登録
physics_step_system()          — Round 4: rapier 物理ステップ
physics_sync_from_system()     — Round 4: rapier → ECS Transform 同期
particle_system()              — パーティクル更新（生成・移動・消滅）
camera_system()                — カメラ更新
light_system()                 — ライト更新 (Round 4: マルチライト + シャドウ VP 計算)
propagate_transforms()         — 親子階層の GlobalTransform 伝播
skin_system()                  — Round 4: スケルタル skin matrix 計算
collision_system()             — AABB コリジョン検出 (legacy)
particle_render_prep()         — パーティクルインスタンスバッファ更新（バッチ描画）
render_prep_system()           — GPU バッファ作成・更新
render_system()                — 描画 (Round 4: シャドウパス → メインパス → パーティクル)
audio.cleanup_finished()       — 完了サウンドのクリーンアップ
```

### モジュール構成
```
app.rs              — イベントループ、ウィンドウ管理、ThrustAppHandler（ThrustResult<()> を返す）
config.rs           — EngineConfig (ウィンドウタイトル/サイズ/クリアカラー/VSync/省電力)
debug.rs            — DebugStats (FPS/フレームタイム、Resources.debug_stats 経由)
error.rs            — ThrustError enum, ThrustResult 型エイリアス（thiserror ベース）
ecs/
  components.rs     — MeshHandle, RenderState, Visible, Name, マーカー
  resources.rs      — Resources (GPU, Time, Input, Events, Assets, Audio, パイプライン)
  systems.rs        — camera/light/render_prep/render システム
  spawn.rs          — spawn_object, spawn_cube, spawn_sphere, spawn_plane, spawn_model, spawn_child, despawn
animation/
  tween.rs          — TransformAnimation, EaseFunction, animation_system
  keyframe.rs       — KeyframeAnimation, KeyframeTrack, KeyframeValues, keyframe_animation_system
  skin.rs           — Round 4: Skeleton, Joint, SkinnedMesh, skin_system
asset/              — AssetManager (テクスチャ・メッシュ・音声・モデルキャッシュ)
audio/              — AudioManager, AudioSource, SoundHandle (rodio + 距離減衰), AudioEmitter, AudioListener
camera/             — Camera, OrbitalController, CameraUniform
event/              — Events (型消去イベントキュー)
input.rs            — キーボード・マウス入力状態管理
light/              — Round 4: DirectionalLight, PointLight, SpotLight, AmbientLight, GpuLight, LightsHeader (storage SSBO)
material/           — Round 4: PBR Material (base_color/MR/normal/AO/emissive), MaterialUniform
math/
  bounds.rs         — Aabb, BoundingSphere
  numeric.rs        — smoothstep, smootherstep, inverse_lerp, remap, nearly_equal, move_towards, wrap
  angle.rs          — deg_to_rad, rad_to_deg, normalize_angle, signed_angle
  geometry.rs       — closest_point_on_line_segment, ray_plane_intersection, ray_triangle_intersection, barycentric_coords, triangle_area, triangle_normal
  matrix.rs         — extract_scale, extract_max_scale, extract_translation, decompose
  quat_utils.rs     — look_rotation, spherical_to_cartesian
  random.rs         — SimpleRng (LCG 擬似乱数)
mesh/
  vertex.rs         — Vertex, compute_face_normals
  mesh.rs           — Mesh (vertex_buffer, index_buffer, num_indices)
  obj_loader.rs     — OBJ ローダー
  gltf_loader.rs    — glTF/GLB ローダー (メッシュ+マテリアル+アニメーション)
  stl_loader.rs     — STL ローダー
  model_loader.rs   — 統一モデルローダー (拡張子で自動判定)
  primitives.rs     — Cube/Sphere/Plane/Quad プリミティブ生成
particle/
  emitter.rs        — Particle, ParticleEmitter (テクスチャ対応), particle_system
  render.rs         — ParticleInstance, ParticleRenderState, バッチ描画 (テクスチャ有無)
physics/
  collider.rs       — Collider, ColliderShape, Velocity, collision_system (legacy)
  ray.rs            — Ray, RayHit, ray_cast, screen_to_ray
  rapier_world.rs   — Round 4: PhysicsWorld, RigidBody, RigidBodyType, PhysicsHandle (rapier3d ラッパー)
  systems.rs        — Round 4: physics_init/step/sync_from システム
renderer/
  context.rs        — GpuContext (device, queue, surface)
  frustum.rs        — Frustum, BoundingVolume (フラスタムカリング)
  pipeline.rs       — ThrustBindGroupLayouts, RenderPipeline (Round 4: PBR)
  render_pass.rs    — DepthTexture, RenderResult
  shadow.rs         — Round 4: ShadowMap, LightSpaceVp (2048² 方向光シャドウ)
  texture.rs        — ThrustTexture (from_bytes, from_path, white/normal/mr/black フォールバック)
scene/
  hierarchy.rs      — Parent, Children, GlobalTransform, propagate_transforms
  transform.rs      — Transform (translation, rotation, scale, Clone)
  scene.rs          — ModelUniform (GPU 用)
shader/
  default.wgsl      — Round 4: PBR シェーダー (Cook-Torrance + マルチライト + シャドウ + Reinhard)
  shadow.wgsl       — Round 4: 深度のみシャドウマップ生成
  particle.wgsl     — パーティクルシェーダー (ビルボード + 円形フェード)
  particle_textured.wgsl — テクスチャ付きパーティクルシェーダー
time.rs             — デルタタイム、フレームカウント
```

## 対応フォーマット

### モデル
- `.obj` — Wavefront OBJ (tobj)
- `.gltf` / `.glb` — glTF 2.0 (gltf クレート、メッシュ+マテリアル+アニメーション)
- `.stl` — STL バイナリ/ASCII (stl_io)
- `load_model(device, queue, path)` で拡張子から自動判定

### テクスチャ
- PNG, JPEG, WebP, TGA, BMP, GIF, TIFF, ICO (image クレート)
- `ThrustTexture::from_path()` / `from_bytes()` で自動フォーマット検出

### アニメーション
- Tween: `TransformAnimation` (コードで定義、start→end 補間)
- キーフレーム: `KeyframeAnimation` (glTF からロード、Translation/Rotation/Scale)

## ECS コンポーネント一覧

### 描画関連
- `Transform` — ローカル変換 (translation, rotation, scale)
- `GlobalTransform` — ワールド空間変換（階層伝播済み）
- `MeshHandle(Mesh)` — GPU メッシュ参照
- `Material` — Round 4: PBR (base_color_factor / metallic / roughness / normal / AO / emissive)
- `RenderState` — GPU バッファ・バインドグループ（エンジン管理）
- `Visible(bool)` — 表示/非表示
- `BoundingVolume(Aabb)` — フラスタムカリング用

### ライト (Round 4: マルチライト対応、最大 dir×4 + point×32 + spot×16)
- `DirectionalLight { direction, color, intensity }` — 平行光源 (複数アクティブ可)
- `PointLight { color, intensity, range }` — 点光源 (距離減衰)
- `SpotLight { color, intensity, range, inner_angle, outer_angle, direction }` — スポット光源
- `AmbientLight { color, intensity }` — 環境光

### マーカー
- `ActiveCamera` — アクティブカメラ
- `ActiveAmbientLight` — アクティブ環境光
- `ActiveDirectionalLight` — Round 4 で旧 API 互換マーカー (光源収集には不要)

### 階層
- `Parent(Entity)` — 親エンティティ
- `Children(Vec<Entity>)` — 子エンティティリスト

### 物理 (Round 4: rapier3d 統合)
- `Collider { shape, is_trigger }` — legacy コリジョン形状 (collision_system 用)
- `Velocity { linear }` — legacy 速度
- `RigidBody { body_type, linear_damping, angular_damping, initial_velocity }` — Round 4: rapier 剛体定義
- `PhysicsHandle { body, collider }` — Round 4: rapier ハンドル (`physics_init_system` が挿入)

### スケルタル (Round 4)
- `Skeleton { joint_entities, inverse_bind_matrices }` — スケルトン定義
- `Joint { skeleton, index }` — 関節マーカー
- `SkinnedMesh { skeleton, joint_matrices }` — スキンメッシュ (skin_system が更新)

### オーディオ (Round 4)
- `AudioEmitter { source, max_distance, auto_play }` — 3D 音響エミッタ
- `AudioListener` — リスナーマーカー (アクティブカメラと同居)

### パーティクル
- `ParticleEmitter` — パーティクルエミッター（emission_rate, lifetime, velocity, color, gravity, texture 等）

### アニメーション
- `TransformAnimation` — Tween Transform 補間 (lerp/slerp)
- `KeyframeAnimation` — キーフレーム Transform 補間（glTF 対応）

## レイキャスト
- `Ray { origin, direction }` — レイ（スラブ法 AABB / 二次方程式 Sphere 交差判定）
- `RayHit { entity, distance, point }` — ヒット結果
- `ray_cast(world, ray, max_distance)` — World 内 Collider 全検索（距離順ソート）
- `screen_to_ray(x, y, w, h, camera)` — スクリーン座標→ワールドレイ変換

## パーティクルシステム
- CPU ベース。パーティクルは `Vec<Particle>` （ECS エンティティではない）
- `ParticleEmitter` コンポーネントを Transform 持ちエンティティにアタッチ
- テクスチャ対応: `ParticleEmitter.texture: Option<Arc<ThrustTexture>>`
- バッチ描画: テクスチャ有無でグルーピング、2つのパイプライン
- 専用パイプライン + `particle.wgsl` / `particle_textured.wgsl` シェーダー

## サウンドシステム
- `AudioManager` — rodio ラッパー（`Resources.audio: Option<AudioManager>`）
- `AudioSource` — ロード済み音声データ（`Arc<Vec<u8>>` キャッシュ）
- `play_sound()` — 効果音再生、`play_music()` — BGM ループ再生
- `AssetManager.load_audio(path)` でファイルキャッシュ対応

## Bind Group レイアウト (Round 5: PBR + CSM + IBL + スキニング + フォグ)
- Group 0: Camera (b0) + LightsHeader uniform (b1) + Lights storage SSBO (b2) + Shadow **D2Array** texture (b3) + Shadow comparison sampler (b4) + **CSM uniform** (3 matrices + splits) (b5) + IBL irradiance cube (b6) + IBL prefilter cube (b7) + IBL BRDF LUT (b8) + IBL sampler (b9) + **Fog uniform** (b10)
- Group 1: Model Transform uniform (b0) + Joint matrices SSBO (b1, スキニング用)
- Group 2: PBR Material uniform (b0) + base_color tex (b1) + sampler (b2) + MR tex (b3) + normal tex (b4) + AO tex (b5) + emissive tex (b6)

## レンダーパス順序 (Round 4 後半)
1. **Shadow Pass** — 2048² D32 depth-only、シーン AABB から最適 light VP
2. **HDR Main Pass** — Rgba16Float MSAA 4× → resolve、PBR + skybox + particles
3. **Bloom Threshold** — HDR resolved → bloom mip 0
4. **Bloom Downsample** ×4 — bloom mip 0..N-2 → bloom mip 1..N-1
5. **Bloom Upsample** ×4 — additive blend で bloom mip N-1..1 → bloom mip 0..N-2
6. **Tonemap Composite** — HDR + bloom → LDR intermediate (ACES Filmic + sRGB)
7. **FXAA** — LDR intermediate → surface
8. **egui Pass** — surface オーバーレイ (UI/HUD)
- パーティクル (テクスチャなし): Group 0 のみ（Camera uniform を再利用）
- パーティクル (テクスチャ付き): Group 0 (Camera) + Group 1 (Texture + Sampler)

## 主要な依存クレート
- `wgpu 29` — GPU 抽象化
- `winit 0.30` — ウィンドウ/入力
- `glam 0.29` (mint feature) — ベクトル/行列演算
- `hecs 0.11` — Entity Component System
- `bytemuck 1` — GPU バッファキャスト
- `tobj 4` — OBJ ファイル読み込み
- `gltf 1` — glTF/GLB ファイル読み込み
- `stl_io 0.8` — STL ファイル読み込み
- `image 0.25` — テクスチャ画像読み込み (PNG/JPEG/WebP/TGA/BMP/GIF/TIFF/ICO/HDR)
- `rapier3d 0.22` — Round 4: 剛体物理エンジン
- `nalgebra 0.33` — Round 4: rapier3d バックエンド数学
- `mikktspace 0.3` — Round 4: タンジェント生成 (PBR ノーマルマップ用)
- `kira 0.12` — Round 4 後半: 3D 空間音響 (rodio から移行)
- `mint 0.5` — Round 4 後半: glam ↔ kira 数学型 interop
- `egui 0.34` / `egui-wgpu 0.34` / `egui-winit 0.34` — Round 4 後半: デバッグ HUD
- `serde 1` / `serde_json 1` — Round 5: シーンシリアライゼーション

## Round 6 監査履歴 (Unreal Engine ギャップクロージャ - 続き 2)
**追加機能** (詳細は memory/MEMORY.md):
- **アニメーションステートマシン + ブレンドツリー**: `animation/state_machine.rs`。State/Transition/Condition、1D BlendTree (パラメータ駆動補間)、クロスフェード対応
- **ビヘイビアツリー**: `ai/behavior_tree.rs`。Sequence/Selector/Inverter/Repeater/UntilSuccess + Action/Condition リーフ + Blackboard (float/bool/int/vec3)
- **ハイトマップ地形**: `mesh/terrain.rs`。`create_terrain_from_heightmap` + `sine_heightmap` / `noise_heightmap` ヘルパ、法線/tangent 自動計算
- **LOD システム**: `mesh/lod.rs`。`MeshLod` コンポーネント + 距離ベース切替 + 範囲外 cull
- **マテリアルインスタンス**: `material/instance.rs`。`MaterialTemplate` (ベース共有) + `MaterialInstance` (param override)、予約済みキー (`metallic`/`roughness`/`base_color`/`emissive`) で自動解決
- **トリガーボリューム**: `physics/triggers.rs`。`TriggerVolume` コンポーネント + Enter/Stay/Exit イベント (前フレーム集合との diff)
- **SSAO ポストプロセス**: `renderer/ssao.rs` + `shader/ssao.wgsl`/`ssao_blur.wgsl`。16 サンプル半球 + ハッシュ回転 + 4×4 ボックスブラー (非 MSAA depth prepass 必要、モジュールは完成、main pass 統合は次回)
- **テスト**: 268 → 315 (+47)

## Round 5 監査履歴 (Unreal Engine ギャップクロージャ - 続き)
**追加機能** (詳細は memory/MEMORY.md):
- **Cascaded Shadow Maps (CSM)**: 3 カスケード × 2048² Depth32Float テクスチャ配列、PSSM 風カスケード分割 (5%/20%/100%)、テクセルスナップでちらつき防止
- **ボリュメトリックフォグ**: 指数高さフォグ + 太陽方向への光散乱、`FogUniform::outdoor/dense` ヘルパ
- **GPU インスタンシング**: `InstancedMesh` コンポーネント + per-vertex instance buffer (mat4)、専用パイプライン (foliage/群衆用)
- **シーンシリアライゼーション**: serde + JSON で Transform/Material/Light/RigidBody/Collider/Velocity を保存・復元 (`SerScene::from_world` / `apply_to_world` / `save_to_file` / `load_from_file`)
- **AI/Navmesh + A***: グリッドベース navmesh (XZ 平面)、8 方向 A* 探索、`AgentMover` コンポーネント + `agent_movement_system` で waypoint 追従
- **Path smoothing**: line-of-sight ベースの string pulling アルゴリズム
- **テスト**: 225 → 268 (+43)

## Round 4 監査履歴 (Unreal Engine 機能ギャップ クロージャ)
**主要追加機能** (詳細は memory/MEMORY.md):
- **PBR レンダリング**: Cook-Torrance BRDF (GGX + Schlick + Smith) で Blinn-Phong 完全置き換え
- **マルチライト**: directional ×4, point ×32, spot ×16 (合計 52)、storage SSBO ベース、距離減衰
- **方向光シャドウマップ**: 2048² Depth32Float、3×3 PCF、シーン AABB から最適 VP 自動計算
- **頂点フォーマット拡張**: tangent (mikktspace) + joints + weights、ストライド 32→80 B
- **rapier3d 物理**: 剛体ダイナミクス + 重力 + 拘束 + ブロードフェーズ BVH、既存 collision_system と共存
- **スケルタルアニメ + シェーダースキニング**: Skeleton/Joint/SkinnedMesh コンポーネント + skin_system (joint matrix 計算) + 頂点シェーダー skinning 分岐 (joint matrices SSBO)
- **空間音響 (kira)**: kira 0.12 で 3D 空間音響 (`AudioManager::play_spatial`)、リスナー位置/向きを ECS で同期
- **glTF PBR**: 全 PBR テクスチャ (base_color/MR/normal/AO/emissive) を抽出
- **HDR + MSAA + ACES**: Rgba16Float + MSAA 4× + ACES Filmic tonemap + bloom (5 mip downsample/upsample) + FXAA 3.11
- **Skybox + IBL**: プロシージャル sky / cubemap + irradiance (32²) + prefilter (128² × 5 mips) + BRDF LUT (256²)
- **egui デバッグ HUD**: egui 0.34 + egui-wgpu 0.34 + egui-winit 0.34 (wgpu 29 互換版)、`ThrustAppHandler::ui` フック
- **テスト**: 189 → 225 (+36)
- **CI**: cargo fmt / clippy -D warnings / test 全パス
