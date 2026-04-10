// Point/Spot ShadowMap Atlas シャドウ生成シェーダー (Round 7)
//
// 与えられた light view-proj 行列で深度のみを書き出す。
// 各 face/spot ごとに 1 パスを描画する。

struct LightVp {
    matrix: mat4x4<f32>,
};

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> light: LightVp;
@group(1) @binding(0) var<uniform> model_data: ModelUniform;
@group(1) @binding(1) var<storage, read> joint_matrices: array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

fn compute_skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let total = weights.x + weights.y + weights.z + weights.w;
    if total < 0.001 {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }
    return joint_matrices[joints.x] * weights.x
         + joint_matrices[joints.y] * weights.y
         + joint_matrices[joints.z] * weights.z
         + joint_matrices[joints.w] * weights.w;
}

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let skin_mat = compute_skin_matrix(in.joints, in.weights);
    let skinned_pos = (skin_mat * vec4<f32>(in.position, 1.0)).xyz;
    let world_pos = model_data.model * vec4<f32>(skinned_pos, 1.0);
    return light.matrix * world_pos;
}
