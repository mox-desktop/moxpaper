use std::{
    borrow::Cow,
    ops::Deref,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct PipelineGroup {
    pub standard: wgpu::RenderPipeline,
    pub horizontal_blur: wgpu::RenderPipeline,
    pub vertical_blur: wgpu::RenderPipeline,
}

#[derive(Clone)]
pub struct Cache(pub Arc<Inner>);

pub struct Inner {
    shader: wgpu::ShaderModule,
    vertex_buffers: [wgpu::VertexBufferLayout<'static>; 2],
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    cache: Mutex<
        Vec<(
            wgpu::TextureFormat,
            wgpu::MultisampleState,
            Option<wgpu::DepthStencilState>,
            PipelineGroup,
        )>,
    >,
}

impl Cache {
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
            offset: wgpu::VertexFormat::Float32.size() * 3,
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

    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<super::TextureInstance>() as _,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: Self::INSTANCE_ATTRIBUTES,
        };
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as _,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::VERTEX_ATTRIBUTES,
        };

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

        Self(Arc::new(Inner {
            shader,
            vertex_buffers: [vertex_buffer_layout, instance_buffer_layout],
            uniform_bind_group_layout,
            pipeline_layout,
            cache: Mutex::new(Vec::new()),
        }))
    }

    pub fn create_uniforms_bind_group(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.0.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("uniforms_bind_group"),
        })
    }

    pub(crate) fn get_or_create_pipelines(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        multisample: wgpu::MultisampleState,
        depth_stencil: Option<wgpu::DepthStencilState>,
    ) -> PipelineGroup {
        let Inner {
            cache,
            pipeline_layout,
            shader,
            vertex_buffers,
            ..
        } = self.0.deref();

        let mut cache = cache.lock().expect("Write pipeline cache");

        cache
            .iter()
            .find(|(fmt, ms, ds, _)| fmt == &format && ms == &multisample && ds == &depth_stencil)
            .map(|(_, _, _, p)| p.clone())
            .unwrap_or_else(|| {
                let standard_pipeline =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("texture renderer pipeline"),
                        layout: Some(pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: shader,
                            entry_point: Some("vs_main"),
                            buffers: vertex_buffers,
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: shader,
                            entry_point: Some("fs_main"),
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
                        depth_stencil: depth_stencil.clone(),
                        multisample,
                        multiview: None,
                        cache: None,
                    });

                let horizontal_blur_pipeline =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("horizontal blur pipeline"),
                        layout: Some(pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: shader,
                            entry_point: Some("vs_main"),
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                            buffers: vertex_buffers,
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
                        depth_stencil: depth_stencil.clone(),
                        multisample,
                        multiview: None,
                        cache: None,
                    });

                let vertical_blur_pipeline =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("vertical blur pipeline"),
                        layout: Some(pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: shader,
                            entry_point: Some("vs_main"),
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                            buffers: vertex_buffers,
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
                        depth_stencil: depth_stencil.clone(),
                        multisample,
                        multiview: None,
                        cache: None,
                    });

                let pipeline_group = PipelineGroup {
                    standard: standard_pipeline,
                    horizontal_blur: horizontal_blur_pipeline,
                    vertical_blur: vertical_blur_pipeline,
                };

                cache.push((format, multisample, depth_stencil, pipeline_group.clone()));

                pipeline_group
            })
            .clone()
    }
}
