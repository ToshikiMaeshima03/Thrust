use std::path::PathBuf;

/// Thrust エンジンの統一エラー型
#[derive(Debug, thiserror::Error)]
pub enum ThrustError {
    // ---- I/O ----
    #[error("ファイル読み込みエラー ({path}): {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    // ---- 画像 / テクスチャ ----
    #[error("画像読み込みエラー: {0}")]
    Image(#[from] image::ImageError),

    #[error("テクスチャデータエラー: {0}")]
    TextureData(String),

    // ---- メッシュローダー ----
    #[error("OBJ読み込みエラー: {0}")]
    ObjLoad(#[from] tobj::LoadError),

    #[error("glTF読み込みエラー: {0}")]
    GltfLoad(#[from] gltf::Error),

    #[error("STL読み込みエラー: {0}")]
    StlLoad(String),

    #[error("メッシュが含まれていません: {0}")]
    EmptyMesh(String),

    #[error("サポートされていないモデル形式です: .{0}")]
    UnsupportedFormat(String),

    // ---- アセット ----
    #[error("アセットは既にロード済みです: {0}")]
    AlreadyLoaded(String),

    #[error("アセットロードエラー: {0}")]
    AssetLoad(String),

    // ---- スクリプト ----
    #[error("スクリプトエラー: {0}")]
    Script(String),

    // ---- アニメーション ----
    #[error("アニメーションエラー: {0}")]
    Animation(String),

    // ---- オーディオ (Round 4: kira ベース) ----
    #[error("音声デコードエラー: {0}")]
    AudioDecode(String),

    #[error("音声再生エラー: {0}")]
    AudioPlayback(String),

    // ---- GPU ----
    #[error("GPU サーフェス作成失敗: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),

    #[error("GPU アダプター取得失敗: 互換アダプターが見つかりません")]
    NoAdapter,

    #[error("GPU デバイス取得失敗: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    // ---- 物理 (Round 4) ----
    #[error("物理エンジンエラー: {0}")]
    Physics(String),

    // ---- シーンシリアライゼーション (Round 5) ----
    #[error("シーン JSON エラー: {0}")]
    SceneSerialize(String),

    // ---- ウィンドウ / EventLoop ----
    #[error("EventLoop エラー: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error("ウィンドウ作成エラー: {0}")]
    WindowCreation(#[from] winit::error::OsError),
}

/// Thrust エンジンの Result 型エイリアス
pub type ThrustResult<T> = Result<T, ThrustError>;
