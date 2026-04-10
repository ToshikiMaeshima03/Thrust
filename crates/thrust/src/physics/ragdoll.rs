//! ラグドール (Round 8)
//!
//! 簡易ヒューマノイドラグドールを構築するヘルパー。
//! 11 ボーン構成 (head/torso/upper_arm × 2/lower_arm × 2/upper_leg × 2/lower_leg × 2 + hip)
//! を rapier3d の RigidBody + Joint で連結する。
//!
//! 用途: キャラクターが死亡時にスケルタルアニメから物理駆動に切り替える等。

use glam::Vec3;
use hecs::{Entity, World};

use crate::math::Aabb;
use crate::physics::joints::{JointDescriptor, JointKind};
use crate::physics::rapier_world::{RigidBody, RigidBodyType};
use crate::physics::{Collider, ColliderShape};
use crate::scene::transform::Transform;

/// ラグドールパーツ識別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RagdollBone {
    Head,
    Torso,
    Hip,
    UpperArmL,
    LowerArmL,
    UpperArmR,
    LowerArmR,
    UpperLegL,
    LowerLegL,
    UpperLegR,
    LowerLegR,
}

/// ラグドールのパーツ寸法 (m)
#[derive(Debug, Clone)]
pub struct RagdollDimensions {
    pub head_radius: f32,
    pub torso_size: Vec3,
    pub hip_size: Vec3,
    pub upper_arm_length: f32,
    pub lower_arm_length: f32,
    pub upper_leg_length: f32,
    pub lower_leg_length: f32,
    pub limb_radius: f32,
}

impl Default for RagdollDimensions {
    fn default() -> Self {
        Self {
            head_radius: 0.12,
            torso_size: Vec3::new(0.4, 0.5, 0.2),
            hip_size: Vec3::new(0.35, 0.15, 0.2),
            upper_arm_length: 0.3,
            lower_arm_length: 0.3,
            upper_leg_length: 0.4,
            lower_leg_length: 0.4,
            limb_radius: 0.05,
        }
    }
}

/// ラグドールビルダー
///
/// 指定した root 位置を中心にヒューマノイドラグドールを構築し、
/// 各ボーンエンティティと joint descriptor を world に挿入する。
pub struct RagdollBuilder<'a> {
    pub world: &'a mut World,
    pub root_position: Vec3,
    pub dimensions: RagdollDimensions,
    pub bones: std::collections::HashMap<RagdollBone, Entity>,
}

impl<'a> RagdollBuilder<'a> {
    pub fn new(world: &'a mut World, root_position: Vec3) -> Self {
        Self {
            world,
            root_position,
            dimensions: RagdollDimensions::default(),
            bones: std::collections::HashMap::new(),
        }
    }

    pub fn with_dimensions(mut self, dim: RagdollDimensions) -> Self {
        self.dimensions = dim;
        self
    }

    /// 11 ボーンと拘束を生成する
    pub fn build(mut self) -> std::collections::HashMap<RagdollBone, Entity> {
        let d = self.dimensions.clone();
        let root = self.root_position;

        // 各ボーンの初期位置 (root が hip 中心)
        let hip_pos = root;
        let torso_pos = hip_pos + Vec3::new(0.0, d.hip_size.y * 0.5 + d.torso_size.y * 0.5, 0.0);
        let head_pos = torso_pos + Vec3::new(0.0, d.torso_size.y * 0.5 + d.head_radius * 1.5, 0.0);

        let shoulder_l = torso_pos + Vec3::new(d.torso_size.x * 0.5, d.torso_size.y * 0.4, 0.0);
        let shoulder_r = torso_pos + Vec3::new(-d.torso_size.x * 0.5, d.torso_size.y * 0.4, 0.0);
        let upper_arm_l = shoulder_l + Vec3::new(d.upper_arm_length * 0.5, 0.0, 0.0);
        let lower_arm_l = upper_arm_l
            + Vec3::new(
                d.upper_arm_length * 0.5 + d.lower_arm_length * 0.5,
                0.0,
                0.0,
            );
        let upper_arm_r = shoulder_r + Vec3::new(-d.upper_arm_length * 0.5, 0.0, 0.0);
        let lower_arm_r = upper_arm_r
            + Vec3::new(
                -d.upper_arm_length * 0.5 - d.lower_arm_length * 0.5,
                0.0,
                0.0,
            );

        let hip_l = hip_pos + Vec3::new(d.hip_size.x * 0.3, -d.hip_size.y * 0.5, 0.0);
        let hip_r = hip_pos + Vec3::new(-d.hip_size.x * 0.3, -d.hip_size.y * 0.5, 0.0);
        let upper_leg_l = hip_l + Vec3::new(0.0, -d.upper_leg_length * 0.5, 0.0);
        let lower_leg_l = upper_leg_l
            + Vec3::new(
                0.0,
                -d.upper_leg_length * 0.5 - d.lower_leg_length * 0.5,
                0.0,
            );
        let upper_leg_r = hip_r + Vec3::new(0.0, -d.upper_leg_length * 0.5, 0.0);
        let lower_leg_r = upper_leg_r
            + Vec3::new(
                0.0,
                -d.upper_leg_length * 0.5 - d.lower_leg_length * 0.5,
                0.0,
            );

        let aabb_shape = |he: Vec3| ColliderShape::Aabb(Aabb::new(-he, he));
        let sphere_shape = |r: f32| ColliderShape::Sphere {
            center: Vec3::ZERO,
            radius: r,
        };

        // ボーン spawn
        let hip_e = self.spawn_bone(hip_pos, aabb_shape(d.hip_size * 0.5));
        let torso_e = self.spawn_bone(torso_pos, aabb_shape(d.torso_size * 0.5));
        let head_e = self.spawn_bone(head_pos, sphere_shape(d.head_radius));

        let upper_arm_l_e = self.spawn_bone(
            upper_arm_l,
            aabb_shape(Vec3::new(
                d.upper_arm_length * 0.5,
                d.limb_radius,
                d.limb_radius,
            )),
        );
        let lower_arm_l_e = self.spawn_bone(
            lower_arm_l,
            aabb_shape(Vec3::new(
                d.lower_arm_length * 0.5,
                d.limb_radius,
                d.limb_radius,
            )),
        );
        let upper_arm_r_e = self.spawn_bone(
            upper_arm_r,
            aabb_shape(Vec3::new(
                d.upper_arm_length * 0.5,
                d.limb_radius,
                d.limb_radius,
            )),
        );
        let lower_arm_r_e = self.spawn_bone(
            lower_arm_r,
            aabb_shape(Vec3::new(
                d.lower_arm_length * 0.5,
                d.limb_radius,
                d.limb_radius,
            )),
        );

        let upper_leg_l_e = self.spawn_bone(
            upper_leg_l,
            aabb_shape(Vec3::new(
                d.limb_radius,
                d.upper_leg_length * 0.5,
                d.limb_radius,
            )),
        );
        let lower_leg_l_e = self.spawn_bone(
            lower_leg_l,
            aabb_shape(Vec3::new(
                d.limb_radius,
                d.lower_leg_length * 0.5,
                d.limb_radius,
            )),
        );
        let upper_leg_r_e = self.spawn_bone(
            upper_leg_r,
            aabb_shape(Vec3::new(
                d.limb_radius,
                d.upper_leg_length * 0.5,
                d.limb_radius,
            )),
        );
        let lower_leg_r_e = self.spawn_bone(
            lower_leg_r,
            aabb_shape(Vec3::new(
                d.limb_radius,
                d.lower_leg_length * 0.5,
                d.limb_radius,
            )),
        );

        self.bones.insert(RagdollBone::Hip, hip_e);
        self.bones.insert(RagdollBone::Torso, torso_e);
        self.bones.insert(RagdollBone::Head, head_e);
        self.bones.insert(RagdollBone::UpperArmL, upper_arm_l_e);
        self.bones.insert(RagdollBone::LowerArmL, lower_arm_l_e);
        self.bones.insert(RagdollBone::UpperArmR, upper_arm_r_e);
        self.bones.insert(RagdollBone::LowerArmR, lower_arm_r_e);
        self.bones.insert(RagdollBone::UpperLegL, upper_leg_l_e);
        self.bones.insert(RagdollBone::LowerLegL, lower_leg_l_e);
        self.bones.insert(RagdollBone::UpperLegR, upper_leg_r_e);
        self.bones.insert(RagdollBone::LowerLegR, lower_leg_r_e);

        // ジョイント
        // Hip → Torso (球面)
        self.add_joint(
            torso_e,
            hip_e,
            Vec3::ZERO,
            Vec3::new(0.0, d.torso_size.y * 0.5, 0.0),
            JointKind::Spherical,
        );
        // Torso → Head (球面)
        self.add_joint(
            head_e,
            torso_e,
            Vec3::ZERO,
            Vec3::new(0.0, d.torso_size.y * 0.5 + d.head_radius * 0.5, 0.0),
            JointKind::Spherical,
        );
        // Torso → Upper Arm L/R (球面)
        self.add_joint(
            upper_arm_l_e,
            torso_e,
            Vec3::new(-d.upper_arm_length * 0.5, 0.0, 0.0),
            Vec3::new(d.torso_size.x * 0.5, d.torso_size.y * 0.4, 0.0),
            JointKind::Spherical,
        );
        self.add_joint(
            upper_arm_r_e,
            torso_e,
            Vec3::new(d.upper_arm_length * 0.5, 0.0, 0.0),
            Vec3::new(-d.torso_size.x * 0.5, d.torso_size.y * 0.4, 0.0),
            JointKind::Spherical,
        );
        // Upper Arm → Lower Arm (リボリュート)
        self.add_joint(
            lower_arm_l_e,
            upper_arm_l_e,
            Vec3::new(-d.lower_arm_length * 0.5, 0.0, 0.0),
            Vec3::new(d.upper_arm_length * 0.5, 0.0, 0.0),
            JointKind::Revolute { axis: Vec3::Z },
        );
        self.add_joint(
            lower_arm_r_e,
            upper_arm_r_e,
            Vec3::new(d.lower_arm_length * 0.5, 0.0, 0.0),
            Vec3::new(-d.upper_arm_length * 0.5, 0.0, 0.0),
            JointKind::Revolute { axis: Vec3::Z },
        );
        // Hip → Upper Leg L/R (球面)
        self.add_joint(
            upper_leg_l_e,
            hip_e,
            Vec3::new(0.0, d.upper_leg_length * 0.5, 0.0),
            Vec3::new(d.hip_size.x * 0.3, -d.hip_size.y * 0.5, 0.0),
            JointKind::Spherical,
        );
        self.add_joint(
            upper_leg_r_e,
            hip_e,
            Vec3::new(0.0, d.upper_leg_length * 0.5, 0.0),
            Vec3::new(-d.hip_size.x * 0.3, -d.hip_size.y * 0.5, 0.0),
            JointKind::Spherical,
        );
        // Upper Leg → Lower Leg (リボリュート、膝)
        self.add_joint(
            lower_leg_l_e,
            upper_leg_l_e,
            Vec3::new(0.0, d.lower_leg_length * 0.5, 0.0),
            Vec3::new(0.0, -d.upper_leg_length * 0.5, 0.0),
            JointKind::Revolute { axis: Vec3::X },
        );
        self.add_joint(
            lower_leg_r_e,
            upper_leg_r_e,
            Vec3::new(0.0, d.lower_leg_length * 0.5, 0.0),
            Vec3::new(0.0, -d.upper_leg_length * 0.5, 0.0),
            JointKind::Revolute { axis: Vec3::X },
        );

        self.bones
    }

    fn spawn_bone(&mut self, position: Vec3, shape: ColliderShape) -> Entity {
        let t = Transform {
            translation: position,
            ..Default::default()
        };
        let body = RigidBody {
            body_type: RigidBodyType::Dynamic,
            linear_damping: 0.5,
            angular_damping: 0.5,
            initial_velocity: Vec3::ZERO,
        };
        let collider = Collider {
            shape,
            is_trigger: false,
        };
        self.world.spawn((t, body, collider))
    }

    fn add_joint(
        &mut self,
        entity: Entity,
        parent: Entity,
        local_anchor1: Vec3,
        local_anchor2: Vec3,
        kind: JointKind,
    ) {
        let desc = JointDescriptor {
            parent: Some(parent),
            local_anchor1,
            local_anchor2,
            kind,
        };
        let _ = self.world.insert_one(entity, desc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_dimensions() {
        let d = RagdollDimensions::default();
        assert!(d.head_radius > 0.0);
        assert!(d.torso_size.y > 0.0);
    }

    #[test]
    fn test_build_creates_11_bones() {
        let mut world = World::new();
        let bones = RagdollBuilder::new(&mut world, Vec3::new(0.0, 5.0, 0.0)).build();
        assert_eq!(bones.len(), 11);
    }

    #[test]
    fn test_build_unique_entities() {
        let mut world = World::new();
        let bones = RagdollBuilder::new(&mut world, Vec3::ZERO).build();
        let mut seen = std::collections::HashSet::new();
        for e in bones.values() {
            assert!(seen.insert(*e), "重複エンティティ: {:?}", e);
        }
    }

    #[test]
    fn test_built_entities_have_transform() {
        let mut world = World::new();
        let bones = RagdollBuilder::new(&mut world, Vec3::ZERO).build();
        for e in bones.values() {
            assert!(world.get::<&Transform>(*e).is_ok());
        }
    }
}
