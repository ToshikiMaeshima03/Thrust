//! 2-Bone Inverse Kinematics (Round 7)
//!
//! 解析的 2 ボーン IK ソルバー: 上腕 → 前腕 → 手首 のような肘関節を持つチェーンで
//! 手首位置を目標位置に近づける。
//!
//! 入力: ルート位置, 中間関節 (肘) のローカル位置, 末端 (手首) の目標位置, ポール位置
//! 出力: 中間関節と末端関節のワールド位置 (回転計算用)
//!
//! ECS 連携: `TwoBoneIk` コンポーネントを末端 (effector) 関節エンティティに追加すると、
//! `ik_system` が毎フレーム解析的に位置を計算し、各関節の Transform.translation を更新する。

use glam::Vec3;
use hecs::{Entity, World};

use crate::scene::transform::Transform;

/// 2-bone IK の入力 / 出力データ
#[derive(Debug, Clone, Copy)]
pub struct IkResult {
    /// 中間関節の世界位置
    pub mid: Vec3,
    /// 末端関節の世界位置
    pub end: Vec3,
}

/// 解析的 2-bone IK ソルバー
///
/// `root` から `target` までの距離が `len_a + len_b` を超える場合は完全伸展する。
/// `pole` は肘の方向を決めるためのワールド空間ポール位置 (NaN を避けるため必須)。
pub fn solve_two_bone_ik(root: Vec3, target: Vec3, pole: Vec3, len_a: f32, len_b: f32) -> IkResult {
    let to_target = target - root;
    let dist = to_target.length().max(1e-5);
    let max_reach = len_a + len_b;

    // 完全伸展ケース
    if dist >= max_reach - 1e-4 {
        let dir = to_target / dist;
        let mid = root + dir * len_a;
        let end = root + dir * max_reach;
        return IkResult { mid, end };
    }

    // 余弦定理で root から肘までの距離を計算
    // a = len_a (root → mid), b = len_b (mid → end), c = dist (root → end)
    // cos(A) = (a² + c² - b²) / (2ac)  ← root の角度
    let cos_a =
        ((len_a * len_a + dist * dist - len_b * len_b) / (2.0 * len_a * dist)).clamp(-1.0, 1.0);
    let projection = len_a * cos_a;

    // 肘の高さ (root→target 軸からの距離)
    let height = (len_a * len_a - projection * projection).max(0.0).sqrt();

    // root から target への単位ベクトル
    let chain_dir = to_target / dist;

    // pole 方向を chain_dir に直交化して肘方向を決定
    let to_pole = pole - root;
    let pole_perp = to_pole - chain_dir * to_pole.dot(chain_dir);
    let elbow_dir = pole_perp.normalize_or_zero();
    let elbow_dir = if elbow_dir.length_squared() < 1e-4 {
        // pole と chain が平行な場合は適当な perpendicular
        let up = if chain_dir.dot(Vec3::Y).abs() < 0.99 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        (up - chain_dir * up.dot(chain_dir)).normalize_or_zero()
    } else {
        elbow_dir
    };

    let mid = root + chain_dir * projection + elbow_dir * height;
    let end = target;
    IkResult { mid, end }
}

/// 2-bone IK コンポーネント (effector 関節に付与)
#[derive(Debug, Clone)]
pub struct TwoBoneIk {
    /// チェーンのルート関節 (例: 上腕の親、肩)
    pub root_joint: Entity,
    /// 中間関節 (例: 肘)
    pub mid_joint: Entity,
    /// ターゲットのワールド位置 (例: 手の到達目標)
    pub target_position: Vec3,
    /// ポール位置 (肘の方向を決める世界座標)
    pub pole_position: Vec3,
    /// root → mid のボーン長 (m)
    pub len_a: f32,
    /// mid → end のボーン長 (m)
    pub len_b: f32,
    /// 重み (0..1)。0 で IK 無効、1 で完全 IK
    pub weight: f32,
}

impl TwoBoneIk {
    pub fn new(
        root: Entity,
        mid: Entity,
        target: Vec3,
        pole: Vec3,
        len_a: f32,
        len_b: f32,
    ) -> Self {
        Self {
            root_joint: root,
            mid_joint: mid,
            target_position: target,
            pole_position: pole,
            len_a,
            len_b,
            weight: 1.0,
        }
    }
}

/// IK システム
///
/// `TwoBoneIk + Transform` を持つ effector エンティティを対象に解析 IK を適用し、
/// `mid_joint` と effector 自身の `Transform.translation` を更新する。
///
/// `propagate_transforms` の後、`skin_system` の前に呼ぶ。
pub fn ik_system(world: &mut World) {
    // (effector, ik) のペアを収集
    let solves: Vec<(Entity, TwoBoneIk, Vec3)> = world
        .query::<(hecs::Entity, &TwoBoneIk, &Transform)>()
        .iter()
        .map(|(e, ik, _)| (e, ik.clone(), Vec3::ZERO))
        .collect();

    for (effector, ik, _) in solves {
        // ルートの世界位置を取得
        let root_pos = match world.get::<&Transform>(ik.root_joint) {
            Ok(t) => t.translation,
            Err(_) => continue,
        };
        let result = solve_two_bone_ik(
            root_pos,
            ik.target_position,
            ik.pole_position,
            ik.len_a,
            ik.len_b,
        );

        // weight でブレンド
        if let Ok(mut t) = world.get::<&mut Transform>(ik.mid_joint) {
            t.translation = t.translation.lerp(result.mid, ik.weight);
        }
        if let Ok(mut t) = world.get::<&mut Transform>(effector) {
            t.translation = t.translation.lerp(result.end, ik.weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_extension() {
        // 完全伸展: root から target まで len_a + len_b の距離
        let root = Vec3::ZERO;
        let target = Vec3::new(2.0, 0.0, 0.0);
        let pole = Vec3::new(1.0, 1.0, 0.0);
        let r = solve_two_bone_ik(root, target, pole, 1.0, 1.0);
        assert!((r.end - target).length() < 0.01);
    }

    #[test]
    fn test_bent_target() {
        // target が len_a + len_b より近い場合は肘が曲がる
        let root = Vec3::ZERO;
        let target = Vec3::new(1.0, 0.0, 0.0);
        let pole = Vec3::new(0.5, 1.0, 0.0);
        let r = solve_two_bone_ik(root, target, pole, 1.0, 1.0);
        // 中間点は y > 0 (pole の方向)
        assert!(r.mid.y > 0.0, "肘は y > 0 で曲がるはず: {:?}", r.mid);
        // end はターゲットに到達
        assert!((r.end - target).length() < 0.01);
        // ボーン長が保たれる
        assert!(((r.mid - root).length() - 1.0).abs() < 0.01);
        assert!(((r.end - r.mid).length() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_unreachable_target() {
        // 完全伸展を超えた場合は最大伸展位置に
        let root = Vec3::ZERO;
        let target = Vec3::new(5.0, 0.0, 0.0);
        let pole = Vec3::Y;
        let r = solve_two_bone_ik(root, target, pole, 1.0, 1.0);
        // end は最大伸展 (2.0, 0, 0) に
        assert!((r.end - Vec3::new(2.0, 0.0, 0.0)).length() < 0.01);
    }

    #[test]
    fn test_zero_pole_fallback() {
        // pole が root と一致する縮退ケース
        let root = Vec3::ZERO;
        let target = Vec3::new(1.0, 0.0, 0.0);
        let pole = Vec3::ZERO;
        let r = solve_two_bone_ik(root, target, pole, 1.0, 1.0);
        // 値が NaN にならないことを確認
        assert!(r.mid.is_finite());
        assert!(r.end.is_finite());
    }
}
