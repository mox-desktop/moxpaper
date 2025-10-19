pub mod wgpu_surface;

use crate::{
    Moxpaper,
    animation::{self, FrameData, bezier::BezierBuilder},
};
use calloop::LoopHandle;
use common::{
    image_data::ImageData,
    ipc::{BezierChoice, OutputInfo, ResizeStrategy},
};
use moxui::{
    texture_renderer::{self, TextureArea, TextureBounds},
    viewport,
};
use std::sync::Arc;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_output, wl_surface},
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

pub struct Output {
    pub id: u32,
    wgpu: Option<wgpu_surface::WgpuSurface>,
    layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    surface: wl_surface::WlSurface,
    wl_output: wl_output::WlOutput,
    pub previous_image: Option<(ImageData, FrameData)>,
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
        Self {
            id,
            wl_output: output,
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

        let mut textures = Vec::new();

        let frame_data = self.animation.frame_data().unwrap_or_default();
        if let Some(prev_texture) = self.previous_image.as_ref() {
            let frame_data = prev_texture.1;
            let mut buffer = texture_renderer::Buffer::new(
                prev_texture.0.width() as f32,
                prev_texture.0.height() as f32,
            );
            buffer.set_bytes(prev_texture.0.data());
            buffer.set_brightness(frame_data.filters.brightness);
            buffer.set_contrast(frame_data.filters.contrast);
            buffer.set_saturation(frame_data.filters.saturation);
            buffer.set_hue_rotate(frame_data.filters.hue_rotate);
            buffer.set_sepia(frame_data.filters.sepia);
            buffer.set_invert(frame_data.filters.invert);
            buffer.set_grayscale(frame_data.filters.grayscale);
            buffer.set_blur(frame_data.filters.blur);
            buffer.set_opacity(frame_data.filters.opacity);
            buffer.set_scale(frame_data.transforms.scale_x, frame_data.transforms.scale_y);
            let color = frame_data.filters.blur_color;
            buffer.set_blur_color(color[0], color[1], color[2], color[3]);

            let prev_texture_area = TextureArea {
                buffer,
                radius: frame_data.radius,
                left: frame_data.transforms.translate[0] * self.info.width as f32,
                top: frame_data.transforms.translate[1] * self.info.height as f32,
                scale: 1.0,
                bounds: TextureBounds {
                    left: (frame_data.clip.left * self.info.width as f32) as u32,
                    top: (frame_data.clip.top * self.info.height as f32) as u32,
                    right: (frame_data.clip.right * self.info.width as f32) as u32,
                    bottom: (frame_data.clip.bottom * self.info.height as f32) as u32,
                },
                rotation: frame_data.rotation,
                skew: [frame_data.transforms.skew_x, frame_data.transforms.skew_y],
                depth: 0.5,
            };

            textures.push(prev_texture_area);
        }

        let mut buffer =
            texture_renderer::Buffer::new(texture.width() as f32, texture.height() as f32);
        buffer.set_bytes(texture.data());
        buffer.set_scale(frame_data.transforms.scale_x, frame_data.transforms.scale_y);
        buffer.set_brightness(frame_data.filters.brightness);
        buffer.set_contrast(frame_data.filters.contrast);
        buffer.set_saturation(frame_data.filters.saturation);
        buffer.set_hue_rotate(frame_data.filters.hue_rotate);
        buffer.set_sepia(frame_data.filters.sepia);
        buffer.set_invert(frame_data.filters.invert);
        buffer.set_grayscale(frame_data.filters.grayscale);
        buffer.set_blur(frame_data.filters.blur);
        buffer.set_opacity(frame_data.filters.opacity);
        let color = frame_data.filters.blur_color;
        buffer.set_blur_color(color[0], color[1], color[2], color[3]);

        let texture_area = TextureArea {
            buffer,
            radius: frame_data.radius,
            left: frame_data.transforms.translate[0] * self.info.width as f32,
            top: frame_data.transforms.translate[1] * self.info.height as f32,
            scale: 1.0,
            bounds: TextureBounds {
                left: (frame_data.clip.left * self.info.width as f32) as u32,
                top: (frame_data.clip.top * self.info.height as f32) as u32,
                right: (frame_data.clip.right * self.info.width as f32) as u32,
                bottom: (frame_data.clip.bottom * self.info.height as f32) as u32,
            },
            rotation: frame_data.rotation,
            skew: [frame_data.transforms.skew_x, frame_data.transforms.skew_y],
            depth: 0.9,
        };

        textures.push(texture_area);

        let surface_texture = wgpu
            .surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = wgpu.device.create_command_encoder(&Default::default());

        wgpu.texture_renderer
            .prepare(&wgpu.device, &wgpu.queue, &textures);

        wgpu.texture_renderer
            .render(&texture_view, &mut encoder, &wgpu.viewport);

        wgpu.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Moxpaper {
    fn event(
        state: &mut Self,
        wl_output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| &output.wl_output == wl_output)
        else {
            return;
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
                    state.config.power_preference.as_ref(),
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

        wgpu.viewport
            .update(&wgpu.queue, viewport::Resolution { width, height });
        wgpu.texture_renderer.resize(
            &wgpu.device,
            wgpu.config.format,
            width as f32,
            height as f32,
        );

        output.layer_surface.ack_configure(serial);

        let wallpaper = state
            .assets
            .get(&output.info.name, output.info.width, output.info.height);

        if let Some(wallpaper) = wallpaper {
            let resized = match wallpaper.resize {
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
            };

            if let Ok(resized) = resized {
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
                    BezierChoice::Named(bezier) => {
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
                if let Some(image) = output.target_image.take() {
                    output.previous_image =
                        Some((image, output.animation.frame_data().unwrap_or_default()));
                }
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
