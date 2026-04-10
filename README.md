# Thrust

wgpu ベースのクロスプラットフォーム 3D ゲームエンジン。

Vulkan / Metal / DX12 / OpenGL を wgpu が抽象化し、Windows・macOS・Linux で同一コードから 3D ゲームを構築できます。

## 特徴

- **ECS アーキテクチャ** — hecs による Entity Component System で柔軟なゲームオブジェクト管理
- **wgpu による GPU 抽象化** — バックエンドを意識せずに描画コードを記述
- **Blinn-Phong ライティング** — DirectionalLight + AmbientLight + スペキュラハイライト
- **親子階層** — Parent/Children コンポーネントで Transform を伝播
- **フラスタムカリング** — BoundingVolume による自動カリングで描画効率化
- **コリジョン検出** — AABB / Sphere コライダー + イベント通知
- **レイキャスト** — スクリーンピッキング、視線判定、距離順ソート
- **パーティクルシステム** — CPU ベース、ビルボードクアッドのインスタンス描画、テクスチャ対応
- **サウンド** — rodio による効果音・BGM 再生（ループ、音量、一時停止）
- **Tween アニメーション** — EaseFunction による Transform 補間
- **キーフレームアニメーション** — glTF からの Translation/Rotation/Scale キーフレーム再生
- **マルチフォーマットモデル** — OBJ / glTF / GLB / STL を拡張子から自動判定
- **豊富なテクスチャ形式** — PNG / JPEG / WebP / TGA / BMP / GIF / TIFF / ICO
- **アセット管理** — テクスチャ・メッシュ・音声のキャッシュ付きローダー（アンロード対応）
- **エンジン設定** — ウィンドウタイトル・サイズ・クリアカラー・VSync を `EngineConfig` で指定
- **デバッグ統計** — FPS・フレームタイムの自動計測（`Resources.debug_stats`）
- **プリミティブ生成** — Cube / Sphere / Plane / Quad をコードから生成
- **軌道カメラ** — マウスドラッグで回転、ホイールでズーム
- **数学ユーティリティ** — 補間（smoothstep, remap）、ジオメトリ（レイ交差, 重心座標）、行列分解、乱数生成など

## 必要環境

- **Rust 1.94.0** 以上
- GPU ドライバ (Vulkan / Metal / DX12 いずれか対応)
- Linux: `libasound2-dev` (サウンド機能に必要)

## クイックスタート

```bash
# リポジトリのクローン
git clone https://github.com/ToshikiMaeshima03/Thrust.git
cd Thrust

# モデルビューアで起動（OBJ/glTF/GLB/STL 対応）
cargo run -p obj_viewer

# glTF モデルを指定して起動
cargo run -p obj_viewer -- assets/models/scene.gltf

# プリミティブデモで起動
cargo run -p primitives_demo
```

## 使い方

### 基本的なアプリケーション

```rust
use thrust::*;

struct MyApp {
    cube: Option<Entity>,
}

impl ThrustAppHandler for MyApp {
    fn init(&mut self, world: &mut World, res: &mut Resources) {
        // キューブをスポーン
        self.cube = Some(spawn_cube(
            world,
            res,
            1.0,
            Transform::default(),
            Material::default(),
        ));
    }

    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
        // キューブを回転
        if let Some(entity) = self.cube
            && let Ok(mut t) = world.get::<&mut Transform>(entity)
        {
            t.rotation *= glam::Quat::from_rotation_y(dt);
        }
    }
}

fn main() {
    env_logger::init();
    thrust::run(MyApp { cube: None }).expect("エンジン起動失敗");
}
```

### エンジン設定

```rust
use thrust::EngineConfig;

fn main() {
    env_logger::init();
    let config = EngineConfig::default()
        .with_title("My Game")
        .with_size(1920, 1080)
        .with_clear_color(0.0, 0.0, 0.1, 1.0)
        .with_vsync(true);

    thrust::run_with_config(MyApp { cube: None }, config)
        .expect("エンジン起動失敗");
}
```

### デバッグ統計

```rust
fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
    // FPS とフレームタイムは毎フレーム自動更新される
    log::info!("FPS: {:.0}, Frame: {:.2}ms",
        res.debug_stats.fps, res.debug_stats.frame_time_ms);
}
```

### モデル読み込み（マルチフォーマット）

```rust
fn init(&mut self, world: &mut World, res: &mut Resources) {
    // 拡張子から自動判定 (OBJ / glTF / GLB / STL)
    let entities = spawn_model(world, res, "assets/models/scene.glb", Transform::default())
        .expect("モデルの読み込みに失敗しました");

    log::info!("{}個のメッシュを読み込みました", entities.len());
}
```

### レイキャスト（マウスピッキング）

```rust
fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
    if res.input.is_mouse_pressed(MouseButton::Left) {
        let (mx, my) = res.input.mouse_position();
        for (camera, _) in world.query_mut::<(&Camera, &ActiveCamera)>() {
            let ray = screen_to_ray(
                mx as f32, my as f32,
                res.gpu.size.width as f32, res.gpu.size.height as f32,
                camera,
            );
            let hits = ray_cast(world, &ray, 100.0);
            if let Some(hit) = hits.first() {
                log::info!("ヒット: distance={}", hit.distance);
            }
            break;
        }
    }
}
```

### パーティクル

```rust
fn init(&mut self, world: &mut World, res: &mut Resources) {
    // テクスチャ付きパーティクル
    let tex = res.assets.load_texture("assets/textures/spark.png",
        &res.gpu.device, &res.gpu.queue).unwrap();

    world.spawn((
        Transform::from_translation(glam::Vec3::new(0.0, 0.0, 0.0)),
        ParticleEmitter {
            emission_rate: 100.0,
            particle_lifetime: 1.5,
            initial_velocity_min: glam::Vec3::new(-0.3, 2.0, -0.3),
            initial_velocity_max: glam::Vec3::new(0.3, 4.0, 0.3),
            initial_color: glam::Vec4::new(1.0, 0.5, 0.1, 1.0),
            initial_size: 0.15,
            texture: Some(tex),  // テクスチャ指定（None = 円形フェード）
            ..Default::default()
        },
    ));
}
```

### キーフレームアニメーション

```rust
fn init(&mut self, world: &mut World, res: &mut Resources) {
    // glTF からモデル+アニメーションを読み込む
    let result = thrust::load_gltf(&res.gpu.device, &res.gpu.queue,
        std::path::Path::new("assets/models/animated.glb")).unwrap();

    // メッシュをスポーン
    for (mesh, material) in result.meshes.into_iter().zip(result.materials) {
        let entity = spawn_object(world, mesh, Transform::default(), material);

        // 最初のアニメーションを適用
        if let Some(anim_data) = result.animations.first() {
            world.insert_one(entity, KeyframeAnimation::new(
                anim_data.name.clone(),
                anim_data.tracks.clone(),
                anim_data.duration,
            ).with_loop(true)).ok();
        }
    }
}
```

### サウンド

```rust
fn init(&mut self, world: &mut World, res: &mut Resources) {
    let bgm = res.assets.load_audio("assets/audio/bgm.ogg").unwrap();
    if let Some(audio) = &mut res.audio {
        self.bgm_handle = audio.play_music(&bgm).ok();
    }
}

fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
    if res.input.is_key_pressed(KeyCode::KeyM) {
        if let Some(audio) = &mut res.audio {
            if let Some(handle) = &self.bgm_handle {
                if audio.is_paused(handle) {
                    audio.resume(handle);
                } else {
                    audio.pause(handle);
                }
            }
        }
    }
}
```

### カメラ操作

| 操作 | 動作 |
|------|------|
| 左ドラッグ | 回転 (ヨー / ピッチ) |
| マウスホイール | ズーム (距離 0.5 〜 50.0) |

## 対応フォーマット

### 3D モデル
| 形式 | 拡張子 | 機能 |
|------|--------|------|
| Wavefront OBJ | `.obj` | メッシュ |
| glTF 2.0 | `.gltf` `.glb` | メッシュ + マテリアル + アニメーション |
| STL | `.stl` | メッシュ（3D プリント用） |

### テクスチャ画像
PNG, JPEG, WebP, TGA, BMP, GIF, TIFF, ICO

### オーディオ
OGG, WAV, MP3, FLAC（rodio 対応形式）

## アーキテクチャ

### ECS (Entity Component System)

```
World (hecs::World)
  └─ Entity
       ├─ Transform          — 位置・回転・スケール
       ├─ MeshHandle          — GPU メッシュ
       ├─ Material            — 色 + テクスチャ
       ├─ Collider            — コリジョン形状
       ├─ ParticleEmitter     — パーティクル放出
       ├─ KeyframeAnimation   — キーフレームアニメーション
       └─ ...

Resources (グローバルシングルトン)
  ├─ GpuContext       — wgpu デバイス・キュー・サーフェス
  ├─ Time             — デルタタイム
  ├─ Input            — キーボード・マウス入力
  ├─ Events           — 型消去イベントキュー
  ├─ AssetManager     — テクスチャ・メッシュ・音声キャッシュ
  ├─ AudioManager     — サウンド再生制御
  └─ DebugStats       — FPS・フレームタイム
```

### フレームループ

```
handler.update()               — ユーザーロジック
animation_system()             — Tween アニメーション
keyframe_animation_system()    — キーフレームアニメーション
velocity_system()              — 速度 → Transform 適用
particle_system()              — パーティクル生成・更新・消滅
camera_system()                — カメラ更新 → GPU バッファ書き込み
light_system()                 — ライト更新 → GPU バッファ書き込み
propagate_transforms()         — 親子階層の GlobalTransform 伝播
collision_system()             — AABB コリジョン検出 → CollisionEvent 発行
render_prep_system()           — GPU バッファ作成・更新
render_system()                — 不透明オブジェクト描画 + パーティクル描画
```

## プロジェクト構成

```
thrust/
├── Cargo.toml                  # ワークスペース定義
├── crates/
│   └── thrust/                 # コアエンジンライブラリ
│       └── src/
│           ├── app.rs              # ThrustAppHandler, フレームループ
│           ├── ecs/                # ECS (コンポーネント, リソース, システム, スポーン)
│           ├── animation/          # Tween + キーフレームアニメーション
│           ├── asset/              # アセットマネージャー
│           ├── audio/              # サウンド (rodio)
│           ├── camera/             # カメラ & 軌道コントローラ
│           ├── event/              # イベントシステム
│           ├── light/              # ライティング
│           ├── material/           # マテリアル
│           ├── math/               # AABB, BoundingSphere
│           ├── mesh/               # 頂点, メッシュ, OBJ/glTF/STL ローダー, プリミティブ
│           ├── particle/           # パーティクルシステム（テクスチャ対応）
│           ├── physics/            # コリジョン, レイキャスト
│           ├── renderer/           # GPU コンテキスト, パイプライン, フラスタム
│           ├── scene/              # Transform, 親子階層, ModelUniform
│           └── shader/             # WGSL シェーダー (default, particle, particle_textured)
├── examples/
│   ├── obj_viewer/             # モデルビューアサンプル（全形式対応）
│   └── primitives_demo/        # プリミティブ・入力・ライティングデモ
└── assets/
    └── models/                 # サンプルモデル
```

## 開発

```bash
cargo check                    # 型チェック
cargo build                    # ビルド
cargo test                     # テスト実行
cargo fmt                      # フォーマット
cargo clippy -- -D warnings    # Lint
```

## 主要な依存クレート

| クレート | バージョン | 用途 |
|----------|-----------|------|
| wgpu | 29 | GPU 抽象化 |
| winit | 0.30 | ウィンドウ/入力 |
| glam | 0.29 | ベクトル/行列演算 |
| hecs | 0.11 | Entity Component System |
| bytemuck | 1 | GPU バッファキャスト |
| rodio | 0.20 | オーディオ再生 |
| tobj | 4 | OBJ ファイル読み込み |
| gltf | 1 | glTF/GLB ファイル読み込み |
| stl_io | 0.8 | STL ファイル読み込み |
| image | 0.25 | テクスチャ画像読み込み |

## ライセンス

MIT OR Apache-2.0
