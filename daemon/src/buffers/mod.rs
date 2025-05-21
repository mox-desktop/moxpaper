pub mod instance;

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
                contents: unsafe {
                    std::slice::from_raw_parts(
                        data as *const [Self::DataType] as *const u8,
                        std::mem::size_of_val(data),
                    )
                },
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

#[repr(C)]
#[derive(Copy, Clone)]
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
                contents: unsafe {
                    std::slice::from_raw_parts(
                        data as *const [Self::DataType] as *const u8,
                        std::mem::size_of_val(data),
                    )
                },
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

pub struct StorageBuffer<T>
where
    T: Clone,
{
    _data: Rc<[T]>,
    pub buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl<T> StorageBuffer<T>
where
    T: Clone,
{
    const VISIBILITY: wgpu::ShaderStages = wgpu::ShaderStages::VERTEX_FRAGMENT;

    pub fn group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn new(device: &wgpu::Device, data: &[T]) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Storage Buffer"),
            contents: unsafe {
                std::slice::from_raw_parts(
                    data as *const [T] as *const u8,
                    std::mem::size_of_val(data),
                )
            },
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Storage Buffer Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: Self::VISIBILITY,
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
                resource: buffer.as_entire_binding(),
            }],
            label: Some("Storage Buffer Bind Group"),
        });

        Self {
            _data: data.into(),
            buffer,
            bind_group_layout,
            bind_group,
        }
    }
}
