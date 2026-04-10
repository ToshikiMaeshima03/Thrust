//! シーンシリアライゼーション (Round 5)
//!
//! Transform / マテリアル / ライト / 剛体定義を JSON で保存・復元する。
//! メッシュ参照はパス文字列で表現 (実体は再ロードされる)。

use hecs::World;
use serde::{Deserialize, Serialize};

use crate::error::{ThrustError, ThrustResult};
use crate::light::light::{AmbientLight, DirectionalLight, PointLight, SpotLight};
use crate::material::material::Material;
use crate::physics::{Collider, ColliderShape, RigidBody, RigidBodyType, Velocity};
use crate::scene::transform::Transform;

/// シリアライズ可能な Transform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl From<&Transform> for SerTransform {
    fn from(t: &Transform) -> Self {
        Self {
            translation: t.translation.to_array(),
            rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
            scale: t.scale.to_array(),
        }
    }
}

impl From<SerTransform> for Transform {
    fn from(s: SerTransform) -> Self {
        Self {
            translation: glam::Vec3::from(s.translation),
            rotation: glam::Quat::from_xyzw(
                s.rotation[0],
                s.rotation[1],
                s.rotation[2],
                s.rotation[3],
            ),
            scale: glam::Vec3::from(s.scale),
        }
    }
}

/// シリアライズ可能な PBR マテリアル (テクスチャパスは保存しない簡易版)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerMaterial {
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub normal_scale: f32,
    pub occlusion_strength: f32,
}

impl From<&Material> for SerMaterial {
    fn from(m: &Material) -> Self {
        Self {
            base_color_factor: m.base_color_factor.to_array(),
            metallic_factor: m.metallic_factor,
            roughness_factor: m.roughness_factor,
            emissive_factor: m.emissive_factor.to_array(),
            normal_scale: m.normal_scale,
            occlusion_strength: m.occlusion_strength,
        }
    }
}

impl From<SerMaterial> for Material {
    fn from(s: SerMaterial) -> Self {
        Self {
            base_color_factor: glam::Vec4::from(s.base_color_factor),
            metallic_factor: s.metallic_factor,
            roughness_factor: s.roughness_factor,
            emissive_factor: glam::Vec3::from(s.emissive_factor),
            normal_scale: s.normal_scale,
            occlusion_strength: s.occlusion_strength,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerDirectionalLight {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerPointLight {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerSpotLight {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
    pub direction: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerAmbientLight {
    pub color: [f32; 3],
    pub intensity: f32,
}

/// 剛体タイプ
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SerBodyType {
    Dynamic,
    Fixed,
    KinematicPositionBased,
}

impl From<RigidBodyType> for SerBodyType {
    fn from(t: RigidBodyType) -> Self {
        match t {
            RigidBodyType::Dynamic => Self::Dynamic,
            RigidBodyType::Fixed => Self::Fixed,
            RigidBodyType::KinematicPositionBased => Self::KinematicPositionBased,
        }
    }
}

impl From<SerBodyType> for RigidBodyType {
    fn from(s: SerBodyType) -> Self {
        match s {
            SerBodyType::Dynamic => Self::Dynamic,
            SerBodyType::Fixed => Self::Fixed,
            SerBodyType::KinematicPositionBased => Self::KinematicPositionBased,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerRigidBody {
    pub body_type: SerBodyType,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub initial_velocity: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerColliderShape {
    Aabb { min: [f32; 3], max: [f32; 3] },
    Sphere { center: [f32; 3], radius: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerCollider {
    pub shape: SerColliderShape,
    pub is_trigger: bool,
}

/// 1 つのエンティティ
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SerEntity {
    pub name: Option<String>,
    pub transform: Option<SerTransform>,
    pub material: Option<SerMaterial>,
    pub directional_light: Option<SerDirectionalLight>,
    pub point_light: Option<SerPointLight>,
    pub spot_light: Option<SerSpotLight>,
    pub ambient_light: Option<SerAmbientLight>,
    pub rigid_body: Option<SerRigidBody>,
    pub collider: Option<SerCollider>,
    pub velocity: Option<[f32; 3]>,
    /// メッシュパス (将来用、現状は未参照)
    pub mesh_path: Option<String>,
}

/// シーン全体
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SerScene {
    pub version: u32,
    pub entities: Vec<SerEntity>,
}

impl SerScene {
    /// World からシーンを抽出する
    pub fn from_world(world: &World) -> Self {
        let mut entities = Vec::new();

        for entity in world.iter() {
            let mut ser = SerEntity::default();
            let mut has_data = false;

            if let Some(t) = entity.get::<&Transform>() {
                ser.transform = Some(SerTransform::from(&*t));
                has_data = true;
            }
            if let Some(m) = entity.get::<&Material>() {
                ser.material = Some(SerMaterial::from(&*m));
                has_data = true;
            }
            if let Some(l) = entity.get::<&DirectionalLight>() {
                ser.directional_light = Some(SerDirectionalLight {
                    direction: l.direction.to_array(),
                    color: l.color.to_array(),
                    intensity: l.intensity,
                });
                has_data = true;
            }
            if let Some(l) = entity.get::<&PointLight>() {
                ser.point_light = Some(SerPointLight {
                    color: l.color.to_array(),
                    intensity: l.intensity,
                    range: l.range,
                });
                has_data = true;
            }
            if let Some(l) = entity.get::<&SpotLight>() {
                ser.spot_light = Some(SerSpotLight {
                    color: l.color.to_array(),
                    intensity: l.intensity,
                    range: l.range,
                    inner_angle: l.inner_angle,
                    outer_angle: l.outer_angle,
                    direction: l.direction.to_array(),
                });
                has_data = true;
            }
            if let Some(l) = entity.get::<&AmbientLight>() {
                ser.ambient_light = Some(SerAmbientLight {
                    color: l.color.to_array(),
                    intensity: l.intensity,
                });
                has_data = true;
            }
            if let Some(rb) = entity.get::<&RigidBody>() {
                ser.rigid_body = Some(SerRigidBody {
                    body_type: SerBodyType::from(rb.body_type),
                    linear_damping: rb.linear_damping,
                    angular_damping: rb.angular_damping,
                    initial_velocity: rb.initial_velocity.to_array(),
                });
                has_data = true;
            }
            if let Some(c) = entity.get::<&Collider>() {
                let shape = match &c.shape {
                    ColliderShape::Aabb(aabb) => SerColliderShape::Aabb {
                        min: aabb.min.to_array(),
                        max: aabb.max.to_array(),
                    },
                    ColliderShape::Sphere { center, radius } => SerColliderShape::Sphere {
                        center: center.to_array(),
                        radius: *radius,
                    },
                };
                ser.collider = Some(SerCollider {
                    shape,
                    is_trigger: c.is_trigger,
                });
                has_data = true;
            }
            if let Some(v) = entity.get::<&Velocity>() {
                ser.velocity = Some(v.linear.to_array());
                has_data = true;
            }

            if has_data {
                entities.push(ser);
            }
        }

        Self {
            version: 1,
            entities,
        }
    }

    /// World にシーンを復元する
    pub fn apply_to_world(&self, world: &mut World) {
        for ser in &self.entities {
            let mut builder = hecs::EntityBuilder::new();

            if let Some(t) = ser.transform.clone() {
                builder.add(Transform::from(t));
            }
            if let Some(m) = ser.material.clone() {
                builder.add(Material::from(m));
            }
            if let Some(l) = &ser.directional_light {
                builder.add(DirectionalLight {
                    direction: glam::Vec3::from(l.direction),
                    color: glam::Vec3::from(l.color),
                    intensity: l.intensity,
                });
            }
            if let Some(l) = &ser.point_light {
                builder.add(PointLight {
                    color: glam::Vec3::from(l.color),
                    intensity: l.intensity,
                    range: l.range,
                });
            }
            if let Some(l) = &ser.spot_light {
                builder.add(SpotLight {
                    color: glam::Vec3::from(l.color),
                    intensity: l.intensity,
                    range: l.range,
                    inner_angle: l.inner_angle,
                    outer_angle: l.outer_angle,
                    direction: glam::Vec3::from(l.direction),
                });
            }
            if let Some(l) = &ser.ambient_light {
                builder.add(AmbientLight {
                    color: glam::Vec3::from(l.color),
                    intensity: l.intensity,
                });
            }
            if let Some(rb) = &ser.rigid_body {
                builder.add(RigidBody {
                    body_type: RigidBodyType::from(rb.body_type),
                    linear_damping: rb.linear_damping,
                    angular_damping: rb.angular_damping,
                    initial_velocity: glam::Vec3::from(rb.initial_velocity),
                });
            }
            if let Some(c) = &ser.collider {
                let shape = match &c.shape {
                    SerColliderShape::Aabb { min, max } => ColliderShape::Aabb(
                        crate::math::Aabb::new(glam::Vec3::from(*min), glam::Vec3::from(*max)),
                    ),
                    SerColliderShape::Sphere { center, radius } => ColliderShape::Sphere {
                        center: glam::Vec3::from(*center),
                        radius: *radius,
                    },
                };
                builder.add(Collider {
                    shape,
                    is_trigger: c.is_trigger,
                });
            }
            if let Some(v) = ser.velocity {
                builder.add(Velocity {
                    linear: glam::Vec3::from(v),
                });
            }

            world.spawn(builder.build());
        }
    }

    /// JSON に保存する
    pub fn save_to_file(&self, path: &str) -> ThrustResult<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ThrustError::SceneSerialize(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| ThrustError::Io {
            path: path.into(),
            source: e,
        })?;
        Ok(())
    }

    /// JSON から読み込む
    pub fn load_from_file(path: &str) -> ThrustResult<Self> {
        let json = std::fs::read_to_string(path).map_err(|e| ThrustError::Io {
            path: path.into(),
            source: e,
        })?;
        serde_json::from_str(&json).map_err(|e| ThrustError::SceneSerialize(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_roundtrip() {
        let original = Transform {
            translation: glam::Vec3::new(1.0, 2.0, 3.0),
            rotation: glam::Quat::from_rotation_y(0.5),
            scale: glam::Vec3::new(2.0, 2.0, 2.0),
        };
        let ser = SerTransform::from(&original);
        let restored = Transform::from(ser);
        assert!((restored.translation - original.translation).length() < 1e-5);
        assert!((restored.rotation.dot(original.rotation) - 1.0).abs() < 1e-5);
        assert!((restored.scale - original.scale).length() < 1e-5);
    }

    #[test]
    fn test_material_roundtrip() {
        let original = Material::metallic(glam::Vec3::new(0.95, 0.93, 0.88), 0.2);
        let ser = SerMaterial::from(&original);
        let restored = Material::from(ser);
        assert!((restored.base_color_factor - original.base_color_factor).length() < 1e-5);
        assert!((restored.metallic_factor - original.metallic_factor).abs() < 1e-5);
        assert!((restored.roughness_factor - original.roughness_factor).abs() < 1e-5);
    }

    #[test]
    fn test_scene_from_world_basic() {
        let mut world = World::new();
        world.spawn((
            Transform::from_translation(glam::Vec3::new(1.0, 2.0, 3.0)),
            Material::default(),
        ));
        world.spawn((PointLight::default(),));

        let scene = SerScene::from_world(&world);
        assert_eq!(scene.entities.len(), 2);
        assert_eq!(scene.version, 1);
    }

    #[test]
    fn test_scene_apply_to_world() {
        let mut world1 = World::new();
        world1.spawn((
            Transform::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)),
            Material::flat_color(glam::Vec3::new(1.0, 0.0, 0.0)),
        ));

        let scene = SerScene::from_world(&world1);

        let mut world2 = World::new();
        scene.apply_to_world(&mut world2);

        let restored_count = world2.iter().count();
        assert_eq!(restored_count, 1);

        let entity = world2.iter().next().unwrap().entity();
        let t = world2.get::<&Transform>(entity).unwrap();
        assert!((t.translation.x - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_scene_json_roundtrip() {
        let mut world = World::new();
        world.spawn((Transform::default(), Material::default()));
        world.spawn((DirectionalLight::default(),));
        world.spawn((PointLight::default(),));

        let scene = SerScene::from_world(&world);
        let json = serde_json::to_string(&scene).unwrap();
        let restored: SerScene = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entities.len(), scene.entities.len());
        assert_eq!(restored.version, scene.version);
    }

    #[test]
    fn test_collider_roundtrip() {
        let aabb = crate::math::Aabb::new(glam::Vec3::splat(-1.0), glam::Vec3::splat(1.0));
        let mut world1 = World::new();
        world1.spawn((
            Transform::default(),
            Collider {
                shape: ColliderShape::Aabb(aabb),
                is_trigger: false,
            },
        ));
        let scene = SerScene::from_world(&world1);
        let json = serde_json::to_string(&scene).unwrap();
        let restored: SerScene = serde_json::from_str(&json).unwrap();
        let mut world2 = World::new();
        restored.apply_to_world(&mut world2);

        let entity = world2.iter().next().unwrap().entity();
        let c = world2.get::<&Collider>(entity).unwrap();
        match &c.shape {
            ColliderShape::Aabb(a) => {
                assert!((a.min - glam::Vec3::splat(-1.0)).length() < 1e-5);
            }
            _ => panic!("expected Aabb"),
        }
    }

    #[test]
    fn test_save_load_nonexistent_path() {
        let result = SerScene::load_from_file("/nonexistent/scene.json");
        assert!(result.is_err());
    }
}
