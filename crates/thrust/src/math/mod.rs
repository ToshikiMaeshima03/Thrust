mod angle;
mod bounds;
mod geometry;
mod matrix;
mod numeric;
mod quat_utils;
mod random;
mod spline;

// 境界ボリューム
pub use bounds::{Aabb, BoundingSphere};

// 角度ユーティリティ
pub use angle::{deg_to_rad, normalize_angle, rad_to_deg, signed_angle};

// ジオメトリユーティリティ
pub use geometry::{
    barycentric_coords, closest_point_on_line_segment, point_to_line_distance,
    ray_plane_intersection, ray_triangle_intersection, triangle_area, triangle_normal,
};

// 行列ユーティリティ
pub use matrix::{decompose, extract_max_scale, extract_scale, extract_translation};

// 数値ユーティリティ
pub use numeric::{
    inverse_lerp, move_towards, nearly_equal, remap, smootherstep, smoothstep, wrap,
};

// クォータニオンユーティリティ
pub use quat_utils::{look_rotation, spherical_to_cartesian};

// 乱数
pub use random::SimpleRng;

// スプライン (Round 8)
pub use spline::{CatmullRomSpline, CubicBezier};
