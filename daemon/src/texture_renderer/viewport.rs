#[derive(PartialEq, Eq, Clone)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

struct Params {
    resolution: Resolution,
    _pad: [u32; 2],
}

pub struct Viewport {
    params: Params,
    buffer: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl Viewport {
    pub fn new(device: &wgpu::Device) -> Self {
        let params = Params {
            resolution: Resolution {
                width: 0,
                height: 0,
            },
            _pad: [0, 0],
        };

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture renderer params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("uniform_bind_group_layout"),
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("uniforms_bind_group"),
        });

        Self {
            params,
            buffer,
            bind_group,
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, resolution: Resolution) {
        if self.params.resolution != resolution {
            self.params.resolution = resolution;
            queue.write_buffer(&self.buffer, 0, unsafe {
                std::slice::from_raw_parts(
                    &self.params as *const Params as *const u8,
                    std::mem::size_of::<Params>(),
                )
            });
        }
    }

    pub fn resolution(&self) -> &Resolution {
        &self.params.resolution
    }
}
