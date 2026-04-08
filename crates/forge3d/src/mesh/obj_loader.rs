use std::path::Path;

use super::mesh::Mesh;
use super::vertex::Vertex;

pub fn load_obj(device: &wgpu::Device, path: &Path) -> Result<Vec<Mesh>, String> {
    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    let (models, _materials) =
        tobj::load_obj(path, &load_options).map_err(|e| format!("OBJ読み込みエラー: {e}"))?;

    let mut meshes = Vec::new();

    for model in &models {
        let mesh_data = &model.mesh;
        let num_vertices = mesh_data.positions.len() / 3;
        let has_normals = !mesh_data.normals.is_empty();

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

            vertices.push(Vertex { position, normal });
        }

        // 法線がない場合はフェイス法線を計算
        if !has_normals {
            compute_face_normals(&mut vertices, &mesh_data.indices);
        }

        let mesh = Mesh::new(device, &vertices, &mesh_data.indices);
        meshes.push(mesh);
    }

    if meshes.is_empty() {
        return Err("OBJファイルにメッシュが含まれていません".to_string());
    }

    Ok(meshes)
}

fn compute_face_normals(vertices: &mut [Vertex], indices: &[u32]) {
    // まず全法線をゼロにリセット
    for v in vertices.iter_mut() {
        v.normal = [0.0, 0.0, 0.0];
    }

    // 三角形ごとにフェイス法線を計算して頂点に加算
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = glam::Vec3::from(vertices[i0].position);
        let p1 = glam::Vec3::from(vertices[i1].position);
        let p2 = glam::Vec3::from(vertices[i2].position);

        let edge1 = p1 - p0;
        let edge2 = p2 - p0;
        let face_normal = edge1.cross(edge2);

        for &idx in &[i0, i1, i2] {
            vertices[idx].normal[0] += face_normal.x;
            vertices[idx].normal[1] += face_normal.y;
            vertices[idx].normal[2] += face_normal.z;
        }
    }

    // 正規化
    for v in vertices.iter_mut() {
        let n = glam::Vec3::from(v.normal);
        let normalized = n.normalize_or_zero();
        v.normal = [normalized.x, normalized.y, normalized.z];
    }
}
