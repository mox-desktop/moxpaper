pub mod wgpu_surface;

use crate::{
    render_svg,
    texture_renderer::{TextureArea, TextureBounds},
    FallbackData, Moxpaper,
};
use anyhow::Context;
use common::{
    cache::{self, CacheEntry},
    image_data::ImageData,
    ipc::OutputInfo,
};
use image::RgbaImage;
use resvg::usvg;
use std::sync::Arc;
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
    pub info: OutputInfo,
}

impl Output {
    pub fn new(
        output: wl_output::WlOutput,
        surface: wl_surface::WlSurface,
        layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
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
        }
    }

    pub fn render(&mut self, texture: &ImageData) {
        let Some(wgpu) = self.wgpu.as_mut() else {
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
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let texture_area = TextureArea {
            left: 0.,
            top: 0.,
            width: self.info.width as f32,
            height: self.info.height as f32,
            scale: self.info.scale as f32,
            bounds: TextureBounds {
                left: 0,
                top: 0,
                right: self.info.width,
                bottom: self.info.height,
            },
            data: texture.data(),
        };

        wgpu.texture_renderer
            .prepare(&wgpu.device, &wgpu.queue, &[texture_area]);
        wgpu.texture_renderer.render(&mut render_pass);

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
                let output = Output::new(wl_output.clone(), surface, layer_surface, *id);
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
                let (width, height) = (output.info.width, output.info.height);

                if let Some(entry) = cache::load(&output.info.name) {
                    let image_result = match entry {
                        CacheEntry::Path(path) => {
                            if path.extension().is_some_and(|e| e == "svg") {
                                render_svg(&path, width, height)
                            } else {
                                image::open(&path)
                                    .context("Failed to open image {path}")
                                    .map(ImageData::from)
                            }
                        }
                        CacheEntry::Image(image) => Ok(image),
                        CacheEntry::Color(color) => {
                            let rgba_image = RgbaImage::from_pixel(
                                width,
                                height,
                                image::Rgba([color[0], color[1], color[2], 255]),
                            );
                            Ok(ImageData::from(rgba_image))
                        }
                    };

                    if let Ok(img) = image_result {
                        state.images.insert(Arc::clone(&output.info.name), img);
                    }
                }

                output.layer_surface.set_size(width, height);
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

        wgpu.texture_renderer
            .resize(&wgpu.queue, width as f32, height as f32);

        wgpu.surface.configure(&wgpu.device, &wgpu.config);

        output.layer_surface.ack_configure(serial);

        state.outputs.iter_mut().for_each(|output| {
            let image = state.images.get(&output.info.name).cloned().or_else(|| {
                state.fallback.as_ref().map(|fallback| match fallback {
                    FallbackData::Image(image) => image.clone(),
                    FallbackData::Color(color) => {
                        let rgba_image = image::RgbaImage::from_pixel(
                            output.info.width,
                            output.info.height,
                            image::Rgba([color[0], color[1], color[2], 255]),
                        );
                        ImageData::from(rgba_image)
                    }
                    FallbackData::Svg(svg_data) => {
                        let opt = usvg::Options::default();

                        let tree = usvg::Tree::from_data(svg_data, &opt).unwrap();

                        let mut pixmap =
                            tiny_skia::Pixmap::new(output.info.width, output.info.height)
                                .context("Failed to create pixmap")
                                .unwrap();

                        let scale_x = output.info.width as f32 / tree.size().width();
                        let scale_y = output.info.height as f32 / tree.size().height();

                        resvg::render(
                            &tree,
                            tiny_skia::Transform::from_scale(scale_x, scale_y),
                            &mut pixmap.as_mut(),
                        );

                        let image = image::load_from_memory(&pixmap.encode_png().unwrap()).unwrap();

                        ImageData::from(image)
                    }
                })
            });

            if let Some(image) = image {
                match ImageData::resize_to_fit(image, output.info.width, output.info.height) {
                    Ok(resized) => output.render(&resized),
                    Err(e) => log::error!("Failed to resize to fit image: {e}"),
                }
            }
        });
    }
}
