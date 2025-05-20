use std::collections::HashMap;

use crate::buffers::{self, GpuBuffer};

pub struct BlurRenderer {
    pub pipelines: Pipelines,
    pub intermediate_view: wgpu::TextureView,
    pub output_view: wgpu::TextureView,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    horizontal_bind_groups: Vec<wgpu::BindGroup>,
    vertical_bind_groups: Vec<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
}

impl BlurRenderer {
    pub fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        buffers: &[wgpu::VertexBufferLayout; 2],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
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

        let blur_bind_group_layout =
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
                label: Some("blur_bind_group_layout"),
            });

        Self {
            blur_bind_group_layout,
            sampler,
            pipelines: Pipelines::new(device, pipeline_layout, shader, buffers, format),
            horizontal_bind_groups: Vec::new(),
            vertical_bind_groups: Vec::new(),
            intermediate_view,
            output_view,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        storage_buffers: &HashMap<u32, (buffers::StorageBuffer<f32>, buffers::StorageBuffer<f32>)>,
        textures: &[super::TextureArea],
    ) {
        self.horizontal_bind_groups.clear();
        self.vertical_bind_groups.clear();

        textures.iter().for_each(|texture| {
            let storage_buffer = &storage_buffers[&texture.blur];

            // Horizontal pass bind group
            let horizontal_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.blur_bind_group_layout,
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
                        resource: storage_buffer.0.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: storage_buffer.1.buffer.as_entire_binding(),
                    },
                ],
                label: Some("horizontal_blur_bg"),
            });

            // Vertical pass bind group
            let vertical_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.blur_bind_group_layout,
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
                        resource: storage_buffer.0.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: storage_buffer.1.buffer.as_entire_binding(),
                    },
                ],
                label: Some("vertical_blur_bg"),
            });

            self.horizontal_bind_groups.push(horizontal_bg);
            self.vertical_bind_groups.push(vertical_bg);
        });
    }

    pub fn render(
        &self,
        output_texture_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport_bind_group: &wgpu::BindGroup,
        vertex_buffer: &buffers::VertexBuffer,
        index_buffer: &buffers::IndexBuffer,
        instance_buffer: &buffers::InstanceBuffer<buffers::TextureInstance>,
        storage_buffers: &HashMap<u32, (buffers::StorageBuffer<f32>, buffers::StorageBuffer<f32>)>,
        instance_index: usize,
        blur: &u32,
    ) {
        let horizontal_bg = &self.horizontal_bind_groups[instance_index];
        let vertical_bg = &self.vertical_bind_groups[instance_index];

        // horizontal blur pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipelines.horizontal);
            pass.set_bind_group(0, horizontal_bg, &[]);
            pass.set_bind_group(1, viewport_bind_group, &[]);
            pass.set_bind_group(2, storage_buffers.get(blur).unwrap().0.group(), &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_vertex_buffer(
                1,
                instance_buffer.slice(
                    (instance_index * std::mem::size_of::<buffers::TextureInstance>()) as u64
                        ..((instance_index + 1) * std::mem::size_of::<buffers::TextureInstance>())
                            as u64,
                ),
            );
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..index_buffer.size(), 0, 0..1);
        }

        // vertical blur pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

            pass.set_pipeline(&self.pipelines.vertical);
            pass.set_bind_group(0, vertical_bg, &[]);
            pass.set_bind_group(1, viewport_bind_group, &[]);
            pass.set_bind_group(2, storage_buffers.get(blur).unwrap().0.group(), &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_vertex_buffer(
                1,
                instance_buffer.slice(
                    (instance_index * std::mem::size_of::<buffers::TextureInstance>()) as u64
                        ..((instance_index + 1) * std::mem::size_of::<buffers::TextureInstance>())
                            as u64,
                ),
            );
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..index_buffer.size(), 0, 0..1);
        }
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
