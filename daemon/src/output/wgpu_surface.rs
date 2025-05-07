use crate::texture_renderer;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandWindowHandle};
use std::ptr::NonNull;
use wayland_client::{protocol::wl_surface, Proxy};

pub struct WgpuSurface {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub queue: wgpu::Queue,
    pub device: wgpu::Device,
    pub texture_renderer: texture_renderer::TextureRenderer,
}

impl WgpuSurface {
    pub fn new(
        surface: &wl_surface::WlSurface,
        raw_display_handle: RawDisplayHandle,
        instance: &wgpu::Instance,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.id().as_ptr() as *mut _)
                .ok_or(anyhow::anyhow!("Failed to create window handle pointer"))?,
        ));

        let wgpu_surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle,
                raw_window_handle,
            })?
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&wgpu_surface),
            ..Default::default()
        }))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;

        let surface_caps = wgpu_surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .find(|a| **a == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(&surface_caps.alpha_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: *alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let texture_renderer =
            texture_renderer::TextureRenderer::new(width, height, &device, config.format);

        Ok(Self {
            texture_renderer,
            surface: wgpu_surface,
            config,
            queue,
            device,
        })
    }
}
