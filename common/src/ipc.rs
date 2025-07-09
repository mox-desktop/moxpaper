use crate::image_data::ImageData;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    io::Read,
    marker::PhantomData,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{Arc, LazyLock},
};

pub struct Client;
pub struct Server;

static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut path = PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    path.push("mox/.moxpaper.sock");

    path
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BezierChoice {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    Named(Box<str>),
    Custom((f32, f32, f32, f32)),
}

impl Default for BezierChoice {
    fn default() -> Self {
        BezierChoice::Custom((0.54, 0.0, 0.34, 0.99))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Transition {
    pub transition_type: Option<TransitionType>,
    pub fps: Option<u64>,
    pub duration: Option<u128>,
    pub bezier: Option<BezierChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransitionType {
    None,
    #[default]
    Simple,
    Fade,
    Left,
    Right,
    Top,
    Bottom,
    Center,
    Outer,
    Any,
    Random,
    Wipe,
    Wave,
    Grow,
    #[serde(untagged)]
    Custom(Arc<str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: Arc<str>,
    pub width: u32,
    pub height: u32,
    pub scale: i32,
}

impl Default for OutputInfo {
    fn default() -> Self {
        Self {
            name: "".into(),
            width: 0,
            height: 0,
            scale: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Data {
    Path(PathBuf),
    Image(ImageData),
    Color([u8; 3]),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, ValueEnum, Serialize, Deserialize)]
pub enum ResizeStrategy {
    /// Keep the original size, centering the image with optional background fill
    No,
    #[default]
    /// Expand and crop the image to fully cover the output
    Crop,
    /// Scale the image to fit within the output while preserving aspect ratio
    Fit,
    /// Stretch the image to completely fill the output, ignoring aspect ratio
    Stretch,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WallpaperData {
    pub outputs: Vec<Arc<str>>,
    pub data: Data,
    pub resize: ResizeStrategy,
    pub transition: Transition,
}

pub struct Ipc<T> {
    phantom: PhantomData<T>,
    inner: IpcInner,
}

struct ServerData {
    listener: UnixListener,
    connections: HashMap<i32, UnixStream>,
}

struct ClientData {
    stream: UnixStream,
}

enum IpcInner {
    Server(ServerData),
    Client(ClientData),
}

impl Ipc<Client> {
    pub fn connect() -> anyhow::Result<Self> {
        let stream = UnixStream::connect(&*PATH)?;

        Ok(Self {
            inner: IpcInner::Client(ClientData { stream }),
            phantom: PhantomData,
        })
    }

    fn get_inner(&self) -> &ClientData {
        let IpcInner::Client(client_data) = &self.inner else {
            unreachable!();
        };

        client_data
    }

    pub fn get_stream(&self) -> &UnixStream {
        &self.get_inner().stream
    }
}

impl Ipc<Server> {
    pub fn server() -> anyhow::Result<Self> {
        if let Ok(output) = std::process::Command::new("pidof").arg("moxpaper").output()
            && output.status.success()
        {
            let pids = String::from_utf8_lossy(&output.stdout);
            if pids.split_whitespace().count() > 1 {
                return Err(anyhow::anyhow!("moxpaper is already running"));
            }
        }

        if !PATH.exists() {
            std::fs::create_dir_all(
                PATH.parent()
                    .ok_or(anyhow::anyhow!("Parent of {:#?} not found", PATH))?,
            )?;
        } else {
            std::fs::remove_file(&*PATH)?;
        }

        let listener = UnixListener::bind(&*PATH)?;

        Ok(Self {
            inner: IpcInner::Server(ServerData {
                listener,
                connections: HashMap::new(),
            }),
            phantom: PhantomData,
        })
    }

    fn get_inner(&self) -> &ServerData {
        let IpcInner::Server(server_data) = &self.inner else {
            unreachable!();
        };

        server_data
    }

    fn get_inner_mut(&mut self) -> &mut ServerData {
        let IpcInner::Server(server_data) = &mut self.inner else {
            unreachable!();
        };

        server_data
    }

    pub fn accept_connection(&mut self) -> &UnixStream {
        let inner = self.get_inner_mut();

        let (stream, _) = inner
            .listener
            .accept()
            .expect("Failed to accept connection");
        let fd = stream.as_raw_fd();
        inner.connections.entry(fd).or_insert(stream)
    }

    pub fn remove_connection(&mut self, fd: &i32) {
        let inner = self.get_inner_mut();
        _ = inner.connections.remove(fd);
    }

    pub fn get_listener(&self) -> &UnixListener {
        let inner = self.get_inner();
        &inner.listener
    }

    pub fn get_mut(&mut self, fd: &i32) -> Option<&mut UnixStream> {
        let inner = self.get_inner_mut();
        inner.connections.get_mut(fd)
    }

    pub fn handle_stream_data(&mut self, fd: &i32) -> anyhow::Result<WallpaperData> {
        let mut buffer = Vec::new();

        if let Some(stream) = self.get_mut(fd) {
            match stream.read_to_end(&mut buffer) {
                Ok(0) => {
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!("Connection removed"))
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    Ok(serde_json::from_slice::<WallpaperData>(data)?)
                }
                Err(e) => {
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!(e))
                }
            }
        } else {
            Err(anyhow::anyhow!(""))
        }
    }
}
