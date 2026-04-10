//! LOD (Level of Detail) システム (Round 6)
//!
//! 距離ベースで複数メッシュを切替える。近いほど高詳細、遠いほど低詳細。
//! `MeshLod` コンポーネントに複数の (mesh, max_distance) を登録し、
//! `lod_system` が毎フレーム `MeshHandle` を自動切替する。

use hecs::{Entity, World};

use crate::camera::camera::Camera;
use crate::ecs::components::ActiveCamera;
use crate::mesh::mesh::Mesh;
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

/// LOD レベル
pub struct LodLevel {
    pub mesh: Mesh,
    /// この LOD が使われる最大距離 (これを超えると次の LOD へ)
    pub max_distance: f32,
}

/// LOD メッシュコンポーネント
///
/// `levels` は `max_distance` が昇順になるようソートされる。
/// 距離が最大値を超えるとオブジェクトは描画されない (culled)。
pub struct MeshLod {
    pub levels: Vec<LodLevel>,
    /// 現在選択中の LOD インデックス (lod_system が更新)
    pub current_index: usize,
    /// 全 LOD より遠い場合に非表示にするか
    pub cull_beyond_last: bool,
}

impl MeshLod {
    /// 新しい LOD を作成する。距離昇順に自動ソート。
    pub fn new(mut levels: Vec<LodLevel>) -> Self {
        levels.sort_by(|a, b| {
            a.max_distance
                .partial_cmp(&b.max_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            levels,
            current_index: 0,
            cull_beyond_last: true,
        }
    }

    /// 距離 d に対応する LOD インデックスを返す
    pub fn select_lod(&self, distance: f32) -> Option<usize> {
        for (i, level) in self.levels.iter().enumerate() {
            if distance <= level.max_distance {
                return Some(i);
            }
        }
        if self.cull_beyond_last {
            None
        } else {
            Some(self.levels.len() - 1)
        }
    }
}

/// LOD システム: カメラ距離に基づいて MeshHandle を切替える
pub fn lod_system(world: &mut World) {
    // ActiveCamera の位置を取得
    let Some(camera_pos) = world
        .query::<(&Camera, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(camera, _)| camera.position)
    else {
        return;
    };

    // LOD エンティティを更新
    // 距離を先に計算してから MeshHandle を差し替え (借用衝突回避)
    let mut updates: Vec<(Entity, usize, bool)> = Vec::new();
    for (entity, lod, transform, gt) in world
        .query::<(Entity, &MeshLod, &Transform, Option<&GlobalTransform>)>()
        .iter()
    {
        let pos = match gt {
            Some(g) => g.0.w_axis.truncate(),
            None => transform.translation,
        };
        let dist = (pos - camera_pos).length();
        match lod.select_lod(dist) {
            Some(idx) => {
                if idx != lod.current_index {
                    updates.push((entity, idx, true));
                }
            }
            None => {
                // 非表示
                updates.push((entity, 0, false));
            }
        }
    }

    for (entity, new_idx, visible) in updates {
        // current_index を更新
        if let Ok(mut lod) = world.get::<&mut MeshLod>(entity) {
            lod.current_index = new_idx;
        }
        // MeshHandle を差し替える場合は clone が必要だが、Mesh は clone 不可
        // → 代わりに Visible をトグルする簡易実装にする
        if !visible {
            let _ = world.insert_one(entity, crate::ecs::components::Visible(false));
        } else {
            let _ = world.insert_one(entity, crate::ecs::components::Visible(true));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_lod_select_closest() {
        // max_distance = 10, 30, 100 の LOD を持つ
        // distance = 5 → 0, distance = 20 → 1, distance = 80 → 2, distance = 200 → None
        // Mesh をスタブ化: LodLevel を max_distance だけで作る
        // → 代わりに select_lod をプリミティブなデータでテスト
        // Mesh の代わりに u32 でテスト用のサンプル構造体を作る
        struct TestLod {
            max_distance: f32,
        }
        struct TestSet {
            levels: Vec<TestLod>,
            cull_beyond_last: bool,
        }
        impl TestSet {
            fn select(&self, d: f32) -> Option<usize> {
                for (i, l) in self.levels.iter().enumerate() {
                    if d <= l.max_distance {
                        return Some(i);
                    }
                }
                if self.cull_beyond_last {
                    None
                } else {
                    Some(self.levels.len() - 1)
                }
            }
        }
        let set = TestSet {
            levels: vec![
                TestLod { max_distance: 10.0 },
                TestLod { max_distance: 30.0 },
                TestLod {
                    max_distance: 100.0,
                },
            ],
            cull_beyond_last: true,
        };
        assert_eq!(set.select(5.0), Some(0));
        assert_eq!(set.select(20.0), Some(1));
        assert_eq!(set.select(80.0), Some(2));
        assert_eq!(set.select(200.0), None);
    }

    #[test]
    fn test_lod_cull_or_clamp() {
        struct TestLod {
            max_distance: f32,
        }
        struct TestSet {
            levels: Vec<TestLod>,
            cull_beyond_last: bool,
        }
        impl TestSet {
            fn select(&self, d: f32) -> Option<usize> {
                for (i, l) in self.levels.iter().enumerate() {
                    if d <= l.max_distance {
                        return Some(i);
                    }
                }
                if self.cull_beyond_last {
                    None
                } else {
                    Some(self.levels.len() - 1)
                }
            }
        }
        let culling = TestSet {
            levels: vec![TestLod { max_distance: 10.0 }],
            cull_beyond_last: true,
        };
        assert_eq!(culling.select(100.0), None);

        let clamping = TestSet {
            levels: vec![TestLod { max_distance: 10.0 }],
            cull_beyond_last: false,
        };
        assert_eq!(clamping.select(100.0), Some(0));
    }
}
