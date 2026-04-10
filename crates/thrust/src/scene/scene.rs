use bytemuck::{Pod, Zeroable};

use super::transform::Transform;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ModelUniform {
    pub model: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
}

impl ModelUniform {
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            model: transform.to_matrix().to_cols_array_2d(),
            normal_matrix: transform.normal_matrix().to_cols_array_2d(),
        }
    }
}
