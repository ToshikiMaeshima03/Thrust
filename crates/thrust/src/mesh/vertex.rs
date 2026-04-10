use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// 頂点フォーマット (Round 4: tangent + skinning 対応)
///
/// レイアウト (offset / size):
/// - 0   : position [f32; 3]   (12 B)
/// - 12  : normal   [f32; 3]   (12 B)
/// - 24  : tangent  [f32; 4]   (16 B, w = handedness)
/// - 40  : uv       [f32; 2]   ( 8 B)
/// - 48  : joints   [u16; 4]   ( 8 B, スキンメッシュ用)
/// - 56  : weights  [f32; 4]   (16 B, スキンメッシュ用、unskinned は全て 0)
///
/// 合計: 72 B → 16 バイトアラインメントのため 80 B にパディング
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coords: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
    /// 80 B にするためのパディング
    pub _padding: [f32; 2],
}

impl Vertex {
    /// 位置・法線・UV のみから頂点を構築する（tangent ゼロ、unskinned）
    pub fn new(position: [f32; 3], normal: [f32; 3], tex_coords: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coords,
            joints: [0; 4],
            weights: [0.0; 4],
            _padding: [0.0; 2],
        }
    }

    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // tangent
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // tex_coords
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // joints (4 * u16)
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint16x4,
                },
                // weights
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// 法線がないメッシュにフェイス法線を計算して設定する
pub fn compute_face_normals(vertices: &mut [Vertex], indices: &[u32]) {
    // まず全法線をゼロにリセット
    for v in vertices.iter_mut() {
        v.normal = [0.0, 0.0, 0.0];
    }

    // 三角形ごとにフェイス法線を計算して頂点に加算
    let vlen = vertices.len();
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        // インデックスが頂点数を超えている場合はスキップ
        if i0 >= vlen || i1 >= vlen || i2 >= vlen {
            continue;
        }

        let p0 = Vec3::from(vertices[i0].position);
        let p1 = Vec3::from(vertices[i1].position);
        let p2 = Vec3::from(vertices[i2].position);

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
        let n = Vec3::from(v.normal);
        let normalized = n.normalize_or_zero();
        v.normal = [normalized.x, normalized.y, normalized.z];
    }
}

/// mikktspace を使ってタンジェントを生成する
///
/// インデックス付き三角形メッシュを想定。
/// 既に tangent が入っている場合は上書きする。
pub fn compute_tangents_mikktspace(vertices: &mut [Vertex], indices: &[u32]) -> bool {
    let mut geom = MikkGeom { vertices, indices };
    mikktspace::generate_tangents(&mut geom)
}

struct MikkGeom<'a> {
    vertices: &'a mut [Vertex],
    indices: &'a [u32],
}

impl<'a> mikktspace::Geometry for MikkGeom<'a> {
    fn num_faces(&self) -> usize {
        self.indices.len() / 3
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        let idx = self.indices[face * 3 + vert] as usize;
        self.vertices[idx].position
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        let idx = self.indices[face * 3 + vert] as usize;
        self.vertices[idx].normal
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        let idx = self.indices[face * 3 + vert] as usize;
        self.vertices[idx].tex_coords
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        let idx = self.indices[face * 3 + vert] as usize;
        self.vertices[idx].tangent = tangent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vertex(pos: [f32; 3]) -> Vertex {
        Vertex::new(pos, [0.0; 3], [0.0; 2])
    }

    #[test]
    fn test_vertex_size_alignment() {
        // 80 B、16 B 倍数
        assert_eq!(std::mem::size_of::<Vertex>(), 80);
        assert_eq!(std::mem::size_of::<Vertex>() % 16, 0);
    }

    #[test]
    fn test_compute_face_normals_single_triangle() {
        // XY 平面上の三角形 → 法線は +Z 方向
        let mut vertices = vec![
            make_vertex([0.0, 0.0, 0.0]),
            make_vertex([1.0, 0.0, 0.0]),
            make_vertex([0.0, 1.0, 0.0]),
        ];
        let indices = vec![0, 1, 2];

        compute_face_normals(&mut vertices, &indices);

        for v in &vertices {
            let n = Vec3::from(v.normal);
            assert!(n.length() > 0.99, "法線がゼロ");
            assert!((n.z - 1.0).abs() < 1e-5, "法線が +Z ではない: {n:?}");
        }
    }

    #[test]
    fn test_compute_face_normals_normalized() {
        let mut vertices = vec![
            make_vertex([0.0, 0.0, 0.0]),
            make_vertex([10.0, 0.0, 0.0]),
            make_vertex([0.0, 10.0, 0.0]),
        ];
        let indices = vec![0, 1, 2];

        compute_face_normals(&mut vertices, &indices);

        for v in &vertices {
            let n = Vec3::from(v.normal);
            assert!(
                (n.length() - 1.0).abs() < 1e-5,
                "法線が正規化されていない: {n:?}"
            );
        }
    }

    #[test]
    fn test_compute_face_normals_out_of_bounds_index() {
        // 不正なインデックス → パニックせずスキップ
        let mut vertices = vec![make_vertex([0.0, 0.0, 0.0]), make_vertex([1.0, 0.0, 0.0])];
        let indices = vec![0, 1, 99]; // 99 は範囲外

        compute_face_normals(&mut vertices, &indices);

        // パニックしないことが主要テスト
        for v in &vertices {
            let n = Vec3::from(v.normal);
            assert_eq!(n, Vec3::ZERO); // スキップされたので法線はゼロ
        }
    }

    #[test]
    fn test_compute_face_normals_incomplete_triangle() {
        let mut vertices = vec![make_vertex([0.0, 0.0, 0.0]), make_vertex([1.0, 0.0, 0.0])];
        let indices = vec![0, 1]; // 2個しかない → chunks(3) でスキップ

        compute_face_normals(&mut vertices, &indices);
        // パニックしない
    }

    #[test]
    fn test_compute_face_normals_shared_vertex() {
        // 2つの三角形が頂点 0 を共有
        let mut vertices = vec![
            make_vertex([0.0, 0.0, 0.0]),
            make_vertex([1.0, 0.0, 0.0]),
            make_vertex([0.0, 1.0, 0.0]),
            make_vertex([-1.0, 0.0, 0.0]),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];

        compute_face_normals(&mut vertices, &indices);

        // 共有頂点 0 の法線は 2 つの面法線の合成（正規化後は +Z）
        let n0 = Vec3::from(vertices[0].normal);
        assert!((n0.z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_buffer_layout_attributes() {
        let layout = Vertex::buffer_layout();
        assert_eq!(layout.array_stride, 80);
        // 6 つの属性
        assert_eq!(layout.attributes.len(), 6);
        // shader_location が 0..=5
        for (i, attr) in layout.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, i as u32);
        }
    }
}
