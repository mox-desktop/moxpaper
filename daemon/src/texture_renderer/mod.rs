mod blur;
pub mod viewport;

use crate::buffers::{self, DataDescription, GpuBuffer};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TextureInstance {
    pub scale: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue_rotate: f32,
    pub sepia: f32,
    pub invert: f32,
    pub grayscale: f32,
    pub skew: [f32; 2],
    pub rect: [f32; 4],
    pub radius: [f32; 4],
    pub container_rect: [f32; 4],
    pub shadow: [f32; 3],
}

impl DataDescription for TextureInstance {
    const STEP_MODE: wgpu::VertexStepMode = wgpu::VertexStepMode::Instance;

    const ATTRIBS: &'static [wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
        1  => Float32,
        2  => Float32,
        3  => Float32,
        4  => Float32,
        5  => Float32,
        6  => Float32,
        7  => Float32,
        8  => Float32,
        9  => Float32,
        10 => Float32,
        11 => Float32x2,
        12 => Float32x4,
        13 => Float32x4,
        14 => Float32x4,
        15 => Float32x3,
    ];
}

impl buffers::instance::Instance for TextureInstance {}

#[derive(Debug, Clone, Copy)]
pub struct Filters {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue_rotate: f32,
    pub sepia: f32,
    pub invert: f32,
    pub grayscale: f32,
    pub opacity: f32,
    pub blur: u32,
    pub blur_color: [f32; 4],
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            hue_rotate: 0.0,
            sepia: 0.0,
            invert: 0.0,
            grayscale: 0.0,
            blur: 0,
            blur_color: [0., 0., 0., 0.],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Transforms {
    pub rotate: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub translate: [f32; 2],
}

impl Default for Transforms {
    fn default() -> Self {
        Self {
            rotate: 0.,
            scale_x: 1.,
            scale_y: 1.,
            skew_x: 0.,
            skew_y: 0.,
            translate: [0., 0.],
        }
    }
}

#[derive(Default)]
pub struct Buffer<'a> {
    width: Option<f32>,
    height: Option<f32>,
    skew: [f32; 2],
    bytes: &'a [u8],
    filters: Filters,
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

    pub fn set_skew(&mut self, skew_x: f32, skew_y: f32) {
        self.skew = [skew_x, skew_y];
    }

    pub fn set_opacity(&mut self, val: f32) {
        self.filters.opacity = val;
    }

    pub fn set_brightness(&mut self, val: f32) {
        self.filters.brightness = val;
    }

    pub fn set_contrast(&mut self, val: f32) {
        self.filters.contrast = val;
    }

    pub fn set_saturation(&mut self, val: f32) {
        self.filters.saturation = val;
    }

    pub fn set_hue_rotate(&mut self, deg: f32) {
        self.filters.hue_rotate = deg;
    }

    pub fn set_sepia(&mut self, val: f32) {
        self.filters.sepia = val;
    }

    pub fn set_invert(&mut self, val: f32) {
        self.filters.invert = val;
    }

    pub fn set_grayscale(&mut self, val: f32) {
        self.filters.grayscale = val;
    }

    pub fn set_blur(&mut self, val: u32) {
        self.filters.blur = val;
    }

    pub fn set_blur_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.filters.blur_color = [r, g, b, a];
    }
}

pub struct Pipelines {
    pub standard: wgpu::RenderPipeline,
    pub blur: blur::Pipelines,
}

pub struct TextureRenderer {
    blur: blur::BlurRenderer,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    texture_bind_group: wgpu::BindGroup,
    vertex_buffer: buffers::VertexBuffer,
    index_buffer: buffers::IndexBuffer,
    instance_buffer: buffers::instance::InstanceBuffer<TextureInstance>,
}

pub struct TextureArea<'a> {
    pub left: f32,
    pub top: f32,
    pub scale: f32,
    pub rotation: f32,
    pub bounds: TextureBounds,
    pub skew: [f32; 2],
    pub radius: [f32; 4],
    pub buffer: Buffer<'a>,
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

        let buffers = [buffers::Vertex::desc(), TextureInstance::desc()];

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

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        let instance_buffer = buffers::instance::InstanceBuffer::new(device, &[]);

        Self {
            blur: blur::BlurRenderer::new(device, texture_format, width, height),
            instance_buffer,
            texture,
            texture_bind_group,
            index_buffer,
            vertex_buffer,
            pipeline: standard_pipeline,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &viewport::Viewport,
        textures: &[TextureArea],
    ) {
        let instances = textures
            .iter()
            .enumerate()
            .map(|(i, texture)| {
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

                let width = texture
                    .buffer
                    .width
                    .unwrap_or(viewport.resolution().width as f32);
                let height = texture
                    .buffer
                    .height
                    .unwrap_or(viewport.resolution().height as f32);

                TextureInstance {
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
                    radius: texture.radius,
                    rotation: texture.rotation,
                    opacity: texture.buffer.filters.opacity,
                    brightness: texture.buffer.filters.brightness,
                    contrast: texture.buffer.filters.contrast,
                    saturation: texture.buffer.filters.saturation,
                    hue_rotate: texture.buffer.filters.hue_rotate,
                    sepia: texture.buffer.filters.sepia,
                    invert: texture.buffer.filters.invert,
                    grayscale: texture.buffer.filters.grayscale,
                    shadow: [10., 10., 10.],
                    skew: texture.skew,
                }
            })
            .collect::<Vec<_>>();

        if instances.is_empty() {
            return;
        }

        let instance_buffer_size = std::mem::size_of::<TextureInstance>() * instances.len();

        if self.instance_buffer.size() < instance_buffer_size as u32 {
            self.instance_buffer =
                buffers::instance::InstanceBuffer::with_size(device, instance_buffer_size as u64);
        }

        self.instance_buffer.write(queue, &instances);

        self.blur.prepare(device, queue, viewport, textures);
    }

    pub fn render(
        &self,
        texture_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &viewport::Viewport,
    ) {
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
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &viewport.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(
            0..self.index_buffer.size(),
            0,
            0..self.instance_buffer.size(),
        );

        drop(render_pass);

        self.blur.render(
            texture_view,
            encoder,
            &viewport.bind_group,
            &self.vertex_buffer,
            &self.index_buffer,
        );
    }
}
