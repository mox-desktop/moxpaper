mod output;
pub mod texture_renderer;
pub mod utils;
mod wgpu_state;

use calloop::{generic::Generic, EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use common::{
    image_data::ImageData,
    ipc::{Ipc, Server},
};
use image::{DynamicImage, RgbaImage};
use std::{io::Write, os::fd::AsRawFd};
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
}

impl Moxpaper {
    fn new(
        conn: &Connection,
        qh: QueueHandle<Self>,
        ipc: Ipc<Server>,
        handle: LoopHandle<'static, Self>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            handle,
            ipc,
            compositor: None,
            output_manager: None,
            layer_shell: None,
            outputs: Vec::new(),
            wgpu: WgpuState::new(conn)?,
            qh,
        })
    }

    fn render(&mut self) {
        self.outputs.iter_mut().for_each(|output| output.render());
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let conn = Connection::connect_to_env().expect("Connection to wayland failed");
    let display = conn.display();

    let event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let ipc = Ipc::server()?;

    let mut event_loop = EventLoop::try_new()?;
    let mut moxpaper = Moxpaper::new(&conn, qh, ipc, event_loop.handle())?;

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

        let output_data = state
            .outputs
            .iter()
            .map(|output| &output.info)
            .collect::<Vec<_>>();

        let res = serde_json::to_string(&output_data).unwrap();
        stream.write_all(format!("{res}\n").as_bytes()).unwrap();
        stream.flush().unwrap();

        let fd = stream.as_raw_fd();

        let source = unsafe {
            Generic::new(
                calloop::generic::FdWrapper::new(fd),
                calloop::Interest::READ,
                calloop::Mode::Level,
            )
        };

        state
            .handle
            .insert_source(source, move |_, _, state| {
                let data = match state.ipc.handle_stream_data(&fd) {
                    Ok(data) => data,
                    Err(e) => {
                        println!("{e}");
                        return Ok(calloop::PostAction::Remove);
                    }
                };

                state
                    .outputs
                    .iter_mut()
                    .filter(|output| {
                        data.outputs.contains(&output.info.name) || data.outputs.is_empty()
                    })
                    .for_each(|output| {
                        let size = format!("{}x{}", output.info.width, output.info.height);

                        output.frames = Some(
                            data.frames
                                .get(&size)
                                .unwrap()
                                .iter()
                                .cloned()
                                .filter_map(|frame| {
                                    let rgba_image = RgbaImage::from_raw(
                                        output.info.height as u32,
                                        output.info.width as u32,
                                        frame,
                                    )?;

                                    ImageData::try_from(DynamicImage::ImageRgba8(rgba_image)).ok()
                                })
                                .collect(),
                        );
                    });

                state.render();

                Ok(calloop::PostAction::Continue)
            })
            .unwrap();

        Ok(calloop::PostAction::Continue)
    })?;

    _ = display.get_registry(&moxpaper.qh, ());

    event_loop.run(None, &mut moxpaper, |_| {})?;

    Ok(())
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
