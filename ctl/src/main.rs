use anyhow::Context;
use clap::Parser;
use common::{
    image_data::ImageData,
    ipc::{
        BezierChoice, Data, Ipc, OutputInfo, ResizeStrategy, Transition, TransitionType,
        WallpaperData,
    },
};
use image::ImageReader;
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
};

fn from_hex(hex: &str) -> anyhow::Result<[u8; 3]> {
    let hex = hex.trim_start_matches('#');

    let chars = hex
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase() as u8);

    if chars.clone().count() != 6 {
        return Err(anyhow::anyhow!(
            "Expected 6 characters for hex color, found {}",
            chars.count()
        ));
    }

    let mut color = [0, 0, 0];

    chars.enumerate().try_for_each(|(i, c)| {
        match c {
            b'A'..=b'F' => color[i / 2] += c - b'A' + 10,
            b'0'..=b'9' => color[i / 2] += c - b'0',
            _ => {
                return Err(anyhow::anyhow!(
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

/// Clears specified outputs by filling them with a solid color
#[derive(Parser, Debug)]
pub struct Clear {
    /// Color in hexadecimal (format: RRGGBB) used to fill the display
    #[arg(value_parser = from_hex, default_value = "000000")]
    pub color: [u8; 3],

    /// List of output names to target, separated by commas
    #[arg(short, long, value_delimiter = ',')]
    pub outputs: Vec<String>,

    /// Type of transition when clearing
    #[arg(long, value_parser = parse_transition_type, default_value = "simple")]
    pub transition_type: TransitionType,

    /// How long transition takes to complete in milliseconds
    #[arg(long, default_value = "3000")]
    pub transition_duration: u128,

    /// Frame rate for the transition effect. Defaults to display's vsync.
    #[arg(long)]
    pub transition_fps: Option<u64>,

    /// Bezier timing, e.g. “ease” or “0.42,0.0,1.0,1.0”
    #[arg(long, value_parser = parse_bezier, default_value = "0.54,0,0.32,0.99")]
    pub bezier: BezierChoice,
}

/// Set of all commands supported by the application
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Cli {
    /// Show an image on one or more outputs
    Img(Img),

    /// Fill selected outputs with a specific color
    Clear(Clear),

    /// Retrieve current output information
    Query,
}

/// Command to show an image across selected outputs
#[derive(Parser, Debug)]
pub struct Img {
    /// File path to the image, or '-' to read from standard input
    #[arg(value_parser = parse_image)]
    pub image: CliImage,

    /// Names of outputs to display the image on, separated by commas
    #[arg(short, long, value_delimiter = ',')]
    pub outputs: Vec<String>,

    /// Strategy for scaling the image to fit outputs
    #[arg(long, default_value = "crop")]
    pub resize: ResizeStrategy,

    /// Type of transition
    #[arg(long, value_parser = parse_transition_type, default_value = "simple")]
    pub transition_type: TransitionType,

    /// How long transition takes to complete in miliseconds
    #[arg(long, default_value = "3000")]
    pub transition_duration: u128,

    /// Frame rate for the transition effect. Defaults to display's vsync.
    #[arg(long)]
    pub transition_fps: Option<u64>,

    /// Bezier timing, e.g. “ease” or “0.42,0.0,1.0,1.0”
    #[arg(long, value_parser = parse_bezier, default_value = "0.54,0,0.32,0.99")]
    pub transition_bezier: BezierChoice,
}

fn parse_bezier(s: &str) -> anyhow::Result<BezierChoice> {
    let nums = s
        .split(',')
        .map(str::trim)
        .map(str::parse)
        .collect::<Result<Vec<f32>, _>>();

    if let Ok(nums) = nums {
        if nums.len() == 4 {
            return Ok(BezierChoice::Custom((nums[0], nums[1], nums[2], nums[3])));
        }
    }

    let bezier = match s {
        "linear" => BezierChoice::Linear,
        "ease" => BezierChoice::Ease,
        "ease-in" => BezierChoice::EaseIn,
        "ease-out" => BezierChoice::EaseOut,
        "ease-in-out" => BezierChoice::EaseInOut,
        _ => BezierChoice::Named(s.into()),
    };

    Ok(bezier)
}

fn parse_transition_type(s: &str) -> anyhow::Result<TransitionType> {
    Ok(match s {
        "none" => TransitionType::None,
        "simple" => TransitionType::Simple,
        "fade" => TransitionType::Fade,
        "left" => TransitionType::Left,
        "right" => TransitionType::Right,
        "top" => TransitionType::Top,
        "bottom" => TransitionType::Bottom,
        "center" => TransitionType::Center,
        "outer" => TransitionType::Outer,
        "any" => TransitionType::Any,
        "random" => TransitionType::Random,
        "wipe" => TransitionType::Wipe,
        "wave" => TransitionType::Wave,
        "grow" => TransitionType::Grow,
        s => TransitionType::Custom(s.into()),
    })
}

#[derive(Clone, Debug)]
pub enum CliImage {
    Path(PathBuf),
    Color([u8; 3]),
}

pub fn parse_image(raw: &str) -> anyhow::Result<CliImage> {
    let path = PathBuf::from(raw);
    if raw == "-" || path.exists() {
        return Ok(CliImage::Path(path));
    }
    Err(anyhow::anyhow!("Path '{raw}' does not exist"))
}

fn main() -> anyhow::Result<()> {
    let ipc = Ipc::connect().context("Failed to connect to IPC")?;
    let mut ipc_stream = ipc.get_stream();

    let mut buf = String::new();
    let mut ipc_reader = std::io::BufReader::new(&mut ipc_stream);
    ipc_reader.read_line(&mut buf)?;

    let outputs: Vec<OutputInfo> = serde_json::from_str(&buf)?;

    match Cli::parse() {
        Cli::Img(img) => {
            let data = match img.image {
                CliImage::Path(path) => {
                    if path.to_str() == Some("-") {
                        let mut img_buf = Vec::new();
                        std::io::stdin().read_to_end(&mut img_buf)?;
                        let image = ImageReader::new(std::io::Cursor::new(&img_buf))
                            .with_guessed_format()?
                            .decode()?;

                        let image_data = ImageData::from(image);

                        Data::Image(image_data)
                    } else {
                        Data::Path(path)
                    }
                }
                CliImage::Color(color) => Data::Color(color),
            };

            let target_outputs = img
                .outputs
                .iter()
                .map(|output| output.as_str().into())
                .collect();

            let wallpaper_data = WallpaperData {
                outputs: target_outputs,
                resize: img.resize,
                transition: Transition {
                    transition_type: img.transition_type,
                    fps: img.transition_fps,
                    duration: img.transition_duration,
                    bezier: img.transition_bezier,
                },
                data,
            };
            ipc_stream.write_all(serde_json::to_string(&wallpaper_data)?.as_bytes())?;
        }
        Cli::Clear(clear) => {
            let target_outputs = clear
                .outputs
                .iter()
                .map(|output| output.as_str().into())
                .collect();

            let wallpaper_data = WallpaperData {
                outputs: target_outputs,
                data: Data::Color(clear.color),
                resize: ResizeStrategy::No,
                transition: Transition {
                    transition_type: clear.transition_type,
                    fps: clear.transition_fps,
                    duration: clear.transition_duration,
                    bezier: clear.bezier,
                },
            };
            ipc_stream.write_all(serde_json::to_string(&wallpaper_data)?.as_bytes())?;
        }
        Cli::Query => {
            outputs.iter().for_each(|output| {
                _ = writeln!(
                    std::io::stdout(),
                    "{}: {}x{}, scale: {}",
                    output.name,
                    output.width,
                    output.height,
                    output.scale
                );
            });
        }
    }

    Ok(())
}
