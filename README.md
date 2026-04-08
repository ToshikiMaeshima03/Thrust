# Forge3D

wgpu ベースのクロスプラットフォーム 3D ゲームエンジン。

Vulkan / Metal / DX12 / OpenGL を wgpu が抽象化し、Windows・macOS・Linux で同一コードから 3D ゲームを構築できます。

## 特徴

- **ECS アーキテクチャ** — hecs による Entity Component System で柔軟なゲームオブジェクト管理
- **wgpu による GPU 抽象化** — バックエンドを意識せずに描画コードを記述
- **Phong ライティング** — DirectionalLight + AmbientLight によるシンプルな陰影
- **親子階層** — Parent/Children コンポーネントで Transform を伝播
- **フラスタムカリング** — BoundingVolume による自動カリングで描画効率化
- **コリジョン検出** — AABB / Sphere コライダー + イベント通知
- **レイキャスト** — スクリーンピッキング、視線判定、距離順ソート
- **パーティクルシステム** — CPU ベース、ビルボードクアッドのインスタンス描画
- **サウンド** — rodio による効果音・BGM 再生（ループ、音量、一時停止）
- **Tween アニメーション** — EaseFunction による Transform 補間
- **アセット管理** — テクスチャ・メッシュ・音声のキャッシュ付きローダー
- **OBJ ファイル読み込み** — Wavefront OBJ 形式のモデルをロード
- **プリミティブ生成** — Cube / Sphere / Plane / Quad をコードから生成
- **軌道カメラ** — マウスドラッグで回転、ホイールでズーム

## 必要環境

- **Rust 1.94.0** 以上
- GPU ドライバ (Vulkan / Metal / DX12 いずれか対応)
- Linux: `libasound2-dev` (サウンド機能に必要)

## クイックスタート

```bash
# リポジトリのクローン
git clone https://github.com/ToshikiMaeshima03/Thrust.git
cd Thrust

# OBJ ビューアで起動
cargo run -p obj_viewer

# プリミティブデモで起動
cargo run -p primitives_demo
```

## 使い方

### 基本的なアプリケーション

```rust
use forge3d::*;

struct MyApp {
    cube: Option<Entity>,
}

impl ForgeAppHandler for MyApp {
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
    forge3d::run(MyApp { cube: None });
}
```

### レイキャスト（マウスピッキング）

```rust
fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
    if res.input.is_mouse_pressed(MouseButton::Left) {
        let (mx, my) = res.input.mouse_position();
        // アクティブカメラを取得
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
fn init(&mut self, world: &mut World, _res: &mut Resources) {
    // 炎のようなパーティクルエミッターを生成
    world.spawn((
        Transform::from_translation(glam::Vec3::new(0.0, 0.0, 0.0)),
        ParticleEmitter {
            emission_rate: 100.0,
            particle_lifetime: 1.5,
            initial_velocity_min: glam::Vec3::new(-0.3, 2.0, -0.3),
            initial_velocity_max: glam::Vec3::new(0.3, 4.0, 0.3),
            initial_color: glam::Vec4::new(1.0, 0.5, 0.1, 1.0),
            initial_size: 0.15,
            size_over_lifetime: 0.0,
            fade_out: true,
            gravity: glam::Vec3::new(0.0, -2.0, 0.0),
            ..Default::default()
        },
    ));
}
```

### サウンド

```rust
fn init(&mut self, world: &mut World, res: &mut Resources) {
    // 音声ファイルをロード（キャッシュ対応）
    let bgm = res.assets.load_audio("assets/audio/bgm.ogg").unwrap();

    // BGM をループ再生
    if let Some(audio) = &mut res.audio {
        self.bgm_handle = audio.play_music(&bgm).ok();
    }
}

fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
    // M キーで BGM の一時停止/再開
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
       └─ ...

Resources (グローバルシングルトン)
  ├─ GpuContext       — wgpu デバイス・キュー・サーフェス
  ├─ Time             — デルタタイム
  ├─ Input            — キーボード・マウス入力
  ├─ Events           — 型消去イベントキュー
  ├─ AssetManager     — テクスチャ・メッシュ・音声キャッシュ
  └─ AudioManager     — サウンド再生制御
```

### フレームループ

```
handler.update()           — ユーザーロジック
animation_system()         — Tween アニメーション
velocity_system()          — 速度 → Transform 適用
particle_system()          — パーティクル生成・更新・消滅
camera_system()            — カメラ更新 → GPU バッファ書き込み
light_system()             — ライト更新 → GPU バッファ書き込み
propagate_transforms()     — 親子階層の GlobalTransform 伝播
collision_system()         — AABB コリジョン検出 → CollisionEvent 発行
render_prep_system()       — GPU バッファ作成・更新
render_system()            — 不透明オブジェクト描画 + パーティクル描画
```

## プロジェクト構成

```
forge3d/
├── Cargo.toml                  # ワークスペース定義
├── crates/
│   └── forge3d/                # コアエンジンライブラリ
│       └── src/
│           ├── app.rs              # ForgeAppHandler, フレームループ
│           ├── ecs/                # ECS (コンポーネント, リソース, システム, スポーン)
│           ├── animation/          # Tween アニメーション
│           ├── asset/              # アセットマネージャー
│           ├── audio/              # サウンド (rodio)
│           ├── camera/             # カメラ & 軌道コントローラ
│           ├── event/              # イベントシステム
│           ├── light/              # ライティング
│           ├── material/           # マテリアル
│           ├── math/               # AABB, BoundingSphere
│           ├── mesh/               # 頂点, メッシュ, OBJ ローダー, プリミティブ
│           ├── particle/           # パーティクルシステム
│           ├── physics/            # コリジョン, レイキャスト
│           ├── renderer/           # GPU コンテキスト, パイプライン, フラスタム
│           ├── scene/              # Transform, 親子階層, ModelUniform
│           └── shader/             # WGSL シェーダー (default, particle)
├── examples/
│   ├── obj_viewer/             # OBJ ビューアサンプル
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
| image | 0.25 | テクスチャ画像読み込み |

## ライセンス

MIT OR Apache-2.0
