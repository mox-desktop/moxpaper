use wgpu::util::DeviceExt;

pub struct InstanceBuffer<T> {
    buffer: wgpu::Buffer,
    instances: Box<[T]>,
}

impl<T> super::GpuBuffer for InstanceBuffer<T>
where
    T: Instance + Clone,
{
    type DataType = T;

    fn new(device: &wgpu::Device, data: &[Self::DataType]) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("InstanceBuffer"),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                contents: unsafe {
                    std::slice::from_raw_parts(
                        data as *const [Self::DataType] as *const u8,
                        std::mem::size_of_val(data) * data.len(),
                    )
                },
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

    fn slice(
        &self,
        bounds: impl std::ops::RangeBounds<wgpu::BufferAddress>,
    ) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(bounds)
    }

    fn write(&mut self, queue: &wgpu::Queue, data: &[Self::DataType]) {
        queue.write_buffer(&self.buffer, 0, unsafe {
            std::slice::from_raw_parts(
                data as *const [Self::DataType] as *const u8,
                std::mem::size_of_val(data),
            )
        });

        self.instances = data.into();
    }
}

pub trait Instance: super::DataDescription {}
