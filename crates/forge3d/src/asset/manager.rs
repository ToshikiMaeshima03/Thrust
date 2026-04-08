use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::audio::AudioSource;
use crate::mesh::mesh::Mesh;
use crate::renderer::texture::ForgeTexture;

/// アセット管理: テクスチャ・メッシュ・音声のキャッシュ付きローダー
///
/// パスをキーとして重複ロードを防止する。
pub struct AssetManager {
    textures: HashMap<String, Arc<ForgeTexture>>,
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
    ) -> Result<Arc<ForgeTexture>, String> {
        if let Some(cached) = self.textures.get(path) {
            return Ok(cached.clone());
        }

        let texture = Arc::new(ForgeTexture::from_path(device, queue, Path::new(path))?);
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
    ) -> Result<Arc<ForgeTexture>, String> {
        if let Some(cached) = self.textures.get(name) {
            return Ok(cached.clone());
        }

        let texture = Arc::new(ForgeTexture::from_bytes(device, queue, bytes, name)?);
        self.textures.insert(name.to_string(), texture.clone());
        Ok(texture)
    }

    /// キャッシュ済みテクスチャを取得する
    pub fn get_texture(&self, path: &str) -> Option<Arc<ForgeTexture>> {
        self.textures.get(path).cloned()
    }

    /// OBJ メッシュをロードしてキャッシュから取得する。
    ///
    /// 注意: Mesh は GPU バッファを保持するためクローン不可。
    /// 2回目以降のロードは空の Vec を返す（最初のロードでメッシュを取り出す設計）。
    /// メッシュの共有が必要な場合は Arc<Mesh> を検討する。
    pub fn load_obj(&mut self, path: &str, device: &wgpu::Device) -> Result<Vec<Mesh>, String> {
        if self.meshes.contains_key(path) {
            log::warn!("メッシュ '{}' は既にロード済みです", path);
            return Ok(Vec::new());
        }

        let meshes = crate::mesh::obj_loader::load_obj(device, Path::new(path))?;
        // キャッシュキーを記録（再ロード防止）
        self.meshes.insert(path.to_string(), Vec::new());
        Ok(meshes)
    }

    /// 音声ファイルをロードする。キャッシュ済みの場合はキャッシュから返す。
    pub fn load_audio(&mut self, path: &str) -> Result<AudioSource, String> {
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
}
