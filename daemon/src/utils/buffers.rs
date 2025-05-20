use std::rc::Rc;

use wgpu::util::DeviceExt;

pub trait DataDescription {
    const ATTRIBS: &'static [wgpu::VertexAttribute];
    const STEP_MODE: wgpu::VertexStepMode;

    fn desc() -> wgpu::VertexBufferLayout<'static>
    where
        Self: Sized,
    {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: Self::STEP_MODE,
            attributes: Self::ATTRIBS,
        }
    }
}

pub trait GpuBuffer {
    type DataType;

    fn new(device: &wgpu::Device, data: &[Self::DataType]) -> Self;

    fn with_size(device: &wgpu::Device, size: u64) -> Self
    where
        Self: Sized;

    fn size(&self) -> u32;

    fn slice(&self, bounds: impl std::ops::RangeBounds<wgpu::BufferAddress>) -> wgpu::BufferSlice;

    fn write(&mut self, queue: &wgpu::Queue, data: &[Self::DataType]);
}

pub struct IndexBuffer {
    buffer: wgpu::Buffer,
    indices: Box<[u16]>,
}

impl GpuBuffer for IndexBuffer {
    type DataType = u16;

    fn new(device: &wgpu::Device, data: &[Self::DataType]) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("IndexBuffer"),
                usage: wgpu::BufferUsages::INDEX,
                contents: bytemuck::cast_slice(data),
            }),
            indices: data.into(),
        }
    }

    fn with_size(device: &wgpu::Device, size: u64) -> Self
    where
        Self: Sized,
    {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("IndexBuffer"),
            size,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            indices: Box::new([]),
        }
    }

    fn size(&self) -> u32 {
        self.indices.len() as u32
    }

    fn slice(&self, bounds: impl std::ops::RangeBounds<wgpu::BufferAddress>) -> wgpu::BufferSlice {
        self.buffer.slice(bounds)
    }

    fn write(&mut self, _: &wgpu::Queue, _: &[Self::DataType]) {}
}

pub struct InstanceBuffer<T> {
    buffer: wgpu::Buffer,
    instances: Box<[T]>,
}

impl<T> GpuBuffer for InstanceBuffer<T>
where
    T: bytemuck::Pod,
{
    type DataType = T;

    fn new(device: &wgpu::Device, data: &[Self::DataType]) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("InstanceBuffer"),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                contents: bytemuck::cast_slice(data),
            }),
            instances: data.into(),
        }
    }

    fn with_size(device: &wgpu::Device, size: u64) -> Self
    where
        Self: Sized,
    {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("InstanceBuffer"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        InstanceBuffer {
            buffer,
            instances: Box::new([]),
        }
    }

    fn size(&self) -> u32 {
        self.instances.len() as u32
    }

    fn slice(&self, bounds: impl std::ops::RangeBounds<wgpu::BufferAddress>) -> wgpu::BufferSlice {
        self.buffer.slice(bounds)
    }

    fn write(&mut self, queue: &wgpu::Queue, data: &[Self::DataType]) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));

        self.instances = data.into();
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
}

impl DataDescription for Vertex {
    const ATTRIBS: &'static [wgpu::VertexAttribute] = &wgpu::vertex_attr_array![0 => Float32x2];
    const STEP_MODE: wgpu::VertexStepMode = wgpu::VertexStepMode::Vertex;
}

pub struct VertexBuffer {
    buffer: wgpu::Buffer,
    vertices: Box<[Vertex]>,
}

impl GpuBuffer for VertexBuffer {
    type DataType = Vertex;

    fn new(device: &wgpu::Device, data: &[Self::DataType]) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("VertexBuffer"),
                usage: wgpu::BufferUsages::VERTEX,
                contents: bytemuck::cast_slice(data),
            }),
            vertices: data.into(),
        }
    }

    fn with_size(device: &wgpu::Device, size: u64) -> Self
    where
        Self: Sized,
    {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VertexBuffer"),
            size,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            vertices: Box::new([]),
        }
    }

    fn size(&self) -> u32 {
        self.vertices.len() as u32
    }

    fn slice(&self, bounds: impl std::ops::RangeBounds<wgpu::BufferAddress>) -> wgpu::BufferSlice {
        self.buffer.slice(bounds)
    }

    fn write(&mut self, _: &wgpu::Queue, _: &[Self::DataType]) {}
}

pub struct DepthBuffer {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl DepthBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("DepthBuffer"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            _texture: texture,
            view,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

pub struct StorageBuffer<T> {
    pub storage: Rc<[T]>,
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl<T> StorageBuffer<T> {
    pub fn new(device: &wgpu::Device, instance_data: Rc<[T]>) -> Self
    where
        T: bytemuck::Pod,
    {
        let storage_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Storage buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 1,
                resource: storage_buffer.as_entire_binding(),
            }],
            label: Some("Instance data buffer"),
        });

        Self {
            storage: instance_data,
            buffer: storage_buffer,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn len(&self) -> u32 {
        self.storage.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
