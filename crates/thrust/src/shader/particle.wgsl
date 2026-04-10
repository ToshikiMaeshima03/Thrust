// Thrust パーティクルシェーダー

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
    _pad0: f32,
    viewport: vec4<f32>,
    camera_params: vec4<f32>,
    time_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct InstanceInput {
    @location(0) position: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
    instance: InstanceInput,
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    // クアッドの頂点を vertex_index から生成 (2 三角形 = 6 頂点)
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    let offset = offsets[vertex_index];

    // ビルボード: View 行列からカメラの right/up ベクトルを抽出
    let right = vec3<f32>(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
    let up = vec3<f32>(camera.view[0][1], camera.view[1][1], camera.view[2][1]);

    let half_size = instance.size * 0.5;
    let world_pos = instance.position
        + right * offset.x * half_size
        + up * offset.y * half_size;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color = instance.color;
    out.uv = offset * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 円形パーティクル: UV 距離でアルファ減衰
    let dist = length(in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let alpha = smoothstep(1.0, 0.7, dist) * in.color.a;

    if alpha < 0.01 {
        discard;
    }

    return vec4<f32>(in.color.rgb, alpha);
}
