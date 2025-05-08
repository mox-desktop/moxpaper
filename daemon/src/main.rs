mod output;
pub mod texture_renderer;
pub mod utils;
mod wgpu_state;

use calloop::{generic::Generic, EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::PathBuf,
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_compositor, wl_output, wl_registry},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;
use wgpu_state::WgpuState;

#[derive(Debug, Deserialize)]
struct Data {
    outputs: Vec<String>,
    frames: Vec<Vec<u8>>,
}

struct Ipc {
    listener: UnixListener,
    connections: HashMap<i32, UnixStream>,
}

struct Moxpaper {
    output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    compositor: Option<wl_compositor::WlCompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    outputs: Vec<output::Output>,
    wgpu: wgpu_state::WgpuState,
    qh: QueueHandle<Moxpaper>,
    ipc: Ipc,
    handle: LoopHandle<'static, Self>,
}

impl Moxpaper {
    fn new(
        conn: &Connection,
        qh: QueueHandle<Self>,
        listener: UnixListener,
        handle: LoopHandle<'static, Self>,
    ) -> anyhow::Result<Self> {
        let ipc = Ipc {
            listener,
            connections: HashMap::new(),
        };

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

    let mut path = PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    path.push("mox/.moxpaper.sock");

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
    } else {
        std::fs::remove_file(&path)?;
    }

    let listener = UnixListener::bind(&path)?;

    let mut event_loop = EventLoop::try_new()?;
    let mut moxpaper = Moxpaper::new(&conn, qh, listener, event_loop.handle())?;

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Failed to insert Wayland source: {}", e))?;

    let source = unsafe {
        Generic::new(
            calloop::generic::FdWrapper::new(moxpaper.ipc.listener.as_raw_fd()),
            calloop::Interest {
                readable: true,
                writable: false,
            },
            calloop::Mode::Level,
        )
    };

    event_loop.handle().insert_source(source, |_, _, state| {
        let (stream, _) = state.ipc.listener.accept().unwrap();
        let fd = stream.as_raw_fd();
        state.ipc.connections.insert(fd, stream);

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
                let fd = fd;
                let mut buffer = Vec::new();
                if let Some(stream) = state.ipc.connections.get_mut(&fd) {
                    match stream.read_to_end(&mut buffer) {
                        Ok(0) => {
                            state.ipc.connections.remove(&fd);
                            return Ok(calloop::PostAction::Remove);
                        }
                        Ok(n) => {
                            let data = &buffer[..n];
                            // Parse directly from bytes instead of converting to UTF-8 string
                            match serde_json::from_slice::<Data>(data) {
                                Ok(parsed_data) => {
                                    println!(
                                        "Received message with {} frames",
                                        parsed_data.frames.len()
                                    );
                                    // Process your data here
                                }
                                Err(e) => {
                                    eprintln!("JSON parsing error: {}", e);
                                    // Handle error gracefully
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Read error: {e}");
                            state.ipc.connections.remove(&fd);
                            return Ok(calloop::PostAction::Remove);
                        }
                    }
                } else {
                    return Ok(calloop::PostAction::Remove);
                }
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
                    .find(|(_, output)| output.info.id == name)
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
