//! 物理拘束 (Joints) — Round 7
//!
//! rapier3d のジョイント API のラッパー。固定 / リボリュート / プリズマティック / 球面の
//! 4 種をサポートする。
//!
//! ECS 経由で `JointDescriptor` をエンティティに挿入すると、`joint_init_system` が
//! 接続先の `PhysicsHandle` を解決して rapier に登録する。

use glam::Vec3;
use hecs::{Entity, World};
use rapier3d::prelude::*;

use crate::physics::PhysicsHandle;
use crate::physics::PhysicsWorld;

/// ジョイントの種類
#[derive(Debug, Clone)]
pub enum JointKind {
    /// 完全固定 (3 軸位置 + 3 軸回転すべて拘束)
    Fixed,
    /// リボリュート (1 軸の回転のみ許可)。`axis` は回転軸 (ローカル)
    Revolute { axis: Vec3 },
    /// プリズマティック (1 軸の並進のみ許可)。`axis` は並進軸
    Prismatic { axis: Vec3 },
    /// 球面 (位置を完全拘束、3 軸回転自由)
    Spherical,
}

/// ジョイント定義 (ユーザー API、ECS コンポーネント)
#[derive(Debug, Clone)]
pub struct JointDescriptor {
    /// 接続先の親エンティティ (None の場合はワールド固定)
    pub parent: Option<Entity>,
    /// 親ローカルでのアタッチ位置
    pub local_anchor1: Vec3,
    /// 子ローカルでのアタッチ位置
    pub local_anchor2: Vec3,
    /// ジョイントの種類
    pub kind: JointKind,
}

impl JointDescriptor {
    pub fn fixed(parent: Entity, anchor1: Vec3, anchor2: Vec3) -> Self {
        Self {
            parent: Some(parent),
            local_anchor1: anchor1,
            local_anchor2: anchor2,
            kind: JointKind::Fixed,
        }
    }

    pub fn revolute(parent: Entity, anchor1: Vec3, anchor2: Vec3, axis: Vec3) -> Self {
        Self {
            parent: Some(parent),
            local_anchor1: anchor1,
            local_anchor2: anchor2,
            kind: JointKind::Revolute { axis },
        }
    }

    pub fn prismatic(parent: Entity, anchor1: Vec3, anchor2: Vec3, axis: Vec3) -> Self {
        Self {
            parent: Some(parent),
            local_anchor1: anchor1,
            local_anchor2: anchor2,
            kind: JointKind::Prismatic { axis },
        }
    }

    pub fn spherical(parent: Entity, anchor1: Vec3, anchor2: Vec3) -> Self {
        Self {
            parent: Some(parent),
            local_anchor1: anchor1,
            local_anchor2: anchor2,
            kind: JointKind::Spherical,
        }
    }
}

/// 登録済みジョイントハンドル (joint_init_system が挿入)
pub struct JointHandle {
    pub handle: ImpulseJointHandle,
}

/// `JointDescriptor` を持つエンティティを rapier の ImpulseJointSet に登録するシステム
pub fn joint_init_system(world: &mut World, physics: &mut PhysicsWorld) {
    let pending: Vec<(Entity, JointDescriptor)> = world
        .query::<(Entity, &JointDescriptor)>()
        .without::<&JointHandle>()
        .iter()
        .map(|(e, d)| (e, d.clone()))
        .collect();

    for (entity, desc) in pending {
        let Some(parent) = desc.parent else {
            continue;
        };
        let parent_body = match world.get::<&PhysicsHandle>(parent) {
            Ok(h) => h.body,
            Err(_) => continue,
        };
        let child_body = match world.get::<&PhysicsHandle>(entity) {
            Ok(h) => h.body,
            Err(_) => continue,
        };

        let a1 = vec_to_point(desc.local_anchor1);
        let a2 = vec_to_point(desc.local_anchor2);

        let joint = match desc.kind {
            JointKind::Fixed => GenericJointBuilder::new(JointAxesMask::all())
                .local_anchor1(a1)
                .local_anchor2(a2)
                .build(),
            JointKind::Revolute { axis } => RevoluteJointBuilder::new(unit_vec(axis))
                .local_anchor1(a1)
                .local_anchor2(a2)
                .build()
                .into(),
            JointKind::Prismatic { axis } => PrismaticJointBuilder::new(unit_vec(axis))
                .local_anchor1(a1)
                .local_anchor2(a2)
                .build()
                .into(),
            JointKind::Spherical => SphericalJointBuilder::new()
                .local_anchor1(a1)
                .local_anchor2(a2)
                .build()
                .into(),
        };

        let handle = physics
            .joints_impulse
            .insert(parent_body, child_body, joint, true);
        let _ = world.insert_one(entity, JointHandle { handle });
    }
}

fn vec_to_point(v: Vec3) -> nalgebra::Point3<f32> {
    nalgebra::Point3::new(v.x, v.y, v.z)
}

fn unit_vec(v: Vec3) -> UnitVector<Real> {
    let mag = v.length().max(1e-5);
    let n = v / mag;
    UnitVector::new_normalize(nalgebra::Vector3::new(n.x, n.y, n.z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_factories() {
        let parent = unsafe { std::mem::transmute::<u64, Entity>(1) };
        let _f = JointDescriptor::fixed(parent, Vec3::ZERO, Vec3::ZERO);
        let _r = JointDescriptor::revolute(parent, Vec3::ZERO, Vec3::ZERO, Vec3::Y);
        let _p = JointDescriptor::prismatic(parent, Vec3::ZERO, Vec3::ZERO, Vec3::X);
        let _s = JointDescriptor::spherical(parent, Vec3::ZERO, Vec3::ZERO);
    }
}
