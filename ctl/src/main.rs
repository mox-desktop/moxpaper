use anyhow::{Context, Result};
use clap::Parser;
use common::{
    cache,
    image_data::ImageData,
    ipc::{Data, Ipc, OutputInfo, WallpaperData},
};
use image::ImageReader;
use std::{
    collections::HashSet,
    io::{BufRead, Read, Write},
    path::PathBuf,
    sync::Arc,
};

fn from_hex(hex: &str) -> Result<[u8; 3], String> {
    let hex = hex.trim_start_matches('#');

    let chars = hex
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase() as u8);

    if chars.clone().count() != 6 {
        return Err(format!(
            "Expected 6 characters for hex color, found {}",
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
                    "Expected [0-9], [a-f], or [A-F], found '{}'",
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

/// Command to clear the display with a specific color
#[derive(Parser, Debug)]
pub struct Clear {
    /// Hex color to use for clearing (format: RRGGBB)
    #[arg(value_parser = from_hex, default_value = "000000")]
    pub color: [u8; 3],

    /// Comma-separated list of output names to clear
    #[arg(short, long, value_delimiter = ',')]
    pub outputs: Vec<String>,
}

/// All available commands for this application
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Cli {
    /// Display an image on specified outputs
    Img(Img),

    /// Clear specified outputs with a color
    Clear(Clear),
}

/// Command to display an image on outputs
#[derive(Parser, Debug)]
pub struct Img {
    /// Path to the image or '-' for stdin
    #[arg(value_parser = parse_image)]
    pub image: CliImage,

    /// Comma-separated list of output names to display on
    #[arg(short, long, value_delimiter = ',')]
    pub outputs: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum CliImage {
    Path(PathBuf),
    Color([u8; 3]),
}

pub fn parse_image(raw: &str) -> Result<CliImage, String> {
    let path = PathBuf::from(raw);
    if raw == "-" || path.exists() {
        return Ok(CliImage::Path(path));
    }
    if let Some(color) = raw.strip_prefix("0x") {
        if let Ok(color) = from_hex(color) {
            return Ok(CliImage::Color(color));
        }
    }
    Err(format!("Path '{raw}' does not exist"))
}

fn main() -> Result<()> {
    let ipc = Ipc::connect().context("Failed to connect to IPC")?;
    let mut stream = ipc.get_stream();

    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(&mut stream);
    reader.read_line(&mut buf)?;

    let cli = Cli::parse();

    match cli {
        Cli::Img(img) => match img.image {
            CliImage::Path(path) => {
                let data = if path.to_str() == Some("-") {
                    let mut buf = Vec::new();
                    std::io::stdin().read_to_end(&mut buf)?;

                    let img = ImageReader::new(std::io::Cursor::new(&buf))
                        .with_guessed_format()?
                        .decode()?;

                    Data::Image(ImageData::from(img))
                } else {
                    Data::Path(path.clone())
                };

                let data = WallpaperData {
                    outputs: Arc::new(HashSet::from_iter(
                        img.outputs.iter().map(|output| output.as_str().into()),
                    )),
                    data,
                };

                stream.write_all(serde_json::to_string(&data)?.as_bytes())?;

                let outputs: Vec<OutputInfo> = serde_json::from_str(&buf)?;

                if data.outputs.is_empty() {
                    outputs.iter().for_each(|output| {
                        if let Err(e) = cache::store(&output.name, path.as_path()) {
                            log::error!("Failed to store output {}: {e}", output.name);
                        }
                    });
                } else {
                    data.outputs.iter().for_each(|output| {
                        if let Err(e) = cache::store(output, path.as_path()) {
                            log::error!("Failed to store output {output}: {e}");
                        }
                    });
                }
            }
            CliImage::Color(_) => {
                todo!()
            }
        },
        Cli::Clear(_) => {
            todo!()
        }
    }

    Ok(())
}
