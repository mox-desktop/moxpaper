pub mod wgpu_surface;

use crate::{
    texture_renderer::{TextureArea, TextureBounds},
    Moxpaper,
};
use common::{
    image_data::ImageData,
    ipc::{OutputInfo, ResizeStrategy},
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

        wgpu.texture_renderer
            .resize(&wgpu.queue, width as f32, height as f32);

        wgpu.surface.configure(&wgpu.device, &wgpu.config);

        output.layer_surface.ack_configure(serial);

        let image = state
            .assets
            .get(&output.info.name, output.info.width, output.info.height);

        if let Some(image) = image {
            let resized = match image.1 {
                ResizeStrategy::No => {
                    Ok(image
                        .0
                        .pad(output.info.width, output.info.height, &[0, 0, 0]))
                }
                ResizeStrategy::Fit => image.0.resize_to_fit(output.info.width, output.info.height),
                ResizeStrategy::Crop => image.0.resize_crop(output.info.width, output.info.height),
                ResizeStrategy::Stretch => image
                    .0
                    .resize_stretch(output.info.width, output.info.height),
            };

            output.render(&resized.unwrap());
        }
    }
}
