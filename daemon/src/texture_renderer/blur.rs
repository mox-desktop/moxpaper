use crate::utils::buffers::{self, GpuBuffer};

pub struct BlurRenderer {
    pub pipelines: Pipelines,
    pub intermediate_view: wgpu::TextureView,
    pub output_view: wgpu::TextureView,
    pub intermediate_bind_group: wgpu::BindGroup,
    pub output_bind_group: wgpu::BindGroup,
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

        let blur_tex_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let intermediate_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("horizontal_blur_texture"),
            size: blur_tex_size,
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
            size: blur_tex_size,
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
                ],
                label: Some("blur_bind_group_layout"),
            });

        Self {
            pipelines: Pipelines::new(device, pipeline_layout, shader, buffers, format),
            intermediate_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &blur_bind_group_layout,
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
                label: Some("intermediate_bind_group"),
            }),
            output_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &blur_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&output_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
                label: Some("output_bind_group"),
            }),
            intermediate_view,
            output_view,
        }
    }

    pub fn render(
        &self,
        output_texture_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport_bind_group: &wgpu::BindGroup,
        vertex_buffer: &buffers::VertexBuffer,
        index_buffer: &buffers::IndexBuffer,
        instance_buffer: &buffers::InstanceBuffer<super::TextureInstance>,
        instance_index: usize,
    ) {
        {
            let mut horizontal_blur_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("horizontal_blur_pass"),
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

            horizontal_blur_pass.set_pipeline(&self.pipelines.horizontal);
            horizontal_blur_pass.set_bind_group(0, &self.intermediate_bind_group, &[]);
            horizontal_blur_pass.set_bind_group(1, viewport_bind_group, &[]);
            horizontal_blur_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            horizontal_blur_pass.set_vertex_buffer(
                1,
                instance_buffer.slice(
                    (instance_index * std::mem::size_of::<super::TextureInstance>()) as u64
                        ..((instance_index + 1) * std::mem::size_of::<super::TextureInstance>())
                            as u64,
                ),
            );
            horizontal_blur_pass
                .set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            horizontal_blur_pass.draw_indexed(0..index_buffer.size(), 0, 0..1);
        }

        {
            let mut vertical_blur_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vertical_blur_pass"),
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

            vertical_blur_pass.set_pipeline(&self.pipelines.vertical);
            vertical_blur_pass.set_bind_group(0, &self.output_bind_group, &[]);
            vertical_blur_pass.set_bind_group(1, viewport_bind_group, &[]);
            vertical_blur_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            vertical_blur_pass.set_vertex_buffer(
                1,
                instance_buffer.slice(
                    (instance_index * std::mem::size_of::<super::TextureInstance>()) as u64
                        ..((instance_index + 1) * std::mem::size_of::<super::TextureInstance>())
                            as u64,
                ),
            );
            vertical_blur_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            vertical_blur_pass.draw_indexed(0..index_buffer.size(), 0, 0..1);
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
