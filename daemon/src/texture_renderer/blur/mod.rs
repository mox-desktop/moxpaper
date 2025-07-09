use crate::buffers::{self, DataDescription, GpuBuffer};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BlurInstance {
    pub blur_sigma: u32,
    pub blur_color: [f32; 4],
    pub rect: [f32; 4],
}

impl DataDescription for BlurInstance {
    const STEP_MODE: wgpu::VertexStepMode = wgpu::VertexStepMode::Instance;
    const ATTRIBS: &'static [wgpu::VertexAttribute] =
        &wgpu::vertex_attr_array![2 => Uint32, 3 => Float32x4, 4 => Float32x4];
}

impl buffers::instance::Instance for BlurInstance {}

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

type StorageBuffers = (
    buffers::StorageBuffer<[u32; 2]>,
    buffers::StorageBuffer<f32>,
    buffers::StorageBuffer<f32>,
);

pub struct BlurRenderer {
    pub pipelines: Pipelines,
    pub intermediate_view: wgpu::TextureView,
    pub output_view: wgpu::TextureView,
    pub instance_buffer: buffers::instance::InstanceBuffer<BlurInstance>,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_groups: Option<[wgpu::BindGroup; 2]>,
    storage_buffers: Option<StorageBuffers>,
    sampler: wgpu::Sampler,
}

impl BlurRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let buffers = [buffers::Vertex::desc(), BlurInstance::desc()];

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
            bind_group_layouts: &[&bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shader.wgsl"
            ))),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let intermediate_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("horizontal_blur_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let intermediate_view = intermediate_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vertical_blur_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        Self {
            storage_buffers: None,
            bind_group_layout,
            sampler,
            pipelines: Pipelines::new(device, &pipeline_layout, &shader, &buffers, format),
            bind_groups: None,
            intermediate_view,
            output_view,
            instance_buffer: buffers::instance::InstanceBuffer::new(device, &[]),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &super::viewport::Viewport,
        textures: &[super::TextureArea],
    ) {
        let (metadata, weights, offsets) = textures.iter().fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut metadata, mut weights, mut offsets), texture| {
                let (mut local_weights, mut local_offsets) = gaussian_kernel_1d(
                    (texture.buffer.filters.blur * 3) as i32,
                    texture.buffer.filters.blur as f32,
                );
                metadata.push([texture.buffer.filters.blur, weights.len() as u32]);
                weights.append(&mut local_weights);
                offsets.append(&mut local_offsets);
                (metadata, weights, offsets)
            },
        );

        let metadata = buffers::StorageBuffer::new(device, &metadata);
        let weights = buffers::StorageBuffer::new(device, &weights);
        let offsets = buffers::StorageBuffer::new(device, &offsets);

        self.bind_groups = Some([
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.intermediate_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: metadata.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: weights.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: offsets.buffer.as_entire_binding(),
                    },
                ],
                label: Some("horizontal_blur_bg"),
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.output_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: metadata.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: weights.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: offsets.buffer.as_entire_binding(),
                    },
                ],
                label: Some("vertical_blur_bg"),
            }),
        ]);

        self.storage_buffers = Some((metadata, weights, offsets));

        let instances = textures
            .iter()
            .map(|texture| {
                let width = texture
                    .buffer
                    .width
                    .unwrap_or(viewport.resolution().width as f32);
                let height = texture
                    .buffer
                    .height
                    .unwrap_or(viewport.resolution().height as f32);

                BlurInstance {
                    blur_sigma: texture.buffer.filters.blur,
                    blur_color: texture.buffer.filters.blur_color,
                    rect: [
                        texture.left,
                        viewport.resolution().height as f32 - texture.top - height,
                        width,
                        height,
                    ],
                }
            })
            .collect::<Vec<_>>();

        let instance_buffer_size = std::mem::size_of::<BlurInstance>() * instances.len();

        if self.instance_buffer.size() < instance_buffer_size as u32 {
            self.instance_buffer =
                buffers::instance::InstanceBuffer::with_size(device, instance_buffer_size as u64);
        }

        self.instance_buffer.write(queue, &instances);
    }

    pub fn render(
        &self,
        output_texture_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport_bind_group: &wgpu::BindGroup,
        vertex_buffer: &buffers::VertexBuffer,
        index_buffer: &buffers::IndexBuffer,
    ) {
        let horizontal_bg = &self.bind_groups.as_ref().unwrap()[0];
        let vertical_bg = &self.bind_groups.as_ref().unwrap()[1];

        let mut horizontal_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        horizontal_pass.set_pipeline(&self.pipelines.horizontal);
        horizontal_pass.set_bind_group(0, horizontal_bg, &[]);
        horizontal_pass.set_bind_group(1, viewport_bind_group, &[]);
        horizontal_pass.set_bind_group(2, self.storage_buffers.as_ref().unwrap().0.group(), &[]);
        horizontal_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        horizontal_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        horizontal_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        horizontal_pass.draw_indexed(0..index_buffer.size(), 0, 0..self.instance_buffer.size());
        drop(horizontal_pass);

        let mut vertical_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        vertical_pass.set_pipeline(&self.pipelines.vertical);
        vertical_pass.set_bind_group(0, vertical_bg, &[]);
        vertical_pass.set_bind_group(1, viewport_bind_group, &[]);
        vertical_pass.set_bind_group(2, self.storage_buffers.as_ref().unwrap().0.group(), &[]);
        vertical_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        vertical_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        vertical_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        vertical_pass.draw_indexed(0..index_buffer.size(), 0, 0..self.instance_buffer.size());
    }
}

pub struct Pipelines {
    pub horizontal: wgpu::RenderPipeline,
    pub vertical: wgpu::RenderPipeline,
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        buffers: &[wgpu::VertexBufferLayout; 2],
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            horizontal: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("horizontal blur pipeline"),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_horizontal_blur"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
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
            }),
            vertical: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vertical blur pipeline"),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_vertical_blur"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
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
            }),
        }
    }
}
