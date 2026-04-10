use std::path::Path;
use std::sync::Arc;

use super::mesh::Mesh;
use super::vertex::{Vertex, compute_face_normals, compute_tangents_mikktspace};
use crate::animation::keyframe::{KeyframeTrack, KeyframeValues};
use crate::error::{ThrustError, ThrustResult};
use crate::material::material::Material;
use crate::renderer::texture::ThrustTexture;

/// glTF/GLB 読み込み結果
pub struct GltfLoadResult {
    /// メッシュリスト
    pub meshes: Vec<Mesh>,
    /// メッシュに対応するマテリアル（meshes と同じ長さ）
    pub materials: Vec<Material>,
    /// glTF アニメーションデータ
    pub animations: Vec<GltfAnimationData>,
}

/// glTF から抽出されたアニメーションデータ
pub struct GltfAnimationData {
    /// アニメーション名
    pub name: String,
    /// キーフレームトラック
    pub tracks: Vec<KeyframeTrack>,
    /// アニメーション全体の長さ（秒）
    pub duration: f32,
}

/// glTF/GLB ファイルからメッシュ・マテリアル・アニメーションを読み込む
pub fn load_gltf(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    path: &Path,
) -> ThrustResult<GltfLoadResult> {
    let (document, buffers, images) = gltf::import(path)?;

    let mut meshes = Vec::new();
    let mut materials = Vec::new();

    // テクスチャキャッシュ（同一テクスチャの重複作成を防止）
    let mut texture_cache: Vec<Option<Arc<ThrustTexture>>> = Vec::new();
    for (i, img_data) in images.iter().enumerate() {
        let rgba = convert_gltf_image_to_rgba(img_data);
        match ThrustTexture::from_rgba_data(
            device,
            queue,
            &rgba,
            img_data.width,
            img_data.height,
            &format!("glTFテクスチャ {i}"),
        ) {
            Ok(tex) => texture_cache.push(Some(Arc::new(tex))),
            Err(e) => {
                log::warn!("glTFテクスチャ {i} の作成に失敗: {e}");
                texture_cache.push(None);
            }
        }
    }

    for gltf_mesh in document.meshes() {
        for primitive in gltf_mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            // positions (必須)
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| {
                    ThrustError::EmptyMesh("glTFメッシュに位置データがありません".to_string())
                })?
                .collect();

            // normals (任意)
            let has_normals = reader.read_normals().is_some();
            let normals: Vec<[f32; 3]> = primitive
                .reader(|buffer| Some(&buffers[buffer.index()]))
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            // tex_coords (任意)
            let tex_coords: Vec<[f32; 2]> = primitive
                .reader(|buffer| Some(&buffers[buffer.index()]))
                .read_tex_coords(0)
                .map(|iter| iter.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            // tangents (任意)
            let has_tangents = primitive
                .reader(|buffer| Some(&buffers[buffer.index()]))
                .read_tangents()
                .is_some();
            let tangents: Vec<[f32; 4]> = primitive
                .reader(|buffer| Some(&buffers[buffer.index()]))
                .read_tangents()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; positions.len()]);

            // 頂点を構築
            let mut vertices: Vec<Vertex> = positions
                .iter()
                .enumerate()
                .map(|(i, pos)| {
                    let mut v = Vertex::new(*pos, normals[i], tex_coords[i]);
                    v.tangent = tangents[i];
                    v
                })
                .collect();

            // インデックス
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|iter| iter.into_u32().collect())
                .unwrap_or_else(|| (0..vertices.len() as u32).collect());

            // 法線がない場合はフェイス法線を計算
            if !has_normals {
                compute_face_normals(&mut vertices, &indices);
            }
            // tangent がない場合は mikktspace で生成
            if !has_tangents {
                let _ = compute_tangents_mikktspace(&mut vertices, &indices);
            }

            let mesh = Mesh::new(device, &vertices, &indices);
            meshes.push(mesh);

            // マテリアル抽出
            let material = extract_material(&primitive, &texture_cache);
            materials.push(material);
        }
    }

    // アニメーション抽出
    let animations = extract_animations(&document, &buffers);

    if meshes.is_empty() {
        return Err(ThrustError::EmptyMesh(
            "glTFファイルにメッシュが含まれていません".to_string(),
        ));
    }

    Ok(GltfLoadResult {
        meshes,
        materials,
        animations,
    })
}

/// glTF 画像データを RGBA8 に変換する
fn convert_gltf_image_to_rgba(img_data: &gltf::image::Data) -> Vec<u8> {
    match img_data.format {
        gltf::image::Format::R8G8B8A8 => img_data.pixels.clone(),
        gltf::image::Format::R8G8B8 => {
            // RGB → RGBA 変換
            let pixel_count = img_data.pixels.len() / 3;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in img_data.pixels.chunks(3) {
                rgba.push(chunk[0]);
                rgba.push(chunk[1]);
                rgba.push(chunk[2]);
                rgba.push(255);
            }
            rgba
        }
        gltf::image::Format::R16G16B16A16 => {
            // 16bit → 8bit 変換
            let pixel_count = img_data.pixels.len() / 8;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in img_data.pixels.chunks(8) {
                // 上位バイトを使用（リトルエンディアン）
                rgba.push(chunk[1]);
                rgba.push(chunk[3]);
                rgba.push(chunk[5]);
                rgba.push(chunk[7]);
            }
            rgba
        }
        gltf::image::Format::R16G16B16 => {
            let pixel_count = img_data.pixels.len() / 6;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in img_data.pixels.chunks(6) {
                rgba.push(chunk[1]);
                rgba.push(chunk[3]);
                rgba.push(chunk[5]);
                rgba.push(255);
            }
            rgba
        }
        _ => {
            // その他のフォーマット: 白ピクセルにフォールバック
            log::warn!(
                "未対応のglTF画像フォーマット: {:?}、白テクスチャにフォールバック",
                img_data.format
            );
            vec![
                255u8;
                (img_data.width as usize)
                    .saturating_mul(img_data.height as usize)
                    .saturating_mul(4)
            ]
        }
    }
}

/// プリミティブから PBR マテリアルを抽出する
fn extract_material(
    primitive: &gltf::Primitive<'_>,
    texture_cache: &[Option<Arc<ThrustTexture>>],
) -> Material {
    let gltf_mat = primitive.material();
    let pbr = gltf_mat.pbr_metallic_roughness();
    let [r, g, b, a] = pbr.base_color_factor();
    let metallic = pbr.metallic_factor();
    let roughness = pbr.roughness_factor();
    let [er, eg, eb] = gltf_mat.emissive_factor();
    let normal_scale = gltf_mat
        .normal_texture()
        .map(|nt| nt.scale())
        .unwrap_or(1.0);
    let occlusion_strength = gltf_mat
        .occlusion_texture()
        .map(|ot| ot.strength())
        .unwrap_or(1.0);

    let lookup = |idx: usize| texture_cache.get(idx).and_then(|t| t.clone());

    let base_color_map = pbr
        .base_color_texture()
        .and_then(|info| lookup(info.texture().source().index()));
    let metallic_roughness_map = pbr
        .metallic_roughness_texture()
        .and_then(|info| lookup(info.texture().source().index()));
    let normal_map = gltf_mat
        .normal_texture()
        .and_then(|nt| lookup(nt.texture().source().index()));
    let occlusion_map = gltf_mat
        .occlusion_texture()
        .and_then(|ot| lookup(ot.texture().source().index()));
    let emissive_map = gltf_mat
        .emissive_texture()
        .and_then(|info| lookup(info.texture().source().index()));

    Material {
        base_color_factor: glam::Vec4::new(r, g, b, a),
        metallic_factor: metallic,
        roughness_factor: roughness,
        emissive_factor: glam::Vec3::new(er, eg, eb),
        normal_scale,
        occlusion_strength,
        base_color_map,
        metallic_roughness_map,
        normal_map,
        occlusion_map,
        emissive_map,
        ..Default::default()
    }
}

/// glTF からアニメーションデータを抽出する
fn extract_animations(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Vec<GltfAnimationData> {
    let mut animations = Vec::new();

    for anim in document.animations() {
        let name = anim.name().unwrap_or("unnamed").to_string();
        let mut tracks = Vec::new();
        let mut max_time: f32 = 0.0;

        for channel in anim.channels() {
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));

            let timestamps: Vec<f32> = match reader.read_inputs() {
                Some(iter) => iter.collect(),
                None => continue,
            };

            if let Some(&last) = timestamps.last() {
                max_time = max_time.max(last);
            }

            let outputs = match reader.read_outputs() {
                Some(outputs) => outputs,
                None => continue,
            };

            let property = channel.target().property();

            match property {
                gltf::animation::Property::Translation => {
                    if let gltf::animation::util::ReadOutputs::Translations(iter) = outputs {
                        let vals: Vec<glam::Vec3> = iter.map(glam::Vec3::from).collect();
                        tracks.push(KeyframeTrack {
                            timestamps,
                            values: KeyframeValues::Translation(vals),
                        });
                    }
                }
                gltf::animation::Property::Rotation => {
                    if let gltf::animation::util::ReadOutputs::Rotations(rotations) = outputs {
                        let vals: Vec<glam::Quat> = rotations
                            .into_f32()
                            .map(|[x, y, z, w]| glam::Quat::from_xyzw(x, y, z, w))
                            .collect();
                        tracks.push(KeyframeTrack {
                            timestamps,
                            values: KeyframeValues::Rotation(vals),
                        });
                    }
                }
                gltf::animation::Property::Scale => {
                    if let gltf::animation::util::ReadOutputs::Scales(iter) = outputs {
                        let vals: Vec<glam::Vec3> = iter.map(glam::Vec3::from).collect();
                        tracks.push(KeyframeTrack {
                            timestamps,
                            values: KeyframeValues::Scale(vals),
                        });
                    }
                }
                gltf::animation::Property::MorphTargetWeights => {
                    // モーフターゲットは未サポート
                }
            }
        }

        if !tracks.is_empty() {
            animations.push(GltfAnimationData {
                name,
                tracks,
                duration: max_time,
            });
        }
    }

    animations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_image_data(
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: gltf::image::Format,
    ) -> gltf::image::Data {
        gltf::image::Data {
            pixels,
            width,
            height,
            format,
        }
    }

    #[test]
    fn test_convert_rgba8_passthrough() {
        let pixels = vec![255, 0, 0, 128, 0, 255, 0, 255];
        let data = make_image_data(pixels.clone(), 2, 1, gltf::image::Format::R8G8B8A8);
        let rgba = convert_gltf_image_to_rgba(&data);
        assert_eq!(rgba, pixels);
    }

    #[test]
    fn test_convert_rgb8_to_rgba8() {
        let pixels = vec![255, 0, 0, 0, 255, 0];
        let data = make_image_data(pixels, 2, 1, gltf::image::Format::R8G8B8);
        let rgba = convert_gltf_image_to_rgba(&data);
        assert_eq!(rgba, vec![255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn test_convert_r16g16b16a16_to_rgba8() {
        let pixels = vec![0, 200, 0, 100, 0, 50, 0, 255];
        let data = make_image_data(pixels, 1, 1, gltf::image::Format::R16G16B16A16);
        let rgba = convert_gltf_image_to_rgba(&data);
        assert_eq!(rgba, vec![200, 100, 50, 255]);
    }

    #[test]
    fn test_convert_r16g16b16_to_rgba8() {
        let pixels = vec![0, 128, 0, 64, 0, 32];
        let data = make_image_data(pixels, 1, 1, gltf::image::Format::R16G16B16);
        let rgba = convert_gltf_image_to_rgba(&data);
        assert_eq!(rgba, vec![128, 64, 32, 255]);
    }

    #[test]
    fn test_convert_unknown_format_fallback() {
        let pixels = vec![1, 2];
        let data = make_image_data(pixels, 1, 1, gltf::image::Format::R8);
        let rgba = convert_gltf_image_to_rgba(&data);
        assert_eq!(rgba, vec![255, 255, 255, 255]);
    }
}
