use crate::mesh::mesh::Mesh;

/// メッシュコンポーネント（GPU 上の頂点・インデックスバッファ参照）
pub struct MeshHandle(pub Mesh);

/// エンティティごとの GPU レンダリングステート（エンジン管理）
pub struct RenderState {
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
    pub material_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
    /// Round 4: スケルタル用 joint matrices buffer (非スキンはフォールバック共有)
    pub joint_buffer: wgpu::Buffer,
    /// このエンティティが固有 joint buffer を持っているか (false = フォールバック共有)
    pub owns_joint_buffer: bool,
}

/// エンティティの表示/非表示制御
pub struct Visible(pub bool);

impl Default for Visible {
    fn default() -> Self {
        Self(true)
    }
}

/// デバッグ用エンティティ名
pub struct Name(pub String);

/// アクティブカメラマーカー
pub struct ActiveCamera;

/// アクティブ環境光マーカー
pub struct ActiveAmbientLight;

/// 旧 API 互換マーカー (Round 4 では全 directional light が自動収集される)
///
/// 残しておくと既存ユーザーコードがコンパイルできる。
pub struct ActiveDirectionalLight;
