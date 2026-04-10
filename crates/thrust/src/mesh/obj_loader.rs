use std::path::Path;

use super::mesh::Mesh;
use super::vertex::{Vertex, compute_face_normals, compute_tangents_mikktspace};
use crate::error::{ThrustError, ThrustResult};

pub fn load_obj(device: &wgpu::Device, path: &Path) -> ThrustResult<Vec<Mesh>> {
    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    let (models, _materials) = tobj::load_obj(path, &load_options)?;

    let mut meshes = Vec::new();

    for model in &models {
        let mesh_data = &model.mesh;
        let num_vertices = mesh_data.positions.len() / 3;
        let has_normals = !mesh_data.normals.is_empty();
        let has_texcoords = !mesh_data.texcoords.is_empty();

        let mut vertices = Vec::with_capacity(num_vertices);

        for i in 0..num_vertices {
            let position = [
                mesh_data.positions[i * 3],
                mesh_data.positions[i * 3 + 1],
                mesh_data.positions[i * 3 + 2],
            ];

            let normal = if has_normals {
                [
                    mesh_data.normals[i * 3],
                    mesh_data.normals[i * 3 + 1],
                    mesh_data.normals[i * 3 + 2],
                ]
            } else {
                [0.0, 1.0, 0.0]
            };

            let tex_coords = if has_texcoords {
                [mesh_data.texcoords[i * 2], mesh_data.texcoords[i * 2 + 1]]
            } else {
                [0.0, 0.0]
            };

            vertices.push(Vertex::new(position, normal, tex_coords));
        }

        // 法線がない場合はフェイス法線を計算
        if !has_normals {
            compute_face_normals(&mut vertices, &mesh_data.indices);
        }

        // タンジェント生成
        let _ = compute_tangents_mikktspace(&mut vertices, &mesh_data.indices);

        let mesh = Mesh::new(device, &vertices, &mesh_data.indices);
        meshes.push(mesh);
    }

    if meshes.is_empty() {
        return Err(ThrustError::EmptyMesh(
            "OBJファイルにメッシュが含まれていません".to_string(),
        ));
    }

    Ok(meshes)
}
