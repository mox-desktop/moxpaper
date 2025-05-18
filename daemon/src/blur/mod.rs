use crate::utils::buffers::{self, DataDescription, GpuBuffer, IndexBuffer, VertexBuffer};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct BlurInstance {
    pub radius: i32,
    pub texture_size: [f32; 2],
    pub direction: [f32; 2],
}

const FULL_SCREEN_QUAD_VERTICES: &[buffers::Vertex] = &[
    buffers::Vertex {
        position: [-1.0, -1.0],
    },
    buffers::Vertex {
        position: [1.0, -1.0],
    },
    buffers::Vertex {
        position: [-1.0, 1.0],
    },
    buffers::Vertex {
        position: [1.0, 1.0],
    },
];

const FULL_SCREEN_QUAD_INDICES: &[u16] = &[0, 1, 2, 2, 1, 3];

pub struct BlurRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: VertexBuffer,
    index_buffer: IndexBuffer,
    intermediate_texture: wgpu::Texture,
    intermediate_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl BlurRenderer {
    pub fn new(
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let intermediate_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur_intermediate_texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let intermediate_view =
            intermediate_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bind_group_layout"),
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
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[buffers::Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&intermediate_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("blur_bind_group"),
        });

        let vertex_buffer = VertexBuffer::new(device, FULL_SCREEN_QUAD_VERTICES);
        let index_buffer = IndexBuffer::new(device, FULL_SCREEN_QUAD_INDICES);

        Self {
            pipeline,
            bind_group,
            vertex_buffer,
            index_buffer,
            intermediate_texture,
            intermediate_view,
            sampler,
            bind_group_layout,
        }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
    ) {
        {
            let mut horizontal_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("horizontal_blur_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.intermediate_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            horizontal_pass.set_pipeline(&self.pipeline);
            horizontal_pass.set_bind_group(0, &self.bind_group, &[]);
            horizontal_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            horizontal_pass
                .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            horizontal_pass.draw_indexed(0..FULL_SCREEN_QUAD_INDICES.len() as u32, 0, 0..1);
        }

        {
            let mut vertical_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vertical_blur_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            vertical_pass.set_pipeline(&self.pipeline);
            vertical_pass.set_bind_group(0, &self.bind_group, &[]);
            vertical_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            vertical_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            vertical_pass.draw_indexed(0..FULL_SCREEN_QUAD_INDICES.len() as u32, 0, 0..1);
        }
    }
}
