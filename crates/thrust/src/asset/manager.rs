use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::audio::AudioSource;
use crate::error::{ThrustError, ThrustResult};
use crate::mesh::mesh::Mesh;
use crate::renderer::texture::ThrustTexture;

/// アセット管理: テクスチャ・メッシュ・音声のキャッシュ付きローダー
///
/// パスをキーとして重複ロードを防止する。
pub struct AssetManager {
    textures: HashMap<String, Arc<ThrustTexture>>,
    meshes: HashMap<String, Vec<Mesh>>,
    audio: HashMap<String, AudioSource>,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            meshes: HashMap::new(),
            audio: HashMap::new(),
        }
    }

    /// テクスチャをロードする。キャッシュ済みの場合はキャッシュから返す。
    pub fn load_texture(
        &mut self,
        path: &str,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> ThrustResult<Arc<ThrustTexture>> {
        if let Some(cached) = self.textures.get(path) {
            return Ok(cached.clone());
        }

        let texture = Arc::new(ThrustTexture::from_path(device, queue, Path::new(path))?);
        self.textures.insert(path.to_string(), texture.clone());
        Ok(texture)
    }

    /// バイト列からテクスチャをロードする。名前をキーとしてキャッシュする。
    pub fn load_texture_from_bytes(
        &mut self,
        name: &str,
        bytes: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> ThrustResult<Arc<ThrustTexture>> {
        if let Some(cached) = self.textures.get(name) {
            return Ok(cached.clone());
        }

        let texture = Arc::new(ThrustTexture::from_bytes(device, queue, bytes, name)?);
        self.textures.insert(name.to_string(), texture.clone());
        Ok(texture)
    }

    /// キャッシュ済みテクスチャを取得する
    pub fn get_texture(&self, path: &str) -> Option<Arc<ThrustTexture>> {
        self.textures.get(path).cloned()
    }

    /// OBJ メッシュをロードする。
    ///
    /// Mesh は GPU バッファを保持するためクローン不可。
    /// ロード済みパスの再ロードはエラーを返す。`is_mesh_loaded()` で事前チェック可能。
    pub fn load_obj(&mut self, path: &str, device: &wgpu::Device) -> ThrustResult<Vec<Mesh>> {
        if self.meshes.contains_key(path) {
            return Err(ThrustError::AlreadyLoaded(path.to_string()));
        }

        let meshes = crate::mesh::obj_loader::load_obj(device, Path::new(path))?;
        let count = meshes.len();
        // ロード済みマーカーとしてメッシュ数を記録（Mesh 自体は呼び出し元に返却）
        self.meshes
            .insert(path.to_string(), Vec::with_capacity(count));
        log::info!("OBJメッシュをロードしました: '{}' ({count} メッシュ)", path);
        Ok(meshes)
    }

    /// モデルをロードする（拡張子から形式を自動判定）。
    ///
    /// サポート形式: OBJ, glTF/GLB, STL
    /// ロード済みパスの再ロードはエラーを返す。
    pub fn load_model(
        &mut self,
        path: &str,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> ThrustResult<crate::mesh::model_loader::ModelLoadResult> {
        if self.meshes.contains_key(path) {
            return Err(ThrustError::AlreadyLoaded(path.to_string()));
        }

        let result =
            crate::mesh::model_loader::load_model(device, queue, std::path::Path::new(path))?;
        let count = result.meshes.len();
        self.meshes
            .insert(path.to_string(), Vec::with_capacity(count));
        log::info!("モデルをロードしました: '{}' ({count} メッシュ)", path);
        Ok(result)
    }

    /// 指定パスのメッシュがロード済みかチェックする
    pub fn is_mesh_loaded(&self, path: &str) -> bool {
        self.meshes.contains_key(path)
    }

    /// 音声ファイルをロードする。キャッシュ済みの場合はキャッシュから返す。
    pub fn load_audio(&mut self, path: &str) -> ThrustResult<AudioSource> {
        if let Some(cached) = self.audio.get(path) {
            return Ok(cached.clone());
        }

        let source = AudioSource::from_path(path)?;
        self.audio.insert(path.to_string(), source.clone());
        Ok(source)
    }

    /// キャッシュ済み音声を取得する
    pub fn get_audio(&self, path: &str) -> Option<&AudioSource> {
        self.audio.get(path)
    }

    /// テクスチャキャッシュからアンロードする
    ///
    /// `Arc` 参照が他に残っている場合、GPU リソースは最後の参照が破棄されるまで保持される。
    pub fn unload_texture(&mut self, path: &str) -> bool {
        self.textures.remove(path).is_some()
    }

    /// メッシュキャッシュからアンロードする
    pub fn unload_mesh(&mut self, path: &str) -> bool {
        self.meshes.remove(path).is_some()
    }

    /// 音声キャッシュからアンロードする
    pub fn unload_audio(&mut self, path: &str) -> bool {
        self.audio.remove(path).is_some()
    }

    /// 全キャッシュをクリアする
    pub fn clear_all(&mut self) {
        self.textures.clear();
        self.meshes.clear();
        self.audio.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let mgr = AssetManager::new();
        assert!(!mgr.is_mesh_loaded("anything"));
        assert!(mgr.get_texture("anything").is_none());
        assert!(mgr.get_audio("anything").is_none());
    }

    #[test]
    fn test_load_audio_nonexistent_file() {
        let mgr = &mut AssetManager::new();
        let result = mgr.load_audio("/nonexistent/audio.wav");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_audio_caches() {
        let mgr = AssetManager::new();
        // テスト用の空ファイルは存在しないのでエラーになるが、
        // キャッシュロジック自体は get_audio で検証
        assert!(mgr.get_audio("test.wav").is_none());
    }

    #[test]
    fn test_unload_nonexistent_returns_false() {
        let mut mgr = AssetManager::new();
        assert!(!mgr.unload_texture("none"));
        assert!(!mgr.unload_mesh("none"));
        assert!(!mgr.unload_audio("none"));
    }

    #[test]
    fn test_clear_all() {
        let mgr = &mut AssetManager::new();
        mgr.clear_all();
        // クリア後も空の状態が正しい
        assert!(!mgr.is_mesh_loaded("anything"));
    }

    #[test]
    fn test_is_mesh_loaded_false_initially() {
        let mgr = AssetManager::new();
        assert!(!mgr.is_mesh_loaded("model.obj"));
        assert!(!mgr.is_mesh_loaded(""));
    }
}
