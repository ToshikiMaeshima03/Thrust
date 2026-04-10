use wgpu::util::DeviceExt;

pub fn create_uniform_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    data: &T,
    label: &str,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&[*data]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}
