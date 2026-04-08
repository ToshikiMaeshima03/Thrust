use glam::Vec3;
use hecs::World;

use crate::camera::camera::Camera;
use crate::math::Aabb;
use crate::physics::collider::{Collider, ColliderShape};
use crate::scene::hierarchy::GlobalTransform;
use crate::scene::transform::Transform;

/// レイ（原点 + 正規化方向）
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// レイキャストヒット結果
#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub entity: hecs::Entity,
    pub distance: f32,
    pub point: Vec3,
}

impl Ray {
    /// 原点と方向からレイを生成（方向は正規化される）
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    /// レイ上の点を取得
    pub fn point_at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }

    /// Ray-AABB 交差判定（スラブ法）
    ///
    /// 交差する場合は入射距離を返す。レイが AABB 内部から始まる場合は 0.0 を返す。
    pub fn intersects_aabb(&self, aabb: &Aabb) -> Option<f32> {
        let inv_dir = Vec3::new(
            1.0 / self.direction.x,
            1.0 / self.direction.y,
            1.0 / self.direction.z,
        );

        let t1 = (aabb.min - self.origin) * inv_dir;
        let t2 = (aabb.max - self.origin) * inv_dir;

        let t_min_v = t1.min(t2);
        let t_max_v = t1.max(t2);

        let t_enter = t_min_v.x.max(t_min_v.y).max(t_min_v.z);
        let t_exit = t_max_v.x.min(t_max_v.y).min(t_max_v.z);

        if t_enter > t_exit || t_exit < 0.0 {
            return None;
        }

        Some(t_enter.max(0.0))
    }

    /// Ray-Sphere 交差判定（二次方程式法）
    ///
    /// 交差する場合は最近接の入射距離を返す。
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> Option<f32> {
        let oc = self.origin - center;
        let a = self.direction.dot(self.direction);
        let b = 2.0 * oc.dot(self.direction);
        let c = oc.dot(oc) - radius * radius;
        let discriminant = b * b - 4.0 * a * c;

        if discriminant < 0.0 {
            return None;
        }

        let sqrt_d = discriminant.sqrt();
        let t1 = (-b - sqrt_d) / (2.0 * a);
        let t2 = (-b + sqrt_d) / (2.0 * a);

        if t1 >= 0.0 {
            Some(t1)
        } else if t2 >= 0.0 {
            Some(t2)
        } else {
            None
        }
    }
}

/// ワールド内の Collider を持つエンティティに対してレイキャストを実行する
///
/// 結果は距離順（近い順）にソートされる。
/// `max_distance` を指定すると、その距離以内のヒットのみ返す。
pub fn ray_cast(world: &World, ray: &Ray, max_distance: f32) -> Vec<RayHit> {
    let mut hits = Vec::new();

    for (entity, collider, transform, global_transform) in world
        .query::<(
            hecs::Entity,
            &Collider,
            &Transform,
            Option<&GlobalTransform>,
        )>()
        .iter()
    {
        let matrix = match global_transform {
            Some(gt) => gt.0,
            None => transform.to_matrix(),
        };

        let hit_distance = match &collider.shape {
            ColliderShape::Aabb(aabb) => {
                let world_aabb = aabb.transformed(&matrix);
                ray.intersects_aabb(&world_aabb)
            }
            ColliderShape::Sphere { center, radius } => {
                let world_center = matrix.transform_point3(*center);
                // スケールを考慮した近似半径（collision_system と同一ロジック）
                let scale = matrix
                    .transform_vector3(Vec3::X)
                    .length()
                    .max(matrix.transform_vector3(Vec3::Y).length())
                    .max(matrix.transform_vector3(Vec3::Z).length());
                let world_radius = radius * scale;
                ray.intersects_sphere(world_center, world_radius)
            }
        };

        if let Some(distance) = hit_distance
            && distance <= max_distance
        {
            hits.push(RayHit {
                entity,
                distance,
                point: ray.point_at(distance),
            });
        }
    }

    hits.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits
}

/// スクリーン座標からワールド空間のレイを生成する
///
/// `screen_x`, `screen_y`: ピクセル座標（左上原点）
/// `screen_width`, `screen_height`: ウィンドウサイズ
/// `camera`: アクティブカメラ
pub fn screen_to_ray(
    screen_x: f32,
    screen_y: f32,
    screen_width: f32,
    screen_height: f32,
    camera: &Camera,
) -> Ray {
    // ピクセル座標を NDC (-1..1) に変換
    let ndc_x = (2.0 * screen_x / screen_width) - 1.0;
    let ndc_y = 1.0 - (2.0 * screen_y / screen_height); // Y 軸反転

    // 逆ビュー射影行列でアンプロジェクト
    let inv_vp = camera.view_projection_matrix().inverse();
    let near_point = inv_vp.project_point3(Vec3::new(ndc_x, ndc_y, -1.0));
    let far_point = inv_vp.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));

    let direction = (far_point - near_point).normalize();
    Ray::new(near_point, direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_aabb_hit() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0));
        let aabb = Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let result = ray.intersects_aabb(&aabb);
        assert!(result.is_some());
        let t = result.unwrap();
        assert!((t - 4.0).abs() < 0.001, "距離は4.0であるべき: {t}");
    }

    #[test]
    fn test_ray_aabb_miss() {
        let ray = Ray::new(Vec3::new(0.0, 5.0, -5.0), Vec3::new(0.0, 0.0, 1.0));
        let aabb = Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let result = ray.intersects_aabb(&aabb);
        assert!(result.is_none());
    }

    #[test]
    fn test_ray_aabb_inside() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let aabb = Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let result = ray.intersects_aabb(&aabb);
        assert!(result.is_some());
        assert!(
            result.unwrap().abs() < 0.001,
            "内部からのレイは距離0.0を返すべき"
        );
    }

    #[test]
    fn test_ray_sphere_hit() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0));
        let result = ray.intersects_sphere(Vec3::ZERO, 1.0);
        assert!(result.is_some());
        let t = result.unwrap();
        assert!((t - 4.0).abs() < 0.001, "距離は4.0であるべき: {t}");
    }

    #[test]
    fn test_ray_sphere_miss() {
        let ray = Ray::new(Vec3::new(0.0, 5.0, -5.0), Vec3::new(0.0, 0.0, 1.0));
        let result = ray.intersects_sphere(Vec3::ZERO, 1.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_ray_sphere_behind() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, 1.0));
        let result = ray.intersects_sphere(Vec3::ZERO, 1.0);
        assert!(result.is_none(), "背後の球はヒットしないべき");
    }
}
