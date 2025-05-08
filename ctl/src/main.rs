use clap::Parser;
use common::ipc::Ipc;
use serde::Serialize;
use std::{env, io::Write, path::PathBuf};
#[derive(Debug, Serialize)]
struct Data {
    outputs: Vec<String>,
    frames: Vec<Vec<u8>>,
}
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    path: PathBuf,
}
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut socket_path =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    socket_path.push("mox/.moxpaper.sock");

    println!("Opening image from: {:?}", cli.path);

    let image = image::open(&cli.path).unwrap();
    let rgba_image = image.to_rgba8();
    let raw_pixels: Vec<u8> = rgba_image.into_raw();

    let data = Data {
        outputs: vec![],
        frames: vec![raw_pixels],
    };

    let ipc = Ipc::connect()?;

    let serialized = serde_json::to_string(&data).unwrap();
    ipc.get_stream().write_all(serialized.as_bytes()).unwrap();

    println!("Image data sent successfully!");

    Ok(())
}
