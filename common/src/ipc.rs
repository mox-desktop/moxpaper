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
    sync::LazyLock,
};

use crate::image_data::ImageData;

pub struct Client;
pub struct Server;

static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut path = PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    path.push("mox/.moxpaper.sock");

    path
});

#[derive(Debug, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
}

impl Default for OutputInfo {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            width: 0,
            height: 0,
            scale: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Data {
    pub outputs: Vec<String>,
    pub frames: HashMap<String, Vec<ImageData>>,
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

    pub fn handle_stream_data(&mut self, fd: &i32) -> anyhow::Result<Data> {
        let mut buffer = Vec::new();

        if let Some(stream) = self.get_mut(fd) {
            match stream.read_to_end(&mut buffer) {
                Ok(0) => {
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!("Connection removed"))
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    Ok(serde_json::from_slice::<Data>(data)?)
                }
                Err(e) => {
                    eprintln!("Read error: {e}");
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!(e))
                }
            }
        } else {
            Err(anyhow::anyhow!(""))
        }
    }
}
