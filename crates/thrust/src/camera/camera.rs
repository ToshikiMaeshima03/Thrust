use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn new(position: Vec3, target: Vec3, aspect: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y: 45.0,
            aspect,
            z_near: 0.1,
            z_far: 100.0,
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov_y.to_radians(),
            self.aspect,
            self.z_near,
            self.z_far,
        )
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_new() {
        let cam = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::ZERO, 16.0 / 9.0);
        assert_eq!(cam.position, Vec3::new(0.0, 5.0, 10.0));
        assert_eq!(cam.target, Vec3::ZERO);
        assert_eq!(cam.up, Vec3::Y);
        assert!((cam.fov_y - 45.0).abs() < 1e-5);
        assert!((cam.z_near - 0.1).abs() < 1e-5);
        assert!((cam.z_far - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_view_matrix_is_finite() {
        let cam = Camera::new(Vec3::new(0.0, 1.0, 3.0), Vec3::ZERO, 1.0);
        let view = cam.view_matrix();
        for col in 0..4 {
            for row in 0..4 {
                assert!(
                    view.col(col)[row].is_finite(),
                    "ビュー行列に NaN/Inf: col={col}, row={row}"
                );
            }
        }
    }

    #[test]
    fn test_projection_matrix_is_finite() {
        let cam = Camera::new(Vec3::ZERO, Vec3::NEG_Z, 16.0 / 9.0);
        let proj = cam.projection_matrix();
        for col in 0..4 {
            for row in 0..4 {
                assert!(
                    proj.col(col)[row].is_finite(),
                    "射影行列に NaN/Inf: col={col}, row={row}"
                );
            }
        }
    }

    #[test]
    fn test_view_transforms_target_to_center() {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 1.0);
        let view = cam.view_matrix();
        // ターゲット (0,0,0) がビュー空間で Z 軸負方向に位置するはず
        let target_view = view.transform_point3(Vec3::ZERO);
        assert!(
            target_view.z < 0.0,
            "ターゲットがカメラの前方に位置すべき: z={}",
            target_view.z
        );
    }

    #[test]
    fn test_view_projection_composition() {
        let cam = Camera::new(Vec3::new(0.0, 1.0, 3.0), Vec3::ZERO, 1.0);
        let vp = cam.view_projection_matrix();
        let expected = cam.projection_matrix() * cam.view_matrix();
        assert!(
            vp.abs_diff_eq(expected, 1e-6),
            "view_projection_matrix は projection * view と一致すべき"
        );
    }
}
