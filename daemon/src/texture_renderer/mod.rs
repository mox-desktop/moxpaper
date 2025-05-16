pub mod cache;
pub mod viewport;

use crate::utils::buffers::{self, GpuBuffer};

#[derive(Default)]
pub struct Buffer<'a> {
    bytes: &'a [u8],
    width: Option<f32>,
    height: Option<f32>,
}

impl<'a> Buffer<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_bytes(&mut self, bytes: &'a [u8]) {
        self.bytes = bytes;
    }

    pub fn set_size(&mut self, width_opt: Option<f32>, height_opt: Option<f32>) {
        self.width = width_opt;
        self.height = height_opt;
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct TextureInstance {
    pub depth: f32,
    pub scale: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub blur: i32,
    pub rect: [f32; 4],
    pub radius: [f32; 4],
    pub container_rect: [f32; 4],
}

pub struct TextureRenderer {
    pub depth_buffer: buffers::DepthBuffer,
    render_pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    vertex_buffer: buffers::VertexBuffer,
    index_buffer: buffers::IndexBuffer,
    instance_buffer: buffers::InstanceBuffer<TextureInstance>,
    prepared_instances: usize,
}

pub struct TextureArea<'a> {
    pub buffer: Buffer<'a>,
    pub radius: [f32; 4],
    pub left: f32,
    pub top: f32,
    pub bounds: TextureBounds,
    pub scale: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub depth: f32,
    pub blur: i32,
}

#[derive(Clone)]
pub struct TextureBounds {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl TextureRenderer {
    pub fn new(
        width: u32,
        height: u32,
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
        cache: &cache::Cache,
    ) -> Self {
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 256,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture_renderer_texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_array_layer: 0,
            array_layer_count: Some(256),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture_renderer_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("texture_bind_group"),
        });

        let render_pipeline = cache.get_or_create_pipeline(
            device,
            texture_format,
            wgpu::MultisampleState::default(),
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
        );

        let vertex_buffer = buffers::VertexBuffer::new(
            device,
            &[
                buffers::Vertex {
                    position: [0.0, 0.0],
                },
                buffers::Vertex {
                    position: [1.0, 0.0],
                },
                buffers::Vertex {
                    position: [0.0, 1.0],
                },
                buffers::Vertex {
                    position: [1.0, 1.0],
                },
            ],
        );

        let index_buffer = buffers::IndexBuffer::new(device, &[0, 1, 3, 3, 2, 0]);

        let instance_buffer = buffers::InstanceBuffer::new(device, &[]);

        let depth_buffer = buffers::DepthBuffer::new(device, width, height);

        Self {
            prepared_instances: 0,
            instance_buffer,
            render_pipeline,
            texture,
            index_buffer,
            vertex_buffer,
            bind_group,
            depth_buffer,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &viewport::Viewport,
        textures: &[TextureArea],
    ) {
        self.prepared_instances = textures.len();

        if textures.is_empty() {
            return;
        }

        let mut instances = Vec::new();

        textures.iter().enumerate().for_each(|(i, texture)| {
            let width = texture
                .buffer
                .width
                .unwrap_or(viewport.resolution().width as f32);
            let height = texture
                .buffer
                .height
                .unwrap_or(viewport.resolution().height as f32);

            instances.push(TextureInstance {
                scale: texture.scale,
                rect: [
                    texture.left,
                    viewport.resolution().height as f32 - texture.top - height,
                    width,
                    height,
                ],
                container_rect: [
                    texture.bounds.left as f32,
                    -(viewport.resolution().height as f32 - texture.bounds.top as f32 - height),
                    texture.bounds.right as f32,
                    texture.bounds.bottom as f32,
                ],
                opacity: texture.opacity,
                radius: texture.radius,
                rotation: texture.rotation,
                blur: texture.blur,
                depth: texture.depth,
            });

            let bytes_per_row = (4 * viewport.resolution().width).div_ceil(256) * 256;

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                texture.buffer.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: viewport.resolution().width,
                    height: viewport.resolution().height,
                    depth_or_array_layers: 1,
                },
            );
        });

        let instance_buffer_size = std::mem::size_of::<TextureInstance>() * instances.len();

        if self.instance_buffer.size() < instance_buffer_size as u32 {
            self.instance_buffer =
                buffers::InstanceBuffer::with_size(device, instance_buffer_size as u64);
        }

        self.instance_buffer.write(queue, &instances);
    }

    pub fn render(&self, render_pass: &mut wgpu::RenderPass, viewport: &viewport::Viewport) {
        if self.prepared_instances == 0 {
            return;
        }

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &viewport.bind_group, &[]);

        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        render_pass.draw_indexed(
            0..self.index_buffer.size(),
            0,
            0..self.prepared_instances as u32,
        );
    }
}
