mod animation;
mod assets;
mod config;
mod output;
pub mod texture_renderer;
pub mod utils;
mod wgpu_state;

use animation::bezier::BezierBuilder;
use anyhow::Context;
use assets::{AssetsManager, FallbackImage};
use calloop::{generic::Generic, EventLoop, LoopHandle};
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
use std::{
    io::Write,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_compositor, wl_output, wl_registry},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;
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
                    let bezier = match wallpaper.transition.bezier {
                        BezierChoice::Linear => BezierBuilder::new().linear(),
                        BezierChoice::Ease => BezierBuilder::new().ease(),
                        BezierChoice::EaseIn => BezierBuilder::new().ease_in(),
                        BezierChoice::EaseOut => BezierBuilder::new().ease_out(),
                        BezierChoice::EaseInOut => BezierBuilder::new().ease_in_out(),
                        BezierChoice::Custom(curve) => {
                            BezierBuilder::new().custom(curve.0, curve.1, curve.2, curve.3)
                        }
                        BezierChoice::Named(ref bezier) => {
                            if let Some(a) = self.config.bezier.get(bezier) {
                                BezierBuilder::new().custom(a.0, a.1, a.2, a.3)
                            } else {
                                log::warn!("Bezier: {bezier} not found");
                                BezierBuilder::new().linear()
                            }
                        }
                    };

                    output.target_image = Some(resized);
                    output
                        .animation
                        .start(&output.info.name, wallpaper.transition, bezier);
                }
            }
        });
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, action = clap::ArgAction::Count)]
    quiet: u8,

    #[arg(short, long, value_name = "FILE", help = "Path to the config file")]
    config: Option<Box<Path>>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut log_level = LevelFilter::Info;

    (0..cli.verbose).for_each(|_| {
        log_level = match log_level {
            LevelFilter::Error => LevelFilter::Warn,
            LevelFilter::Warn => LevelFilter::Info,
            LevelFilter::Info => LevelFilter::Debug,
            LevelFilter::Debug => LevelFilter::Trace,
            _ => log_level,
        };
    });

    (0..cli.quiet).for_each(|_| {
        log_level = match log_level {
            LevelFilter::Warn => LevelFilter::Error,
            LevelFilter::Info => LevelFilter::Warn,
            LevelFilter::Debug => LevelFilter::Info,
            LevelFilter::Trace => LevelFilter::Debug,
            _ => log_level,
        };
    });

    Builder::new().filter(Some("daemon"), log_level).init();

    let config = Config::load(cli.config);

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

        if let Ok(res) = res {
            if let Err(e) = stream
                .write_all(format!("{res}\n").as_bytes())
                .and_then(|_| stream.flush())
            {
                log::error!("Stream write error: {e}");
                return Ok(calloop::PostAction::Continue);
            }
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
                    registry.bind::<wl_output::WlOutput, u32, _>(name, version, qh, name);
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
