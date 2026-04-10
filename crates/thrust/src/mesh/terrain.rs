//! ハイトマップ地形 (Round 6)
//!
//! 2D ハイトマップ配列から XZ グリッド上の三角形メッシュを生成する。
//! 法線は自動計算、UV は (x/width, z/depth)。

use super::mesh::Mesh;
use super::vertex::{Vertex, compute_face_normals, compute_tangents_mikktspace};

/// ハイトマップから地形メッシュを生成する
///
/// - `heights`: 高さ値の 2D 配列 (row-major: heights[z * width + x])
/// - `width` / `depth`: グリッド解像度
/// - `cell_size`: 1 セルのワールドスケール
///
/// グリッドの中心が (0, 0, 0) になるよう配置される。
///
/// # パニック
/// `heights.len() != width * depth` のときパニックする。
pub fn create_terrain_from_heightmap(
    device: &wgpu::Device,
    heights: &[f32],
    width: usize,
    depth: usize,
    cell_size: f32,
) -> Mesh {
    assert_eq!(
        heights.len(),
        width * depth,
        "heightmap size mismatch: expected {} got {}",
        width * depth,
        heights.len()
    );
    assert!(width >= 2 && depth >= 2, "terrain requires at least 2x2");

    let mut vertices = Vec::with_capacity(width * depth);
    let half_w = (width as f32 - 1.0) * cell_size * 0.5;
    let half_d = (depth as f32 - 1.0) * cell_size * 0.5;

    for z in 0..depth {
        for x in 0..width {
            let h = heights[z * width + x];
            let pos = [
                x as f32 * cell_size - half_w,
                h,
                z as f32 * cell_size - half_d,
            ];
            let u = x as f32 / (width as f32 - 1.0);
            let v = z as f32 / (depth as f32 - 1.0);
            vertices.push(Vertex::new(pos, [0.0, 1.0, 0.0], [u, v]));
        }
    }

    // インデックス: 各グリッドセルを 2 三角形に
    let mut indices: Vec<u32> = Vec::with_capacity((width - 1) * (depth - 1) * 6);
    for z in 0..depth - 1 {
        for x in 0..width - 1 {
            let tl = (z * width + x) as u32;
            let tr = tl + 1;
            let bl = ((z + 1) * width + x) as u32;
            let br = bl + 1;
            // 三角形 1: tl, bl, br (CCW from above)
            indices.push(tl);
            indices.push(bl);
            indices.push(br);
            // 三角形 2: tl, br, tr
            indices.push(tl);
            indices.push(br);
            indices.push(tr);
        }
    }

    // 法線を計算
    compute_face_normals(&mut vertices, &indices);
    let _ = compute_tangents_mikktspace(&mut vertices, &indices);

    Mesh::new(device, &vertices, &indices)
}

/// 正弦波ベースのプロシージャルハイトマップを生成する (デモ用)
pub fn sine_heightmap(width: usize, depth: usize, amplitude: f32, frequency: f32) -> Vec<f32> {
    let mut h = Vec::with_capacity(width * depth);
    for z in 0..depth {
        for x in 0..width {
            let fx = x as f32 * frequency;
            let fz = z as f32 * frequency;
            h.push(amplitude * (fx.sin() + fz.cos()));
        }
    }
    h
}

/// シンプルな 2D Perlin 風ノイズハイトマップ (デモ用、疑似)
pub fn noise_heightmap(
    width: usize,
    depth: usize,
    amplitude: f32,
    scale: f32,
    seed: u32,
) -> Vec<f32> {
    let mut rng = crate::math::SimpleRng::new(seed);
    // ローパス擬似ノイズ: セルごとに乱数生成 + smoothstep 補間
    let grid_res = 8;
    let coarse: Vec<f32> = (0..(grid_res * grid_res))
        .map(|_| rng.next_f32() * 2.0 - 1.0)
        .collect();

    let mut heights = Vec::with_capacity(width * depth);
    for z in 0..depth {
        for x in 0..width {
            let fx = (x as f32 / width as f32) * grid_res as f32 * scale;
            let fz = (z as f32 / depth as f32) * grid_res as f32 * scale;
            let ix = (fx as usize).min(grid_res - 1);
            let iz = (fz as usize).min(grid_res - 1);
            let v = coarse[iz * grid_res + ix];
            heights.push(v * amplitude);
        }
    }
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sine_heightmap_size() {
        let h = sine_heightmap(10, 10, 1.0, 0.1);
        assert_eq!(h.len(), 100);
    }

    #[test]
    fn test_noise_heightmap_size() {
        let h = noise_heightmap(20, 15, 2.0, 1.0, 42);
        assert_eq!(h.len(), 300);
    }

    #[test]
    fn test_noise_heightmap_deterministic() {
        let h1 = noise_heightmap(10, 10, 1.0, 1.0, 123);
        let h2 = noise_heightmap(10, 10, 1.0, 1.0, 123);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_noise_heightmap_different_seeds() {
        let h1 = noise_heightmap(10, 10, 1.0, 1.0, 1);
        let h2 = noise_heightmap(10, 10, 1.0, 1.0, 2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_sine_heightmap_amplitude_zero() {
        let h = sine_heightmap(10, 10, 0.0, 0.1);
        for v in h {
            assert_eq!(v, 0.0);
        }
    }
}
