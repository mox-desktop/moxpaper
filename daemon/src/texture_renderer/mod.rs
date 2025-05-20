mod blur;
pub mod viewport;

use std::collections::HashMap;

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

fn gaussian_kernel_1d(radius: i32, sigma: f32) -> (Vec<f32>, Vec<f32>) {
    use std::f32::consts::PI;

    let mut k_values = Vec::with_capacity((2 * radius + 1) as usize);
    let mut offsets = Vec::with_capacity((2 * radius + 1) as usize);
    let mut intensity = 0.0;

    for y in -radius..=radius {
        let y_f = y as f32;
        let g =
            1.0 / (2.0 * PI * sigma * sigma).sqrt() * (-y_f * y_f / (2.0 * sigma * sigma)).exp();
        k_values.push(g);
        offsets.push(y_f);
        intensity += g;
    }

    let mut final_k_values = Vec::new();
    let mut final_offsets = Vec::new();

    let mut i = 0;
    while i + 1 < k_values.len() {
        let a = k_values[i];
        let b = k_values[i + 1];
        let k = a + b;
        let alpha = a / k;
        let offset = offsets[i] + alpha;
        final_k_values.push(k / intensity);
        final_offsets.push(offset);
        i += 2;
    }

    if i < k_values.len() {
        let a = k_values[i];
        let offset = offsets[i];
        final_k_values.push(a / intensity);
        final_offsets.push(offset);
    }

    (final_k_values, final_offsets)
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct TextureInstance {
    pub scale: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub blur: i32,
    pub rect: [f32; 4],
    pub radius: [f32; 4],
    pub container_rect: [f32; 4],
}

pub struct Pipelines {
    pub standard: wgpu::RenderPipeline,
    pub blur: blur::Pipelines,
}

pub struct TextureRenderer {
    blur: blur::BlurRenderer,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    vertex_buffer: buffers::VertexBuffer,
    index_buffer: buffers::IndexBuffer,
    instance_buffer: buffers::InstanceBuffer<TextureInstance>,
    storage_buffers: HashMap<i32, (buffers::StorageBuffer<f32>, buffers::StorageBuffer<f32>)>,
    prepared_instances: usize,
    prepared_blurs: Vec<i32>,
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
    const INSTANCE_ATTRIBUTES: &'static [wgpu::VertexAttribute] = &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 0,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: wgpu::VertexFormat::Float32.size(),
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: wgpu::VertexFormat::Float32.size() * 2,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Sint32,
            offset: wgpu::VertexFormat::Float32.size() * 2 + wgpu::VertexFormat::Sint32.size(),
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 3 + wgpu::VertexFormat::Sint32.size(),
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 3
                + wgpu::VertexFormat::Float32x4.size()
                + wgpu::VertexFormat::Sint32.size(),
            shader_location: 7,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 3
                + wgpu::VertexFormat::Float32x4.size() * 2
                + wgpu::VertexFormat::Sint32.size(),
            shader_location: 8,
        },
    ];

    const VERTEX_ATTRIBUTES: &'static [wgpu::VertexAttribute] = &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];

    pub fn new(
        width: u32,
        height: u32,
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shader.wgsl"
            ))),
        });

        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextureInstance>() as _,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: Self::INSTANCE_ATTRIBUTES,
        };

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<buffers::Vertex>() as _,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::VERTEX_ATTRIBUTES,
        };

        let buffers = [vertex_buffer_layout, instance_buffer_layout];

        let standard_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("texture renderer pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::default(),
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 2,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture_renderer_texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let texture_bind_groups = Vec::new();

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

        Self {
            blur: blur::BlurRenderer::new(
                device,
                &pipeline_layout,
                &shader,
                &buffers,
                texture_format,
                width,
                height,
            ),
            prepared_instances: 0,
            instance_buffer,
            texture,
            texture_bind_group_layout,
            sampler,
            texture_bind_groups,
            index_buffer,
            vertex_buffer,
            storage_buffers: HashMap::new(),
            pipeline: standard_pipeline,
            prepared_blurs: Vec::new(),
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
        self.texture_bind_groups.clear();
        self.prepared_blurs.clear();

        if textures.is_empty() {
            return;
        }

        let mut instances = Vec::new();

        textures.iter().enumerate().for_each(|(i, texture)| {
            self.prepared_blurs.push(texture.blur);
            let storage_buffer = self.storage_buffers.entry(texture.blur).or_insert_with(|| {
                let (weights, offsets) = gaussian_kernel_1d(texture.blur * 3, texture.blur as f32);
                (
                    buffers::StorageBuffer::new(device, weights.into()),
                    buffers::StorageBuffer::new(device, offsets.into()),
                )
            });

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

            let texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: storage_buffer.0.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: storage_buffer.1.buffer.as_entire_binding(),
                    },
                ],
                label: Some(&format!("texture_bind_group_{i}")),
            });

            self.texture_bind_groups.push(bind_group);
        });

        let instance_buffer_size = std::mem::size_of::<TextureInstance>() * instances.len();

        if self.instance_buffer.size() < instance_buffer_size as u32 {
            self.instance_buffer =
                buffers::InstanceBuffer::with_size(device, instance_buffer_size as u64);
        }

        self.instance_buffer.write(queue, &instances);

        self.blur.prepare(device, &self.storage_buffers, textures);
    }

    pub fn render(
        &self,
        texture_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &viewport::Viewport,
    ) {
        (0..self.prepared_instances).for_each(|index| {
            let blur = self.prepared_blurs[index];
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("standard_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.blur.intermediate_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });

                render_pass.set_pipeline(&self.pipeline);
                render_pass.set_bind_group(0, &self.texture_bind_groups[index], &[]);
                render_pass.set_bind_group(1, &viewport.bind_group, &[]);
                render_pass.set_bind_group(
                    2,
                    &self.storage_buffers.get(&blur).unwrap().0.bind_group,
                    &[],
                );
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(
                    1,
                    self.instance_buffer.slice(
                        (index * std::mem::size_of::<TextureInstance>()) as u64
                            ..((index + 1) * std::mem::size_of::<TextureInstance>()) as u64,
                    ),
                );
                render_pass
                    .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..self.index_buffer.size(), 0, 0..1);
            }

            self.blur.render(
                texture_view,
                encoder,
                &viewport.bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                &self.instance_buffer,
                &self.storage_buffers,
                index,
                &blur,
            );
        });
    }
}
