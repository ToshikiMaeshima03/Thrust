use bytemuck::{Pod, Zeroable};

use super::camera::Camera;

/// 拡張カメラユニフォーム (Round 7)
///
/// 通常の view/proj に加え、inverse 各行列・前フレーム行列・ビューポートサイズ・時間を保持する。
/// SSAO/SSR/Decal/Motion Blur など多くのスクリーン空間エフェクトで必要となる情報を集約する。
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub inv_view: [[f32; 4]; 4],
    pub inv_proj: [[f32; 4]; 4],
    /// 前フレームの view_proj (モーションブラー用)
    pub prev_view_proj: [[f32; 4]; 4],
    pub camera_position: [f32; 3],
    pub _pad0: f32,
    /// xy = viewport size (px), zw = inverse viewport size
    pub viewport: [f32; 4],
    /// x = z_near, y = z_far, z = aspect, w = fov_y(rad)
    pub camera_params: [f32; 4],
    /// x = elapsed_time, y = dt, z = frame_index, w = _
    pub time_params: [f32; 4],
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraUniform {
    pub fn new() -> Self {
        let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
        Self {
            view_proj: identity,
            view: identity,
            proj: identity,
            inv_view_proj: identity,
            inv_view: identity,
            inv_proj: identity,
            prev_view_proj: identity,
            camera_position: [0.0; 3],
            _pad0: 0.0,
            viewport: [1.0, 1.0, 1.0, 1.0],
            camera_params: [0.1, 100.0, 1.0, std::f32::consts::FRAC_PI_4],
            time_params: [0.0; 4],
        }
    }

    /// カメラ情報を更新する。`viewport_size` と `dt` は呼び出し元で渡す。
    pub fn update(&mut self, camera: &Camera) {
        // 前フレーム保存 (motion blur 用)
        self.prev_view_proj = self.view_proj;

        let view = camera.view_matrix();
        let proj = camera.projection_matrix();
        let view_proj = proj * view;

        self.view = view.to_cols_array_2d();
        self.proj = proj.to_cols_array_2d();
        self.view_proj = view_proj.to_cols_array_2d();
        self.inv_view = view.inverse().to_cols_array_2d();
        self.inv_proj = proj.inverse().to_cols_array_2d();
        self.inv_view_proj = view_proj.inverse().to_cols_array_2d();
        self.camera_position = camera.position.to_array();

        self.camera_params = [
            camera.z_near,
            camera.z_far,
            camera.aspect,
            camera.fov_y.to_radians(),
        ];
    }

    /// ビューポートサイズと時間情報を更新する。
    pub fn update_viewport_time(
        &mut self,
        width: u32,
        height: u32,
        time: f32,
        dt: f32,
        frame: u32,
    ) {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        self.viewport = [w, h, 1.0 / w, 1.0 / h];
        self.time_params = [time, dt, frame as f32, 0.0];
    }
}
