//! レベルストリーミング (Round 8)
//!
//! ワールドをチャンクに分割し、カメラ位置に応じてロード/アンロードする。
//! 各チャンクは `SerScene` として保存されており、ロード時に entity を spawn する。
//!
//! 用途: 大規模オープンワールド、ストリーミング的なシーン構築。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use glam::{IVec2, Vec3};
use hecs::{Entity, World};

use crate::error::ThrustResult;
use crate::scene::serialize::SerScene;

/// チャンク座標 (XZ グリッド)
pub type ChunkCoord = IVec2;

/// チャンクマネージャー
pub struct StreamingWorld {
    /// チャンク 1 辺の長さ (m)
    pub chunk_size: f32,
    /// ロード半径 (チャンク数)
    pub load_radius: i32,
    /// チャンクファイルへのベースパス (`{base}_{x}_{z}.json`)
    pub chunk_base_path: PathBuf,
    /// 現在ロード済みのチャンク (チャンク座標 → 生成されたエンティティ群)
    loaded_chunks: HashMap<ChunkCoord, Vec<Entity>>,
}

impl StreamingWorld {
    pub fn new(chunk_size: f32, load_radius: i32, chunk_base_path: impl Into<PathBuf>) -> Self {
        Self {
            chunk_size,
            load_radius,
            chunk_base_path: chunk_base_path.into(),
            loaded_chunks: HashMap::new(),
        }
    }

    /// ワールド位置からチャンク座標に変換
    pub fn world_to_chunk(&self, world_pos: Vec3) -> ChunkCoord {
        ChunkCoord::new(
            (world_pos.x / self.chunk_size).floor() as i32,
            (world_pos.z / self.chunk_size).floor() as i32,
        )
    }

    /// 必要なチャンク座標の集合を計算する
    pub fn chunks_in_range(&self, center: ChunkCoord) -> HashSet<ChunkCoord> {
        let mut out = HashSet::new();
        for dx in -self.load_radius..=self.load_radius {
            for dz in -self.load_radius..=self.load_radius {
                if dx * dx + dz * dz <= self.load_radius * self.load_radius {
                    out.insert(ChunkCoord::new(center.x + dx, center.y + dz));
                }
            }
        }
        out
    }

    /// チャンクファイルパスを構築
    pub fn chunk_path(&self, coord: ChunkCoord) -> PathBuf {
        let mut path = self.chunk_base_path.clone();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        path = parent.join(format!("{stem}_{}_{}.json", coord.x, coord.y));
        path
    }

    /// カメラ位置に基づいてロード/アンロードする
    pub fn update(&mut self, camera_position: Vec3, world: &mut World) -> ThrustResult<()> {
        let center = self.world_to_chunk(camera_position);
        let needed = self.chunks_in_range(center);

        // アンロードすべきチャンク
        let to_unload: Vec<ChunkCoord> = self
            .loaded_chunks
            .keys()
            .copied()
            .filter(|c| !needed.contains(c))
            .collect();
        for coord in to_unload {
            if let Some(entities) = self.loaded_chunks.remove(&coord) {
                for e in entities {
                    let _ = world.despawn(e);
                }
            }
        }

        // ロードすべきチャンク
        for coord in needed {
            if self.loaded_chunks.contains_key(&coord) {
                continue;
            }
            let path = self.chunk_path(coord);
            if !path.exists() {
                // ファイルがなければスキップ (空のチャンク)
                self.loaded_chunks.insert(coord, Vec::new());
                continue;
            }
            let scene = SerScene::load_from_file(path.to_str().unwrap_or(""))?;
            let before: Vec<Entity> = world.iter().map(|e| e.entity()).collect();
            scene.apply_to_world(world);
            let after: Vec<Entity> = world.iter().map(|e| e.entity()).collect();
            let new_entities: Vec<Entity> =
                after.into_iter().filter(|e| !before.contains(e)).collect();
            self.loaded_chunks.insert(coord, new_entities);
        }
        Ok(())
    }

    /// 現在ロードされているチャンク数
    pub fn loaded_count(&self) -> usize {
        self.loaded_chunks.len()
    }

    /// 全チャンクを強制アンロード
    pub fn unload_all(&mut self, world: &mut World) {
        for (_, entities) in self.loaded_chunks.drain() {
            for e in entities {
                let _ = world.despawn(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_chunk() {
        let s = StreamingWorld::new(100.0, 2, "/tmp/test");
        assert_eq!(s.world_to_chunk(Vec3::ZERO), ChunkCoord::new(0, 0));
        assert_eq!(
            s.world_to_chunk(Vec3::new(150.0, 0.0, 250.0)),
            ChunkCoord::new(1, 2)
        );
        assert_eq!(
            s.world_to_chunk(Vec3::new(-50.0, 0.0, 0.0)),
            ChunkCoord::new(-1, 0)
        );
    }

    #[test]
    fn test_chunks_in_range_radius_1() {
        let s = StreamingWorld::new(100.0, 1, "/tmp/test");
        let chunks = s.chunks_in_range(ChunkCoord::new(0, 0));
        // 中心 + 4 直接近傍 = 5
        assert_eq!(chunks.len(), 5);
    }

    #[test]
    fn test_chunks_in_range_radius_2() {
        let s = StreamingWorld::new(100.0, 2, "/tmp/test");
        let chunks = s.chunks_in_range(ChunkCoord::new(0, 0));
        // radius=2 円形なので 13 個
        assert!(chunks.len() >= 12 && chunks.len() <= 14);
    }

    #[test]
    fn test_chunk_path_format() {
        let s = StreamingWorld::new(100.0, 2, "/tmp/world.json");
        let p = s.chunk_path(ChunkCoord::new(3, -2));
        assert!(p.to_string_lossy().contains("world_3_-2.json"));
    }

    #[test]
    fn test_loaded_count_starts_zero() {
        let s = StreamingWorld::new(100.0, 1, "/tmp/test");
        assert_eq!(s.loaded_count(), 0);
    }
}
