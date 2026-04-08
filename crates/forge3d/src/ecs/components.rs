use crate::mesh::mesh::Mesh;

/// メッシュコンポーネント（GPU 上の頂点・インデックスバッファ参照）
pub struct MeshHandle(pub Mesh);

/// エンティティごとの GPU レンダリングステート（エンジン管理）
pub struct RenderState {
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
    pub material_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
}

/// Transform/Material 変更追跡フラグ
pub struct DirtyFlags {
    pub transform: bool,
    pub material: bool,
}

impl Default for DirtyFlags {
    fn default() -> Self {
        Self {
            transform: true,
            material: true,
        }
    }
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

/// アクティブ平行光源マーカー
pub struct ActiveDirectionalLight;

/// アクティブ環境光マーカー
pub struct ActiveAmbientLight;
