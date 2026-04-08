use super::mesh::Mesh;
use super::vertex::Vertex;

/// Y-up 水平面を生成
pub fn create_plane(device: &wgpu::Device, size: f32) -> Mesh {
    let h = size / 2.0;
    let vertices = vec![
        Vertex {
            position: [-h, 0.0, -h],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.0, 0.0],
        },
        Vertex {
            position: [h, 0.0, -h],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [1.0, 0.0],
        },
        Vertex {
            position: [h, 0.0, h],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [1.0, 1.0],
        },
        Vertex {
            position: [-h, 0.0, h],
            normal: [0.0, 1.0, 0.0],
            tex_coords: [0.0, 1.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    Mesh::new(device, &vertices, &indices)
}

/// Z-forward 垂直クアッドを生成
pub fn create_quad(device: &wgpu::Device, width: f32, height: f32) -> Mesh {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let vertices = vec![
        Vertex {
            position: [-hw, -hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 1.0],
        },
        Vertex {
            position: [hw, -hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 1.0],
        },
        Vertex {
            position: [hw, hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 0.0],
        },
        Vertex {
            position: [-hw, hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 0.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    Mesh::new(device, &vertices, &indices)
}

/// キューブを生成（24 頂点、面ごとに法線分離）
pub fn create_cube(device: &wgpu::Device, size: f32) -> Mesh {
    let h = size / 2.0;

    #[rustfmt::skip]
    let vertices = vec![
        // 前面 (+Z)
        Vertex { position: [-h, -h,  h], normal: [0.0, 0.0, 1.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [ h, -h,  h], normal: [0.0, 0.0, 1.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [ h,  h,  h], normal: [0.0, 0.0, 1.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [-h,  h,  h], normal: [0.0, 0.0, 1.0], tex_coords: [0.0, 0.0] },
        // 背面 (-Z)
        Vertex { position: [ h, -h, -h], normal: [0.0, 0.0, -1.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [-h, -h, -h], normal: [0.0, 0.0, -1.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [-h,  h, -h], normal: [0.0, 0.0, -1.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [ h,  h, -h], normal: [0.0, 0.0, -1.0], tex_coords: [0.0, 0.0] },
        // 右面 (+X)
        Vertex { position: [ h, -h,  h], normal: [1.0, 0.0, 0.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [ h, -h, -h], normal: [1.0, 0.0, 0.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [ h,  h, -h], normal: [1.0, 0.0, 0.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [ h,  h,  h], normal: [1.0, 0.0, 0.0], tex_coords: [0.0, 0.0] },
        // 左面 (-X)
        Vertex { position: [-h, -h, -h], normal: [-1.0, 0.0, 0.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [-h, -h,  h], normal: [-1.0, 0.0, 0.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [-h,  h,  h], normal: [-1.0, 0.0, 0.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [-h,  h, -h], normal: [-1.0, 0.0, 0.0], tex_coords: [0.0, 0.0] },
        // 上面 (+Y)
        Vertex { position: [-h,  h,  h], normal: [0.0, 1.0, 0.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [ h,  h,  h], normal: [0.0, 1.0, 0.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [ h,  h, -h], normal: [0.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [-h,  h, -h], normal: [0.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },
        // 下面 (-Y)
        Vertex { position: [-h, -h, -h], normal: [0.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
        Vertex { position: [ h, -h, -h], normal: [0.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
        Vertex { position: [ h, -h,  h], normal: [0.0, -1.0, 0.0], tex_coords: [1.0, 0.0] },
        Vertex { position: [-h, -h,  h], normal: [0.0, -1.0, 0.0], tex_coords: [0.0, 0.0] },
    ];

    #[rustfmt::skip]
    let indices: Vec<u32> = vec![
         0,  1,  2,  0,  2,  3, // 前面
         4,  5,  6,  4,  6,  7, // 背面
         8,  9, 10,  8, 10, 11, // 右面
        12, 13, 14, 12, 14, 15, // 左面
        16, 17, 18, 16, 18, 19, // 上面
        20, 21, 22, 20, 22, 23, // 下面
    ];

    Mesh::new(device, &vertices, &indices)
}

/// UV 球を生成
pub fn create_sphere(device: &wgpu::Device, radius: f32, segments: u32, rings: u32) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for ring in 0..=rings {
        let phi = std::f32::consts::PI * ring as f32 / rings as f32;
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for seg in 0..=segments {
            let theta = 2.0 * std::f32::consts::PI * seg as f32 / segments as f32;
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            let x = sin_phi * cos_theta;
            let y = cos_phi;
            let z = sin_phi * sin_theta;

            vertices.push(Vertex {
                position: [x * radius, y * radius, z * radius],
                normal: [x, y, z],
                tex_coords: [seg as f32 / segments as f32, ring as f32 / rings as f32],
            });
        }
    }

    for ring in 0..rings {
        for seg in 0..segments {
            let current = ring * (segments + 1) + seg;
            let next = current + segments + 1;

            indices.push(current);
            indices.push(next);
            indices.push(current + 1);

            indices.push(current + 1);
            indices.push(next);
            indices.push(next + 1);
        }
    }

    Mesh::new(device, &vertices, &indices)
}
