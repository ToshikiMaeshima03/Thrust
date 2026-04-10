use std::path::Path;

use super::mesh::Mesh;
use super::vertex::{Vertex, compute_tangents_mikktspace};
use crate::error::{ThrustError, ThrustResult};

/// STL ファイル（バイナリ/ASCII）からメッシュを読み込む
pub fn load_stl(device: &wgpu::Device, path: &Path) -> ThrustResult<Vec<Mesh>> {
    let mut file = std::fs::File::open(path).map_err(|e| ThrustError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let stl = stl_io::read_stl(&mut file).map_err(|e| ThrustError::StlLoad(e.to_string()))?;

    let mut vertices = Vec::with_capacity(stl.faces.len() * 3);
    let mut indices = Vec::with_capacity(stl.faces.len() * 3);

    for (i, face) in stl.faces.iter().enumerate() {
        let normal = [face.normal[0], face.normal[1], face.normal[2]];
        for (j, &vertex_index) in face.vertices.iter().enumerate() {
            let v = stl.vertices[vertex_index];
            vertices.push(Vertex::new([v[0], v[1], v[2]], normal, [0.0, 0.0]));
            indices.push((i * 3 + j) as u32);
        }
    }

    if vertices.is_empty() {
        return Err(ThrustError::EmptyMesh(
            "STLファイルにメッシュが含まれていません".to_string(),
        ));
    }

    let _ = compute_tangents_mikktspace(&mut vertices, &indices);

    Ok(vec![Mesh::new(device, &vertices, &indices)])
}
