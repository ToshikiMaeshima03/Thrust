use std::path::Path;

use super::mesh::Mesh;
use crate::error::{ThrustError, ThrustResult};
use crate::material::material::Material;

/// モデルの読み込み結果
pub struct ModelLoadResult {
    /// メッシュリスト
    pub meshes: Vec<Mesh>,
    /// メッシュに対応するマテリアル（meshes と同じ長さ）
    pub materials: Vec<Material>,
}

/// 拡張子から適切なローダーにディスパッチしてモデルを読み込む
///
/// サポート形式:
/// - `.obj` — Wavefront OBJ
/// - `.gltf` / `.glb` — glTF 2.0
/// - `.stl` — STL (バイナリ/ASCII)
pub fn load_model(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    path: &Path,
) -> ThrustResult<ModelLoadResult> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "obj" => {
            let meshes = super::obj_loader::load_obj(device, path)?;
            let materials = meshes.iter().map(|_| Material::default()).collect();
            Ok(ModelLoadResult { meshes, materials })
        }
        "gltf" | "glb" => {
            let result = super::gltf_loader::load_gltf(device, queue, path)?;
            Ok(ModelLoadResult {
                meshes: result.meshes,
                materials: result.materials,
            })
        }
        "stl" => {
            let meshes = super::stl_loader::load_stl(device, path)?;
            let materials = meshes.iter().map(|_| Material::default()).collect();
            Ok(ModelLoadResult { meshes, materials })
        }
        _ => Err(ThrustError::UnsupportedFormat(ext)),
    }
}
