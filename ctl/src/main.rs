use anyhow::Context;
use clap::Parser;
use common::{
    cache::{self, CacheEntry},
    image_data::ImageData,
    ipc::{Data, Ipc, OutputInfo, ResizeStrategy, WallpaperData},
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

    chars.enumerate().try_for_each(|(i, c)| {
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

        Ok(())
    })?;
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

    /// Whether to resize the image and the method by which to resize it
    #[arg(long, default_value = "crop")]
    pub resize: ResizeStrategy,
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

fn main() -> anyhow::Result<()> {
    let ipc = Ipc::connect().context("Failed to connect to IPC")?;
    let mut ipc_stream = ipc.get_stream();

    let mut buf = String::new();
    let mut ipc_reader = std::io::BufReader::new(&mut ipc_stream);
    ipc_reader.read_line(&mut buf)?;

    let outputs: Vec<OutputInfo> = serde_json::from_str(&buf)?;
    let cli = Cli::parse();

    match cli {
        Cli::Img(img) => {
            let (data, cache_entry) = match img.image {
                CliImage::Path(path) => {
                    if path.to_str() == Some("-") {
                        let mut img_buf = Vec::new();
                        std::io::stdin().read_to_end(&mut img_buf)?;
                        let image = ImageReader::new(std::io::Cursor::new(&img_buf))
                            .with_guessed_format()?
                            .decode()?;

                        let image_data = ImageData::from(image);

                        (
                            Data::Image(image_data.clone()),
                            CacheEntry::Image {
                                image: image_data,
                                resize: img.resize,
                            },
                        )
                    } else {
                        (
                            Data::Path(path.clone()),
                            CacheEntry::Path {
                                path: path.clone().into(),
                                resize: img.resize,
                            },
                        )
                    }
                }
                CliImage::Color(color) => (Data::Color(color), CacheEntry::Color(color)),
            };

            let target_outputs = Arc::new(HashSet::from_iter(
                img.outputs.iter().map(|output| output.as_str().into()),
            ));

            let wallpaper_data = WallpaperData {
                outputs: target_outputs.clone(),
                resize: img.resize,
                data,
            };
            ipc_stream.write_all(serde_json::to_string(&wallpaper_data)?.as_bytes())?;

            if target_outputs.is_empty() {
                outputs.iter().for_each(|output| {
                    let result = cache::store(&output.name, cache_entry.clone());

                    if let Err(e) = result {
                        log::error!("Failed to store output {}: {e}", output.name);
                    }
                });
            } else {
                target_outputs.iter().for_each(|output| {
                    let result = cache::store(output, cache_entry.clone());

                    if let Err(e) = result {
                        log::error!("Failed to store output {output}: {e}");
                    }
                });
            }
        }
        Cli::Clear(clear) => {
            let target_outputs = Arc::new(HashSet::from_iter(
                clear.outputs.iter().map(|output| output.as_str().into()),
            ));

            let wallpaper_data = WallpaperData {
                outputs: target_outputs.clone(),
                data: Data::Color(clear.color),
                resize: ResizeStrategy::No,
            };
            ipc_stream.write_all(serde_json::to_string(&wallpaper_data)?.as_bytes())?;

            if target_outputs.is_empty() {
                outputs.iter().for_each(|output| {
                    if let Err(e) = cache::store(&output.name, CacheEntry::Color(clear.color)) {
                        log::error!("Failed to store output {}: {e}", output.name);
                    }
                });
            } else {
                target_outputs.iter().for_each(|output| {
                    if let Err(e) = cache::store(output, CacheEntry::Color(clear.color)) {
                        log::error!("Failed to store output {output}: {e}");
                    }
                });
            }
        }
    }

    Ok(())
}
