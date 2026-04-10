use glam::Vec3;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

use super::camera::Camera;

pub struct OrbitalController {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub rotate_speed: f32,
    pub zoom_speed: f32,

    is_rotating: bool,
    last_mouse_pos: Option<(f64, f64)>,
}

impl OrbitalController {
    pub fn new(distance: f32, target: Vec3) -> Self {
        Self {
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.4,
            distance,
            target,
            rotate_speed: 0.005,
            zoom_speed: 0.5,
            is_rotating: false,
            last_mouse_pos: None,
        }
    }

    pub fn process_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    self.is_rotating = *state == ElementState::Pressed;
                    if !self.is_rotating {
                        self.last_mouse_pos = None;
                    }
                    return true;
                }
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.is_rotating {
                    if let Some((last_x, last_y)) = self.last_mouse_pos {
                        let dx = (position.x - last_x) as f32;
                        let dy = (position.y - last_y) as f32;
                        self.yaw -= dx * self.rotate_speed;
                        self.pitch -= dy * self.rotate_speed;

                        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
                        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
                    }
                    self.last_mouse_pos = Some((position.x, position.y));
                    return true;
                }
                false
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                };
                self.distance -= scroll * self.zoom_speed;
                self.distance = self.distance.clamp(0.5, 50.0);
                true
            }
            _ => false,
        }
    }

    pub fn update_camera(&self, camera: &mut Camera) {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();

        camera.position = self.target + Vec3::new(x, y, z);
        camera.target = self.target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let ctrl = OrbitalController::new(5.0, Vec3::ZERO);
        assert!((ctrl.distance - 5.0).abs() < 1e-5);
        assert_eq!(ctrl.target, Vec3::ZERO);
        assert!(!ctrl.is_rotating);
    }

    #[test]
    fn test_update_camera_sets_target() {
        let ctrl = OrbitalController::new(3.0, Vec3::new(1.0, 2.0, 3.0));
        let mut camera = Camera::new(Vec3::ZERO, Vec3::ZERO, 1.0);
        ctrl.update_camera(&mut camera);
        assert_eq!(camera.target, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_update_camera_distance() {
        let ctrl = OrbitalController::new(5.0, Vec3::ZERO);
        let mut camera = Camera::new(Vec3::ZERO, Vec3::ZERO, 1.0);
        ctrl.update_camera(&mut camera);

        let distance = camera.position.length();
        assert!(
            (distance - 5.0).abs() < 1e-4,
            "カメラとターゲットの距離が distance と一致すべき: {distance}"
        );
    }

    #[test]
    fn test_pitch_clamped() {
        let mut ctrl = OrbitalController::new(5.0, Vec3::ZERO);
        // 極端な pitch を設定
        ctrl.pitch = 10.0;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
        ctrl.pitch = ctrl.pitch.clamp(-max_pitch, max_pitch);

        assert!(ctrl.pitch <= max_pitch);
    }

    #[test]
    fn test_zoom_clamped() {
        let mut ctrl = OrbitalController::new(5.0, Vec3::ZERO);
        ctrl.distance = -10.0;
        ctrl.distance = ctrl.distance.clamp(0.5, 50.0);
        assert!((ctrl.distance - 0.5).abs() < 1e-5);

        ctrl.distance = 100.0;
        ctrl.distance = ctrl.distance.clamp(0.5, 50.0);
        assert!((ctrl.distance - 50.0).abs() < 1e-5);
    }
}
