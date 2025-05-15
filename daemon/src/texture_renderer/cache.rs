use std::{
    borrow::Cow,
    ops::Deref,
    sync::{Arc, Mutex},
};

use super::TextureInstance;

#[derive(Debug, Clone)]
pub struct Cache(pub Arc<Inner>);

#[derive(Debug)]
pub struct Inner {
    sampler: wgpu::Sampler,
    shader: wgpu::ShaderModule,
    vertex_buffers: [wgpu::VertexBufferLayout<'static>; 2],
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    cache: Mutex<
        Vec<(
            wgpu::TextureFormat,
            wgpu::MultisampleState,
            Option<wgpu::DepthStencilState>,
            wgpu::RenderPipeline,
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
            format: wgpu::VertexFormat::Float32,
            offset: wgpu::VertexFormat::Float32.size() * 3,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 4,
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 4 + wgpu::VertexFormat::Float32x4.size(),
            shader_location: 7,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: wgpu::VertexFormat::Float32.size() * 4
                + wgpu::VertexFormat::Float32x4.size() * 2,
            shader_location: 8,
        },
    ];

    const VERTEX_ATTRIBUTES: &'static [wgpu::VertexAttribute] = &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];

    pub fn new(device: &wgpu::Device) -> Self {
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("texture renderer shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextureInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: Self::INSTANCE_ATTRIBUTES,
        };

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
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

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            label: Some("Projection Bind Group Layout"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&texture_bind_group_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        Self(Arc::new(Inner {
            sampler,
            shader,
            vertex_buffers: [vertex_buffer_layout, instance_buffer_layout],
            bind_group_layout,
            pipeline_layout,
            cache: Mutex::new(Vec::new()),
        }))
    }

    pub(crate) fn create_uniforms_bind_group(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.0.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("glyphon uniforms bind group"),
        })
    }

    pub(crate) fn get_or_create_pipeline(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        multisample: wgpu::MultisampleState,
        depth_stencil: Option<wgpu::DepthStencilState>,
    ) -> wgpu::RenderPipeline {
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
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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

                cache.push((format, multisample, depth_stencil, pipeline.clone()));

                pipeline
            })
            .clone()
    }
}
