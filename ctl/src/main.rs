use clap::Parser;
use common::{image_data::ImageData, ipc::Ipc};
use serde::Serialize;
use std::{env, io::Write, path::PathBuf};

#[derive(Debug, Serialize)]
struct Data<'a> {
    outputs: Vec<String>,
    frames: Vec<&'a [u8]>,
}

fn from_hex(hex: &str) -> Result<[u8; 3], String> {
    let chars = hex
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase() as u8);

    if chars.clone().count() != 6 {
        return Err(format!(
            "expected 6 characters, found {}",
            chars.clone().count()
        ));
    }

    let mut color = [0, 0, 0];

    for (i, c) in chars.enumerate() {
        match c {
            b'A'..=b'F' => color[i / 2] += c - b'A' + 10,
            b'0'..=b'9' => color[i / 2] += c - b'0',
            _ => {
                return Err(format!(
                    "expected [0-9], [a-f], or [A-F], found '{}'",
                    char::from(c)
                ));
            }
        }
        if i % 2 == 0 {
            color[i / 2] *= 16;
        }
    }
    Ok(color)
}

#[derive(Parser)]
pub struct Clear {
    #[arg(value_parser = from_hex, default_value = "000000")]
    pub color: [u8; 3],

    #[clap(short, long, default_value = "")]
    pub outputs: String,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Cli {
    Img(Img),
    Clear(Clear),
}

#[derive(Parser)]
pub struct Img {
    #[arg(value_parser = parse_image)]
    pub image: CliImage,

    #[arg(short, long, default_value = "")]
    pub outputs: String,
}

#[derive(Clone)]
pub enum CliImage {
    Path(PathBuf),
    Color([u8; 3]),
}

pub fn parse_image(raw: &str) -> Result<CliImage, String> {
    let path = PathBuf::from(raw);
    if raw == "-" || path.exists() {
        return Ok(CliImage::Path(path));
    }
    Err(format!("Path '{}' does not exist", raw))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut socket_path =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    socket_path.push("mox/.moxpaper.sock");

    let Cli::Img(img) = cli else {
        return Ok(());
    };

    let ipc = Ipc::connect()?;

    let CliImage::Path(path) = img.image else {
        return Ok(());
    };

    let image = image::open(&path).unwrap();
    let image_data = ImageData::try_from(image)?.to_rgba().resize(1920, 1080);

    let data = Data {
        outputs: vec![],
        frames: vec![image_data.data()],
    };

    let serialized = serde_json::to_string(&data).unwrap();
    ipc.get_stream().write_all(serialized.as_bytes()).unwrap();

    println!("Image data sent successfully!");

    Ok(())
}
