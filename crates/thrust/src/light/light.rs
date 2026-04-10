use bytemuck::{Pod, Zeroable};

/// 平行光源 (directional light) - 複数同時アクティブ可
pub struct DirectionalLight {
    pub direction: glam::Vec3,
    pub color: glam::Vec3,
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: glam::Vec3::new(1.0, 1.0, 1.0).normalize(),
            color: glam::Vec3::ONE,
            intensity: 0.85,
        }
    }
}

/// 点光源 (point light)
pub struct PointLight {
    pub color: glam::Vec3,
    pub intensity: f32,
    /// 影響範囲（メートル）
    pub range: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: glam::Vec3::ONE,
            intensity: 1.0,
            range: 10.0,
        }
    }
}

/// スポット光源 (spot light)
pub struct SpotLight {
    pub color: glam::Vec3,
    pub intensity: f32,
    pub range: f32,
    /// 内側コーン角度（ラジアン）— ここまでは強度 100%
    pub inner_angle: f32,
    /// 外側コーン角度（ラジアン）— ここからゼロにフェード
    pub outer_angle: f32,
    /// ローカル空間の方向（GlobalTransform で変換）
    pub direction: glam::Vec3,
}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            color: glam::Vec3::ONE,
            intensity: 1.0,
            range: 15.0,
            inner_angle: std::f32::consts::FRAC_PI_6,
            outer_angle: std::f32::consts::FRAC_PI_4,
            direction: glam::Vec3::new(0.0, -1.0, 0.0),
        }
    }
}

/// 環境光
pub struct AmbientLight {
    pub color: glam::Vec3,
    pub intensity: f32,
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: glam::Vec3::ONE,
            intensity: 0.15,
        }
    }
}

/// ライトの最大数
pub const MAX_DIR_LIGHTS: usize = 4;
pub const MAX_POINT_LIGHTS: usize = 32;
pub const MAX_SPOT_LIGHTS: usize = 16;
pub const MAX_LIGHTS_TOTAL: usize = MAX_DIR_LIGHTS + MAX_POINT_LIGHTS + MAX_SPOT_LIGHTS;

/// ライトタイプタグ (シェーダーと一致させる)
pub const LIGHT_TYPE_DIRECTIONAL: u32 = 0;
pub const LIGHT_TYPE_POINT: u32 = 1;
pub const LIGHT_TYPE_SPOT: u32 = 2;

/// GPU 用ライトデータ (storage buffer 1要素 = 48 B)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuLight {
    /// xyz = 位置 (point/spot) または 方向 (directional)
    /// w   = type tag (0=dir, 1=point, 2=spot)
    pub position_or_dir: [f32; 4],
    /// xyz = 色, w = intensity
    pub color_intensity: [f32; 4],
    /// x = range
    /// y = inner_cos (spot)
    /// z = outer_cos (spot)
    /// w = padding
    pub params: [f32; 4],
    /// xyz = spot light の方向 (directional は方向なのでここは未使用、point も未使用)
    pub spot_dir: [f32; 4],
}

impl GpuLight {
    pub fn directional(dir: glam::Vec3, color: glam::Vec3, intensity: f32) -> Self {
        let n = dir.normalize_or_zero();
        Self {
            position_or_dir: [n.x, n.y, n.z, f32::from_bits(LIGHT_TYPE_DIRECTIONAL)],
            color_intensity: [color.x, color.y, color.z, intensity],
            params: [0.0; 4],
            spot_dir: [0.0; 4],
        }
    }

    pub fn point(pos: glam::Vec3, color: glam::Vec3, intensity: f32, range: f32) -> Self {
        Self {
            position_or_dir: [pos.x, pos.y, pos.z, f32::from_bits(LIGHT_TYPE_POINT)],
            color_intensity: [color.x, color.y, color.z, intensity],
            params: [range, 0.0, 0.0, 0.0],
            spot_dir: [0.0; 4],
        }
    }

    pub fn spot(
        pos: glam::Vec3,
        dir: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    ) -> Self {
        let n = dir.normalize_or_zero();
        Self {
            position_or_dir: [pos.x, pos.y, pos.z, f32::from_bits(LIGHT_TYPE_SPOT)],
            color_intensity: [color.x, color.y, color.z, intensity],
            params: [range, inner_angle.cos(), outer_angle.cos(), 0.0],
            spot_dir: [n.x, n.y, n.z, 0.0],
        }
    }
}

/// ライトヘッダー (uniform buffer、storage の前段)
///
/// レイアウト:
/// - ambient: vec4<f32>  (rgb + intensity)
/// - counts:  vec4<u32>  (dir, point, spot, total)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LightsHeader {
    pub ambient: [f32; 4],
    pub counts: [u32; 4],
}

impl LightsHeader {
    pub fn new(ambient: &AmbientLight, dir: u32, point: u32, spot: u32) -> Self {
        Self {
            ambient: [
                ambient.color.x,
                ambient.color.y,
                ambient.color.z,
                ambient.intensity,
            ],
            counts: [dir, point, spot, dir + point + spot],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directional_light_default() {
        let light = DirectionalLight::default();
        assert!((light.direction.length() - 1.0).abs() < 1e-5);
        assert_eq!(light.color, glam::Vec3::ONE);
    }

    #[test]
    fn test_gpu_light_size_alignment() {
        // 16B 倍数 (vec4 が 4 つで 64 B)
        assert_eq!(std::mem::size_of::<GpuLight>(), 64);
        assert_eq!(std::mem::size_of::<GpuLight>() % 16, 0);
    }

    #[test]
    fn test_lights_header_size_alignment() {
        // vec4 + vec4 = 32 B
        assert_eq!(std::mem::size_of::<LightsHeader>(), 32);
        assert_eq!(std::mem::size_of::<LightsHeader>() % 16, 0);
    }

    #[test]
    fn test_gpu_light_directional_tag() {
        let l = GpuLight::directional(glam::Vec3::Y, glam::Vec3::ONE, 1.0);
        let tag = l.position_or_dir[3].to_bits();
        assert_eq!(tag, LIGHT_TYPE_DIRECTIONAL);
    }

    #[test]
    fn test_gpu_light_point_tag() {
        let l = GpuLight::point(glam::Vec3::ZERO, glam::Vec3::ONE, 1.0, 5.0);
        let tag = l.position_or_dir[3].to_bits();
        assert_eq!(tag, LIGHT_TYPE_POINT);
        assert!((l.params[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_gpu_light_spot_tag() {
        let l = GpuLight::spot(
            glam::Vec3::ZERO,
            glam::Vec3::NEG_Y,
            glam::Vec3::ONE,
            1.0,
            10.0,
            std::f32::consts::FRAC_PI_6,
            std::f32::consts::FRAC_PI_4,
        );
        let tag = l.position_or_dir[3].to_bits();
        assert_eq!(tag, LIGHT_TYPE_SPOT);
        // outer cos > 0.7 (45度=0.707)
        assert!(l.params[2] > 0.7 && l.params[2] < 0.8);
    }

    #[test]
    fn test_lights_header_counts() {
        let amb = AmbientLight::default();
        let h = LightsHeader::new(&amb, 1, 4, 2);
        assert_eq!(h.counts, [1, 4, 2, 7]);
    }

    #[test]
    fn test_max_lights_constants() {
        assert_eq!(MAX_LIGHTS_TOTAL, 4 + 32 + 16);
    }
}
