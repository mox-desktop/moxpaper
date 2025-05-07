mod output;
pub mod texture_renderer;
pub mod utils;
mod wgpu_state;

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
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
}

impl Moxpaper {
    fn new(conn: &Connection, qh: QueueHandle<Self>) -> anyhow::Result<Self> {
        Ok(Self {
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

    let mut event_loop = EventLoop::try_new()?;
    let mut moxpaper = Moxpaper::new(&conn, qh)?;

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Failed to insert Wayland source: {}", e))?;

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
