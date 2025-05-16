pub mod wgpu_surface;

use std::sync::Arc;

use crate::{
    animation::{self, bezier::BezierBuilder},
    texture_renderer::{self, TextureArea, TextureBounds},
    Moxpaper,
};
use calloop::LoopHandle;
use common::{
    image_data::ImageData,
    ipc::{BezierChoice, OutputInfo, ResizeStrategy},
};
use wayland_client::{
    protocol::{wl_output, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer,
    zwlr_layer_surface_v1::{self, Anchor},
};

pub struct Output {
    pub id: u32,
    wgpu: Option<wgpu_surface::WgpuSurface>,
    layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    surface: wl_surface::WlSurface,
    output: wl_output::WlOutput,
    pub previous_image: Option<ImageData>,
    pub target_image: Option<ImageData>,
    pub info: OutputInfo,
    pub animation: animation::Animation,
}

impl Output {
    pub fn new(
        output: wl_output::WlOutput,
        surface: wl_surface::WlSurface,
        layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        loop_handle: LoopHandle<'static, Moxpaper>,
        id: u32,
    ) -> Self {
        layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::all());
        layer_surface.set_exclusive_zone(-1);

        Self {
            id,
            output,
            layer_surface,
            surface,
            info: OutputInfo::default(),
            wgpu: None,
            animation: animation::Animation::new(loop_handle),
            previous_image: None,
            target_image: None,
        }
    }

    pub fn render(&mut self) {
        let Some(wgpu) = self.wgpu.as_mut() else {
            return;
        };

        let Some(texture) = self.target_image.as_ref() else {
            return;
        };

        let surface_texture = wgpu
            .surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = wgpu.device.create_command_encoder(&Default::default());
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: wgpu.texture_renderer.depth_buffer.view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let mut textures = Vec::new();

        let transform = self.animation.calculate_transform().unwrap_or_default();
        if let Some(prev_texture) = self.previous_image.as_ref() {
            let mut buffer = texture_renderer::Buffer::new();
            buffer.set_bytes(prev_texture.data());
            buffer.set_size(Some(self.info.width as f32), Some(self.info.height as f32));

            let prev_texture_area = TextureArea {
                buffer,
                radius: [0.; 4],
                left: 0.,
                top: 0.,
                scale: self.info.scale as f32,
                bounds: TextureBounds {
                    left: 0,
                    top: 0,
                    right: self.info.width,
                    bottom: self.info.height,
                },
                opacity: 1.0,
                rotation: 0.,
                depth: 1.0,
                blur: 0,
            };
            textures.push(prev_texture_area);
        }

        let mut buffer = texture_renderer::Buffer::new();
        buffer.set_bytes(texture.data());
        buffer.set_size(
            Some(transform.extents.width * self.info.width as f32),
            Some(transform.extents.height * self.info.height as f32),
        );

        let texture_area = TextureArea {
            buffer,
            radius: std::array::from_fn(|i| transform.radius[i] * 50.),
            left: transform.extents.x * self.info.width as f32,
            top: transform.extents.y * self.info.height as f32,
            scale: self.info.scale as f32,
            bounds: TextureBounds {
                left: (transform.clip.left * self.info.width as f32) as u32,
                top: (transform.clip.top * self.info.height as f32) as u32,
                right: (transform.clip.right * self.info.width as f32) as u32,
                bottom: (transform.clip.bottom * self.info.height as f32) as u32,
            },
            opacity: transform.opacity,
            rotation: 360. * transform.rotation,
            depth: 0.9,
            blur: transform.blur,
        };

        textures.push(texture_area);

        wgpu.texture_renderer
            .prepare(&wgpu.device, &wgpu.queue, &wgpu.viewport, &textures);

        wgpu.texture_renderer
            .render(&mut render_pass, &wgpu.viewport);

        drop(render_pass); // Drop renderpass and release mutable borrow on encoder

        wgpu.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}

impl Dispatch<wl_output::WlOutput, u32> for Moxpaper {
    fn event(
        state: &mut Self,
        wl_output: &wl_output::WlOutput,
        event: wl_output::Event,
        id: &u32,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        let output = match state.outputs.iter_mut().find(|o| o.output == *wl_output) {
            Some(o) => o,
            None => {
                let compositor = match state.compositor.as_ref() {
                    Some(comp) => comp,
                    None => {
                        log::error!("wl_compositor not initialized");
                        return;
                    }
                };

                let surface = compositor.create_surface(&state.qh, ());

                let layer_shell = match state.layer_shell.as_ref() {
                    Some(shell) => shell,
                    None => {
                        log::error!("wlr_layer_shell not initialized");
                        return;
                    }
                };

                let layer_surface = layer_shell.get_layer_surface(
                    &surface,
                    Some(wl_output),
                    Layer::Background,
                    "moxpaper".into(),
                    &state.qh,
                    (),
                );

                layer_surface.set_anchor(Anchor::all());
                let output = Output::new(
                    wl_output.clone(),
                    surface,
                    layer_surface,
                    state.handle.clone(),
                    *id,
                );
                state.outputs.push(output);

                state.outputs.last_mut().unwrap()
            }
        };

        match event {
            wl_output::Event::Mode {
                flags: _,
                width,
                height,
                refresh: _,
            } => {
                output.info.width = width as u32;
                output.info.height = height as u32;
            }
            wl_output::Event::Scale { factor } => {
                output.info.scale = factor;
            }
            wl_output::Event::Name { name } => {
                output.info.name = name.into();
            }
            wl_output::Event::Done => {
                output
                    .layer_surface
                    .set_size(output.info.width, output.info.height);
                output.surface.commit();
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for Moxpaper {
    fn event(
        _state: &mut Self,
        _proxy: &wl_surface::WlSurface,
        _event: wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Moxpaper {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.layer_surface == *layer_surface)
        else {
            return;
        };

        let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        else {
            return;
        };

        let wgpu = match output.wgpu.as_mut() {
            Some(wgpu) => wgpu,
            None => {
                let wgpu_surface = wgpu_surface::WgpuSurface::new(
                    &output.surface,
                    state.wgpu.raw_display_handle,
                    &state.wgpu.instance,
                    width,
                    height,
                )
                .ok();

                output.wgpu = wgpu_surface;
                output.wgpu.as_mut().unwrap()
            }
        };

        output.info.width = width;
        output.info.height = height;

        wgpu.config.width = width;
        wgpu.config.height = height;

        wgpu.surface.configure(&wgpu.device, &wgpu.config);

        output.layer_surface.ack_configure(serial);

        let wallpaper = state
            .assets
            .get(&output.info.name, output.info.width, output.info.height);

        if let Some(wallpaper) = wallpaper {
            if let Ok(resized) = match wallpaper.resize {
                ResizeStrategy::No => {
                    Ok(wallpaper
                        .image
                        .pad(output.info.width, output.info.height, &[0, 0, 0]))
                }
                ResizeStrategy::Fit => wallpaper
                    .image
                    .resize_to_fit(output.info.width, output.info.height),
                ResizeStrategy::Crop => wallpaper
                    .image
                    .resize_crop(output.info.width, output.info.height),
                ResizeStrategy::Stretch => wallpaper
                    .image
                    .resize_stretch(output.info.width, output.info.height),
            } {
                let bezier = wallpaper
                    .transition
                    .bezier
                    .as_ref()
                    .unwrap_or(&state.config.default_bezier);
                let bezier = match bezier {
                    BezierChoice::Linear => BezierBuilder::new().linear(),
                    BezierChoice::Ease => BezierBuilder::new().ease(),
                    BezierChoice::EaseIn => BezierBuilder::new().ease_in(),
                    BezierChoice::EaseOut => BezierBuilder::new().ease_out(),
                    BezierChoice::EaseInOut => BezierBuilder::new().ease_in_out(),
                    BezierChoice::Custom(curve) => {
                        BezierBuilder::new().custom(curve.0, curve.1, curve.2, curve.3)
                    }
                    BezierChoice::Named(ref bezier) => {
                        if let Some(a) = state.config.bezier.get(bezier) {
                            BezierBuilder::new().custom(a.0, a.1, a.2, a.3)
                        } else {
                            log::warn!("Bezier: {bezier} not found");
                            BezierBuilder::new().linear()
                        }
                    }
                };
                let extents = animation::Extents {
                    x: 0.,
                    y: 0.,
                    width: output.info.width as f32,
                    height: output.info.height as f32,
                };

                output.target_image = Some(resized);
                output.animation.start(
                    &output.info.name,
                    animation::TransitionConfig {
                        enabled_transition_types: state
                            .config
                            .enabled_transition_types
                            .as_ref()
                            .map(Arc::clone),
                        transition_type: wallpaper
                            .transition
                            .transition_type
                            .unwrap_or(state.config.default_transition_type.clone()),
                        fps: wallpaper.transition.fps.or(state.config.default_fps),
                        duration: wallpaper
                            .transition
                            .duration
                            .unwrap_or(state.config.default_transition_duration),
                        bezier,
                    },
                    extents,
                    state.config.lua_env.clone(),
                );
            }
        }
    }
}
