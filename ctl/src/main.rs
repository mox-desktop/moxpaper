use anyhow::{Context, Result};
use clap::Parser;
use common::{
    image_data::ImageData,
    ipc::{Data, Ipc, OutputInfo},
};
use resvg::usvg;
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
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
    #[clap(short, long, default_value = "")]
    pub outputs: String,
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
    Err(format!("Path '{raw}' does not exist"))
}

fn render_svg(path: &PathBuf, width: i32, height: i32) -> Result<Vec<u8>> {
    let svg_data =
        std::fs::read(path).context(format!("Failed to read SVG file: {}", path.display()))?;

    let opt = usvg::Options {
        resources_dir: Some(path.clone()),
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_data(&svg_data, &opt).context("Failed to parse SVG data")?;

    let mut pixmap =
        tiny_skia::Pixmap::new(width as u32, height as u32).context("Failed to create pixmap")?;

    let scale_x = width as f32 / tree.size().width();
    let scale_y = height as f32 / tree.size().height();

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );

    pixmap.encode_png().context("Failed to encode PNG")
}

fn process_image(path: &PathBuf, width: i32, height: i32) -> Result<ImageData> {
    if path.extension().is_some_and(|ext| ext == "svg") {
        let png_data = render_svg(path, width, height)?;
        let image = image::load_from_memory(&png_data).context("Failed to load rendered SVG")?;

        Ok(ImageData::from(image).resize_to_fit(width as u32, height as u32))
    } else {
        let image =
            image::open(path).context(format!("Failed to open image: {}", path.display()))?;

        Ok(ImageData::from(image).resize_to_fit(width as u32, height as u32))
    }
}

fn handle_img_command(img: Img) -> Result<()> {
    let ipc = Ipc::connect().context("Failed to connect to IPC")?;
    let mut stream = ipc.get_stream();

    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(&mut stream);
    reader
        .read_line(&mut buf)
        .context("Failed to read from IPC stream")?;

    let outputs = serde_json::from_str::<Vec<OutputInfo>>(&buf)
        .context("Failed to parse output information")?;

    match img.image {
        CliImage::Path(path) => {
            let mut frames = HashMap::new();

            let outputs: Vec<&OutputInfo> = outputs
                .iter()
                .filter(|output| img.outputs.contains(&output.name) || img.outputs.is_empty())
                .collect();

            outputs.iter().for_each(|output| {
                let size = format!("{}x{}", output.width, output.height);
                frames.entry(size).or_insert_with(|| {
                    let image_data = process_image(&path, output.width, output.height).unwrap();
                    vec![image_data]
                });
            });

            let data = Data {
                outputs: img.outputs,
                frames,
            };

            let serialized =
                serde_json::to_string(&data).context("Failed to serialize image data")?;

            stream
                .write_all(serialized.as_bytes())
                .context("Failed to write to IPC stream")?;

            println!("Image data sent successfully!");
        }
        CliImage::Color(_) => {
            todo!()
        }
    }

    Ok(())
}

fn handle_clear_command(clear: Clear) -> Result<()> {
    println!(
        "Clear command with color {:?} for outputs '{}'",
        clear.color, clear.outputs
    );
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli {
        Cli::Img(img) => handle_img_command(img)?,
        Cli::Clear(clear) => handle_clear_command(clear)?,
    }

    Ok(())
}
