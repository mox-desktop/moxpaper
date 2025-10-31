use anyhow::Context;
use clap::Parser;
use common::image_data::ImageData;
use common::ipc::{BezierChoice, ResizeStrategy, TransitionType};
use image::ImageReader;
use libmoxpaper::MoxpaperClient;
use std::{
    env, fs,
    io::{self, BufRead, Read},
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
    #[arg(long, value_parser = parse_transition_type)]
    pub transition_type: Option<TransitionType>,

    /// How long transition takes to complete in milliseconds
    #[arg(long)]
    pub transition_duration: Option<u128>,

    /// Frame rate for the transition effect. Defaults to display's vsync.
    #[arg(long)]
    pub transition_fps: Option<u64>,

    /// Bezier timing, e.g. "ease" or "0.42,0.0,1.0,1.0"
    #[arg(long, value_parser = parse_bezier)]
    pub bezier: Option<BezierChoice>,
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
    #[arg(long, value_parser = parse_transition_type)]
    pub transition_type: Option<TransitionType>,

    /// How long transition takes to complete in miliseconds
    #[arg(long)]
    pub transition_duration: Option<u128>,

    /// Frame rate for the transition effect. Defaults to display's vsync.
    #[arg(long)]
    pub transition_fps: Option<u64>,

    /// Bezier timing, e.g. "ease" or "0.42,0.0,1.0,1.0"
    #[arg(long, value_parser = parse_bezier)]
    pub transition_bezier: Option<BezierChoice>,

    /// S3 Access Key ID.
    #[arg(long, env = "MOXPAPER_S3_ACCESS_KEY_ID")]
    pub s3_access_key_id: Option<String>,

    /// Secret Access Key.
    #[arg(long, env = "MOXPAPER_S3_SECRET_ACCESS_KEY")]
    pub s3_secret_access_key: Option<String>,

    /// S3 Region.
    #[arg(long, env = "MOXPAPER_S3_REGION")]
    pub s3_region: Option<String>,
}

fn parse_bezier(s: &str) -> anyhow::Result<BezierChoice> {
    let nums = s
        .split(',')
        .map(str::trim)
        .map(str::parse)
        .collect::<Result<Vec<f32>, _>>();

    if let Ok(nums) = nums
        && nums.len() == 4
    {
        return Ok(BezierChoice::Custom((nums[0], nums[1], nums[2], nums[3])));
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
    Http(String),
    S3(String),
}

fn parse_s3_url(url: &str) -> Option<(String, String)> {
    if let Some(stripped) = url.strip_prefix("s3://") {
        if let Some(slash_idx) = stripped.find('/') {
            let bucket = stripped[..slash_idx].to_string();
            let key = stripped[slash_idx + 1..].to_string();
            return Some((bucket, key));
        }
    }
    None
}

pub fn parse_image(raw: &str) -> anyhow::Result<CliImage> {
    if raw.starts_with("s3://") {
        if parse_s3_url(raw).is_some() {
            return Ok(CliImage::S3(raw.to_string()));
        }
        return Err(anyhow::anyhow!("Invalid S3 URL format: {}", raw));
    }

    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Ok(CliImage::Http(raw.to_string()));
    }

    if raw == "-" {
        return Ok(CliImage::Path(PathBuf::from("-")));
    }

    let path = PathBuf::from(raw);
    if path.exists() {
        return Ok(CliImage::Path(path));
    }

    Err(anyhow::anyhow!(
        "Path '{}' does not exist and is not a valid URL",
        raw
    ))
}

#[derive(Debug, Clone)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl S3Credentials {
    pub fn fetch(
        cli_access_key: Option<String>,
        cli_secret_key: Option<String>,
    ) -> anyhow::Result<Self> {
        if let (Some(access_key), Some(secret_key)) = (cli_access_key, cli_secret_key) {
            return Ok(Self {
                access_key_id: access_key,
                secret_access_key: secret_key,
            });
        }

        if let (Ok(access_key), Ok(secret_key)) = (
            env::var("MOXPAPER_S3_ACCESS_KEY_ID"),
            env::var("MOXPAPER_S3_SECRET_ACCESS_KEY"),
        ) {
            return Ok(Self {
                access_key_id: access_key,
                secret_access_key: secret_key,
            });
        }

        let file_access_key = env::var("MOXPAPER_S3_ACCESS_KEY_ID_FILE")
            .ok()
            .and_then(|path| fs::read_to_string(&path).ok());
        let file_secret_key = env::var("MOXPAPER_S3_SECRET_ACCESS_KEY_FILE")
            .ok()
            .and_then(|path| fs::read_to_string(&path).ok());

        if let (Some(access_key), Some(secret_key)) = (file_access_key, file_secret_key) {
            return Ok(Self {
                access_key_id: access_key,
                secret_access_key: secret_key,
            });
        }

        Err(anyhow::anyhow!("S3 credentials not found."))
    }
}

fn main() -> anyhow::Result<()> {
    let mut client = MoxpaperClient::connect().context("Failed to connect to daemon")?;

    match Cli::parse() {
        Cli::Img(img) => {
            let transition = MoxpaperClient::transition(
                img.transition_type,
                img.transition_fps,
                img.transition_duration,
                img.transition_bezier,
            );

            let mut builder = client.set().resize(img.resize).transition(transition);

            if !img.outputs.is_empty() {
                builder = builder.outputs(img.outputs);
            }

            match img.image {
                CliImage::Path(path) => {
                    if path.to_str() == Some("-") {
                        let mut img_buf = Vec::new();
                        std::io::stdin().read_to_end(&mut img_buf)?;
                        let image = ImageReader::new(std::io::Cursor::new(&img_buf))
                            .with_guessed_format()?
                            .decode()?;

                        let image_data = ImageData::from(image);
                        builder.image(image_data).apply()?;
                    } else {
                        builder.path(path).apply()?;
                    }
                }
                CliImage::Color(color) => {
                    builder.color(color).apply()?;
                }
                CliImage::Http(url) => {
                    builder.http_data(url, None).apply()?;
                }
                CliImage::S3(url) => {
                    let (bucket, key) = parse_s3_url(&url)
                        .ok_or_else(|| anyhow::anyhow!("Invalid S3 URL format: {}", url))?;

                    let creds = S3Credentials::fetch(
                        img.s3_access_key_id.clone(),
                        img.s3_secret_access_key.clone(),
                    )?;

                    builder
                        .s3_url(
                            url,
                            creds.access_key_id,
                            creds.secret_access_key,
                            img.s3_region.clone(),
                        )
                        .apply()?;
                }
            }
        }
        Cli::Clear(clear) => {
            let transition = MoxpaperClient::transition(
                clear.transition_type,
                clear.transition_fps,
                clear.transition_duration,
                clear.bezier,
            );
            let mut builder = client.set().transition(transition);
            if !clear.outputs.is_empty() {
                builder = builder.outputs(clear.outputs);
            }
            builder.color(clear.color).apply()?;
        }
        Cli::Query => {
            for output in client.outputs() {
                println!(
                    "{}: {}x{}, scale: {}",
                    output.name, output.width, output.height, output.scale
                );
            }
        }
    }

    Ok(())
}
