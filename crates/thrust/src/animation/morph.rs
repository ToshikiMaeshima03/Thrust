//! Morph Target (Blend Shape) — Round 7
//!
//! 各 morph target はベースメッシュの頂点ごとの delta (位置 + 法線 + tangent) を持つ。
//! 重みつき blend を CPU で計算してメッシュの頂点バッファを書き換える簡易実装。
//!
//! GPU 側で blend する best practice では vertex shader で全 target にアクセスするが、
//! ここでは互換性を重視して CPU で blend → vertex_buffer を queue.write_buffer する。

use glam::{Vec3, Vec4};
use hecs::World;

use crate::ecs::resources::Resources;
use crate::mesh::vertex::Vertex;

/// 単一の morph target (delta 形式)
#[derive(Debug, Clone)]
pub struct MorphTarget {
    pub name: String,
    /// 頂点ごとの位置 delta (ベースから加算)
    pub position_deltas: Vec<Vec3>,
    /// 頂点ごとの法線 delta (オプション)
    pub normal_deltas: Vec<Vec3>,
    /// 頂点ごとのタンジェント delta (オプション)
    pub tangent_deltas: Vec<Vec4>,
}

impl MorphTarget {
    pub fn new(name: impl Into<String>, position_deltas: Vec<Vec3>) -> Self {
        Self {
            name: name.into(),
            position_deltas,
            normal_deltas: Vec::new(),
            tangent_deltas: Vec::new(),
        }
    }
}

/// メッシュに付与するモーフコンポーネント (ECS)
pub struct MorphController {
    /// ベース頂点 (CPU 側オリジナル、blend の度に再構築する)
    pub base_vertices: Vec<Vertex>,
    /// 各 morph target
    pub targets: Vec<MorphTarget>,
    /// 各 target の現在の重み (0..1)
    pub weights: Vec<f32>,
    /// 重みが変わったかどうか (changed フラグ、毎フレーム再アップロード判定用)
    pub dirty: bool,
}

impl MorphController {
    pub fn new(base_vertices: Vec<Vertex>, targets: Vec<MorphTarget>) -> Self {
        let n = targets.len();
        Self {
            base_vertices,
            targets,
            weights: vec![0.0; n],
            dirty: true,
        }
    }

    /// 重みを設定する (dirty フラグを立てる)
    pub fn set_weight(&mut self, target_idx: usize, w: f32) {
        if target_idx < self.weights.len() {
            let new_w = w.clamp(0.0, 1.0);
            if (self.weights[target_idx] - new_w).abs() > 1e-5 {
                self.weights[target_idx] = new_w;
                self.dirty = true;
            }
        }
    }

    /// 名前で重みを設定する
    pub fn set_weight_by_name(&mut self, name: &str, w: f32) {
        if let Some(i) = self.targets.iter().position(|t| t.name == name) {
            self.set_weight(i, w);
        }
    }

    /// blend した頂点配列を構築する (毎フレーム呼ぶ)
    pub fn blend(&self) -> Vec<Vertex> {
        let mut out = self.base_vertices.clone();
        for (i, target) in self.targets.iter().enumerate() {
            let w = self.weights[i];
            if w < 1e-5 {
                continue;
            }
            for (vidx, delta) in target.position_deltas.iter().enumerate() {
                if vidx < out.len() {
                    out[vidx].position[0] += delta.x * w;
                    out[vidx].position[1] += delta.y * w;
                    out[vidx].position[2] += delta.z * w;
                }
            }
            for (vidx, delta) in target.normal_deltas.iter().enumerate() {
                if vidx < out.len() {
                    out[vidx].normal[0] += delta.x * w;
                    out[vidx].normal[1] += delta.y * w;
                    out[vidx].normal[2] += delta.z * w;
                }
            }
            for (vidx, delta) in target.tangent_deltas.iter().enumerate() {
                if vidx < out.len() {
                    out[vidx].tangent[0] += delta.x * w;
                    out[vidx].tangent[1] += delta.y * w;
                    out[vidx].tangent[2] += delta.z * w;
                    out[vidx].tangent[3] += delta.w * w;
                }
            }
        }
        // 法線正規化
        for v in out.iter_mut() {
            let n = Vec3::new(v.normal[0], v.normal[1], v.normal[2]).normalize_or_zero();
            v.normal = [n.x, n.y, n.z];
        }
        out
    }
}

/// MorphController を持つエンティティを毎フレーム blend して GPU に再アップロードするシステム
pub fn morph_system(world: &mut World, res: &mut Resources) {
    use crate::ecs::components::MeshHandle;

    let dirty_entities: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &MorphController, &MeshHandle)>()
        .iter()
        .filter(|(_e, mc, _)| mc.dirty)
        .map(|(e, _, _)| e)
        .collect();

    for entity in dirty_entities {
        let blended = match world.get::<&MorphController>(entity) {
            Ok(mc) => mc.blend(),
            Err(_) => continue,
        };
        if let Ok(mh) = world.get::<&MeshHandle>(entity) {
            res.gpu
                .queue
                .write_buffer(&mh.0.vertex_buffer, 0, bytemuck::cast_slice(&blended));
        }
        if let Ok(mut mc) = world.get::<&mut MorphController>(entity) {
            mc.dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vertex(pos: [f32; 3]) -> Vertex {
        Vertex {
            position: pos,
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coords: [0.0, 0.0],
            joints: [0; 4],
            weights: [0.0; 4],
            _padding: [0.0; 2],
        }
    }

    #[test]
    fn test_blend_zero_weight() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let target = MorphTarget::new("a", vec![Vec3::new(1.0, 2.0, 3.0)]);
        let mut mc = MorphController::new(base.clone(), vec![target]);
        mc.set_weight(0, 0.0);
        let blended = mc.blend();
        assert_eq!(blended[0].position, base[0].position);
    }

    #[test]
    fn test_blend_full_weight() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let target = MorphTarget::new("a", vec![Vec3::new(1.0, 2.0, 3.0)]);
        let mut mc = MorphController::new(base, vec![target]);
        mc.set_weight(0, 1.0);
        let blended = mc.blend();
        assert!((blended[0].position[0] - 1.0).abs() < 1e-5);
        assert!((blended[0].position[1] - 2.0).abs() < 1e-5);
        assert!((blended[0].position[2] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_blend_partial_weight() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let target = MorphTarget::new("a", vec![Vec3::new(2.0, 0.0, 0.0)]);
        let mut mc = MorphController::new(base, vec![target]);
        mc.set_weight(0, 0.5);
        let blended = mc.blend();
        assert!((blended[0].position[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_blend_two_targets() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let t1 = MorphTarget::new("a", vec![Vec3::new(1.0, 0.0, 0.0)]);
        let t2 = MorphTarget::new("b", vec![Vec3::new(0.0, 1.0, 0.0)]);
        let mut mc = MorphController::new(base, vec![t1, t2]);
        mc.set_weight(0, 1.0);
        mc.set_weight(1, 1.0);
        let blended = mc.blend();
        assert!((blended[0].position[0] - 1.0).abs() < 1e-5);
        assert!((blended[0].position[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_weight_by_name() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let t = MorphTarget::new("smile", vec![Vec3::ONE]);
        let mut mc = MorphController::new(base, vec![t]);
        mc.set_weight_by_name("smile", 0.7);
        assert!((mc.weights[0] - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_dirty_flag() {
        let base = vec![test_vertex([0.0, 0.0, 0.0])];
        let t = MorphTarget::new("a", vec![Vec3::ZERO]);
        let mut mc = MorphController::new(base, vec![t]);
        mc.dirty = false;
        mc.set_weight(0, 0.5);
        assert!(mc.dirty);
    }
}
