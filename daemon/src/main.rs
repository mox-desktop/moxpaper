mod animation;
mod assets;
pub mod buffers;
mod config;
mod output;
mod wgpu_state;

use animation::bezier::BezierBuilder;
use anyhow::Context;
use assets::{AssetsManager, FallbackImage};
use calloop::{EventLoop, LoopHandle, generic::Generic};
use calloop_wayland_source::WaylandSource;
use clap::Parser;
use common::{
    image_data::ImageData,
    ipc::{BezierChoice, Data, Ipc, ResizeStrategy, Server},
};
use config::Config;
use env_logger::Builder;
use image::RgbaImage;
use log::LevelFilter;
use resvg::usvg;
use s3::{Bucket, Region, creds::Credentials};
use std::{
    io::Write,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_compositor, wl_output, wl_registry},
};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1;
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use wgpu_state::WgpuState;

struct Moxpaper {
    output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    compositor: Option<wl_compositor::WlCompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    outputs: Vec<output::Output>,
    wgpu: wgpu_state::WgpuState,
    qh: QueueHandle<Moxpaper>,
    ipc: Ipc<Server>,
    handle: LoopHandle<'static, Self>,
    assets: AssetsManager,
    config: Config,
}

impl Moxpaper {
    fn new(
        conn: &Connection,
        qh: QueueHandle<Self>,
        ipc: Ipc<Server>,
        handle: LoopHandle<'static, Self>,
        config: Config,
    ) -> anyhow::Result<Self> {
        let mut assets = AssetsManager::default();
        config.wallpaper.iter().for_each(|(k, v)| {
            let image = image::open(&v.path);

            if &**k == "any" {
                match image {
                    Ok(img) => assets.set_fallback(FallbackImage::Image(assets::AssetData {
                        image: ImageData::from(img),
                        resize: v.resize,
                        transition: v.transition.clone(),
                    })),
                    Err(e) => log::error!("{e}: {}", v.path.display()),
                }
            } else {
                match image {
                    Ok(img) => assets.insert_asset(
                        Arc::clone(k),
                        assets::AssetData {
                            image: ImageData::from(img),
                            resize: v.resize,
                            transition: v.transition.clone(),
                        },
                    ),
                    Err(e) => log::error!("{e}: {}", v.path.display()),
                }
            }
        });

        Ok(Self {
            config,
            qh,
            ipc,
            handle,
            compositor: None,
            output_manager: None,
            layer_shell: None,
            outputs: Vec::new(),
            wgpu: WgpuState::new(conn)?,
            assets,
        })
    }

    fn render(&mut self) {
        self.outputs.iter_mut().for_each(|output| {
            let wallpaper =
                self.assets
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
                        .unwrap_or(&self.config.default_bezier);
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
                            if let Some(a) = self.config.bezier.get(bezier) {
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
                            enabled_transition_types: self
                                .config
                                .enabled_transition_types
                                .as_ref()
                                .map(Arc::clone),
                            transition_type: wallpaper
                                .transition
                                .transition_type
                                .unwrap_or(self.config.default_transition_type.clone()),
                            fps: wallpaper.transition.fps.or(self.config.default_fps),
                            duration: wallpaper
                                .transition
                                .duration
                                .unwrap_or(self.config.default_transition_duration),
                            bezier,
                        },
                        extents,
                    );
                }
            }
        });
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long, value_enum, help = "Set the log level")]
    log_level: Option<LevelFilter>,

    #[arg(short, long, value_name = "FILE", help = "Path to the config file")]
    config: Option<Box<Path>>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    Builder::new()
        .filter(Some("daemon"), cli.log_level.unwrap_or(LevelFilter::Info))
        .init();

    let config = Config::load(cli.config.as_ref());

    let conn = Connection::connect_to_env().expect("Connection to wayland failed");
    let display = conn.display();

    let event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let ipc = Ipc::server()?;

    let mut event_loop = EventLoop::try_new()?;
    let mut moxpaper = Moxpaper::new(&conn, qh, ipc, event_loop.handle(), config)?;

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Failed to insert Wayland source: {}", e))?;

    let source = unsafe {
        Generic::new(
            calloop::generic::FdWrapper::new(moxpaper.ipc.get_listener().as_raw_fd()),
            calloop::Interest {
                readable: true,
                writable: false,
            },
            calloop::Mode::Level,
        )
    };

    event_loop.handle().insert_source(source, |_, _, state| {
        let mut stream = state.ipc.accept_connection();
        log::info!("Connection added");

        let output_data = state
            .outputs
            .iter()
            .map(|output| &output.info)
            .collect::<Vec<_>>();

        let res = serde_json::to_string(&output_data).map_err(|e| {
            log::error!("Failed to serialize output data: {e}");
            anyhow::anyhow!(e)
        });

        if let Ok(res) = res
            && let Err(e) = stream
                .write_all(format!("{res}\n").as_bytes())
                .and_then(|_| stream.flush())
        {
            log::error!("Stream write error: {e}");
            return Ok(calloop::PostAction::Continue);
        }

        let fd = stream.as_raw_fd();

        let source = unsafe {
            Generic::new(
                calloop::generic::FdWrapper::new(fd),
                calloop::Interest::READ,
                calloop::Mode::Level,
            )
        };

        if let Err(e) = state.handle.insert_source(source, move |_, _, state| {
            let wallpaper = match state.ipc.handle_stream_data(&fd) {
                Ok(data) => data,
                Err(e) => {
                    log::info!("{e}");
                    return Ok(calloop::PostAction::Remove);
                }
            };

            if wallpaper.outputs.is_empty() {
                let image = match wallpaper.data {
                    Data::Image(image) => FallbackImage::Image(assets::AssetData {
                        image,
                        resize: wallpaper.resize,
                        transition: wallpaper.transition,
                    }),
                    Data::Path(path) => {
                        if path.extension().is_some_and(|e| e == "svg") {
                            let svg_data = std::fs::read(path)?;

                            FallbackImage::Svg {
                                data: svg_data.into(),
                                transition: wallpaper.transition,
                            }
                        } else {
                            match image::open(path).map(ImageData::from) {
                                Ok(img) => FallbackImage::Image(assets::AssetData {
                                    image: img,
                                    resize: wallpaper.resize,
                                    transition: wallpaper.transition,
                                }),
                                Err(e) => {
                                    log::error!("Image open error: {e}");
                                    return Ok(calloop::PostAction::Continue);
                                }
                            }
                        }
                    }
                    Data::Color(color) => FallbackImage::Color {
                        color: image::Rgb(color),
                        transition: wallpaper.transition,
                    },
                    Data::S3 { alias, bucket, key } => {
                        let alias_name = alias.as_str();
                        let alias_config = match state.config.s3_aliases.get(alias_name) {
                            Some(config) => config,
                            None => {
                                log::warn!("Alias {} not found", alias_name);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };

                        let access_key = match alias_config.get_access_key() {
                            Ok(key) => key,
                            Err(e) => {
                                log::warn!("Failed to get access key for alias {}: {e}", alias_name);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };
                        let secret_key = match alias_config.get_secret_key() {
                            Ok(key) => key,
                            Err(e) => {
                                log::warn!("Failed to get secret key for alias {}: {e}", alias_name);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };

                        let endpoint = &alias_config.url;
                        let region = if let Some(region) = alias_config.region.as_ref() {
                            region
                        } else {
                            if endpoint.contains("localhost") || endpoint.contains("127.0.0.1") {
                                "garage"
                            } else {
                                log::warn!("No region specified for alias '{}' and could not auto-detect", alias_name);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };

                        let credentials = Credentials {
                            access_key: Some(access_key),
                            secret_key: Some(secret_key),
                            security_token: None,
                            session_token: None,
                            expiration: None,
                        };

                        let s3_region = Region::Custom {
                            region: region.to_string(),
                            endpoint: endpoint.clone(),
                        };

                        let mut bucket_obj = match Bucket::new(&bucket, s3_region, credentials) {
                            Ok(bucket) => bucket,
                            Err(e) => {
                                log::warn!("Failed to create S3 bucket '{}': {e}", bucket);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };
                        bucket_obj.set_path_style();

                        let res = match bucket_obj.get_object(&key) {
                            Ok(res) => res,
                            Err(e) => {
                                log::warn!("Failed to get S3 object '{}' from bucket '{}': {e}", key, bucket);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };

                        if res.status_code() != 200 {
                            log::warn!("Non 200 status code response for S3 object '{}' in bucket '{}': status {}", key, bucket, res.status_code());
                            return Ok(calloop::PostAction::Continue);
                        }

                        let bytes = res.bytes();

                        if bytes.len() < 1000 {
                            let content_str = String::from_utf8_lossy(&bytes);
                            if content_str.trim_start().starts_with("<?xml") {
                                log::warn!("S3 error response for object '{}' in bucket '{}': {}", key, bucket, content_str);
                                return Ok(calloop::PostAction::Continue);
                            }
                        }

                        let image_data = match image::load_from_memory(&bytes) {
                            Ok(data) => data,
                            Err(e) => {
                                log::warn!("Failed to load image from S3 object '{}' in bucket '{}': {e}", key, bucket);
                                return Ok(calloop::PostAction::Continue);
                            }
                        };

                        FallbackImage::Image(assets::AssetData {
                            image: ImageData::from(image_data),
                            resize: wallpaper.resize,
                            transition: wallpaper.transition,
                        })
                    }
                    Data::Http { .. } => todo!(),
                };

                state.assets.set_fallback(image);
            } else {
                wallpaper.outputs.iter().for_each(|output_name| {
                    let image = match &wallpaper.data {
                        Data::Image(image) => Some(image.clone()),
                        Data::Path(path) => {
                            if path.extension().is_some_and(|e| e == "svg") {
                                state
                                    .outputs
                                    .iter()
                                    .find(|output| &output.info.name == output_name)
                                    .and_then(|output| {
                                        render_svg(path, output.info.width, output.info.height).ok()
                                    })
                            } else {
                                image::open(path).map(ImageData::from).ok()
                            }
                        }
                        Data::Color(color) => state
                            .outputs
                            .iter()
                            .find(|output| &output.info.name == output_name)
                            .map(|output| {
                                let rgba_image = RgbaImage::from_pixel(
                                    output.info.width,
                                    output.info.height,
                                    image::Rgba([color[0], color[1], color[2], 255]),
                                );

                                ImageData::from(rgba_image)
                            }),
                        Data::S3 { alias, bucket, key } => {
                            load_s3_image(&Some(alias.clone()), bucket, key, &state.config.s3_aliases)
                        }
                        Data::Http { .. } => todo!(),
                    };

                    if let Some(image) = image {
                        state.assets.insert_asset(
                            Arc::clone(output_name),
                            assets::AssetData {
                                image,
                                resize: wallpaper.resize,
                                transition: wallpaper.transition.clone(),
                            },
                        );
                    }
                });
            }

            state.render();

            Ok(calloop::PostAction::Continue)
        }) {
            log::error!("Failed to insert source: {e}")
        }

        Ok(calloop::PostAction::Continue)
    })?;

    _ = display.get_registry(&moxpaper.qh, ());

    event_loop.run(None, &mut moxpaper, |_| {})?;
    drop(event_loop);

    Ok(())
}

fn render_svg<T>(path: T, width: u32, height: u32) -> anyhow::Result<ImageData>
where
    T: AsRef<Path>,
{
    let svg_data = std::fs::read(path.as_ref())?;

    let opt = usvg::Options {
        resources_dir: path.as_ref().parent().map(PathBuf::from),
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_data(&svg_data, &opt)?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height).context("Failed to create pixmap")?;

    let scale_x = width as f32 / tree.size().width();
    let scale_y = height as f32 / tree.size().height();

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );

    let image = image::load_from_memory(&pixmap.encode_png()?)?;

    Ok(ImageData::from(image))
}

fn load_s3_image(
    alias: &Option<String>,
    bucket: &str,
    key: &str,
    s3_aliases: &std::collections::HashMap<String, config::S3Alias>,
) -> Option<ImageData> {
}

impl Dispatch<wl_registry::WlRegistry, ()> for Moxpaper {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        version,
                        qh,
                        (),
                    ));
                }
                "zxdg_output_manager_v1" => {
                    state.output_manager = Some(
                        registry.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(
                            name,
                            version,
                            qh,
                            (),
                        ),
                    );
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(
                        registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                            name,
                            version,
                            qh,
                            (),
                        ),
                    );
                }
                "wl_output" => {
                    let wl_output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version, qh, ());
                    let surface = state
                        .compositor
                        .as_ref()
                        .unwrap()
                        .create_surface(&state.qh, ());

                    let layer_shell = match state.layer_shell.as_ref() {
                        Some(shell) => shell,
                        None => {
                            log::error!("wlr_layer_shell not initialized");
                            return;
                        }
                    };

                    let layer_surface = layer_shell.get_layer_surface(
                        &surface,
                        Some(&wl_output),
                        zwlr_layer_shell_v1::Layer::Background,
                        "moxpaper".into(),
                        &state.qh,
                        (),
                    );

                    layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::all());
                    layer_surface.set_exclusive_zone(-1);
                    let output = output::Output::new(
                        wl_output,
                        surface,
                        layer_surface,
                        state.handle.clone(),
                        name,
                    );

                    state.outputs.push(output);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                let index = state
                    .outputs
                    .iter()
                    .enumerate()
                    .find(|(_, output)| output.id == name)
                    .map(|(index, _)| index);

                if let Some(index) = index {
                    state.outputs.swap_remove(index);
                }
            }
            _ => unreachable!(),
        }
    }
}

delegate_noop!(Moxpaper: zxdg_output_manager_v1::ZxdgOutputManagerV1);
delegate_noop!(Moxpaper: zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(Moxpaper: wl_compositor::WlCompositor);
