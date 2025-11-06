#[cfg(any(feature = "server", feature = "client"))]
pub mod image_data;

#[cfg(feature = "server")]
pub mod ipc;

#[cfg(all(feature = "client", not(feature = "server")))]
mod ipc;

// Re-export ImageData for public API when features are enabled
#[cfg(any(feature = "server", feature = "client"))]
pub use image_data::ImageData;

use anyhow::Context;
#[cfg(any(feature = "server", feature = "client"))]
use ipc::Ipc;
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::Arc,
};

// Data types moved from ipc module
#[cfg(any(feature = "server", feature = "client"))]
use clap::ValueEnum;
#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};

#[cfg(any(feature = "server", feature = "client"))]
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

#[cfg(any(feature = "server", feature = "client"))]
impl Default for BezierChoice {
    fn default() -> Self {
        BezierChoice::Custom((0.54, 0.0, 0.34, 0.99))
    }
}

#[cfg(any(feature = "server", feature = "client"))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Transition {
    pub transition_type: Option<TransitionType>,
    pub fps: Option<u64>,
    pub duration: Option<u128>,
    pub bezier: Option<BezierChoice>,
}

#[cfg(any(feature = "server", feature = "client"))]
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

#[cfg(any(feature = "server", feature = "client"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: Arc<str>,
    pub width: u32,
    pub height: u32,
    pub scale: i32,
}

#[cfg(any(feature = "server", feature = "client"))]
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

#[cfg(any(feature = "server", feature = "client"))]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Data {
    Path(PathBuf),
    Image(ImageData),
    Color([u8; 3]),
    S3 {
        bucket: String,
        key: String,
    },
    Http {
        url: String,
        headers: Option<Vec<(String, String)>>,
    },
}

#[cfg(any(feature = "server", feature = "client"))]
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

#[cfg(any(feature = "server", feature = "client"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct WallpaperData {
    pub outputs: Vec<Arc<str>>,
    pub data: Data,
    pub resize: ResizeStrategy,
    pub transition: Transition,
}

fn parse_s3_url(url: &str) -> anyhow::Result<(String, String)> {
    if let Some(stripped) = url.strip_prefix("s3://") {
        let parts: Vec<&str> = stripped.split('/').collect();
        if parts.len() >= 2 {
            let bucket = parts[0].to_string();
            let key = parts[1..].join("/");
            return Ok((bucket, key));
        }
        return Err(anyhow::anyhow!("Invalid S3 URL: missing bucket and key"));
    }

    Err(anyhow::anyhow!(
        "Invalid S3 URL format. Expected s3://bucket/key"
    ))
}

/// Client for interacting with the moxpaper daemon
#[cfg(feature = "client")]
pub struct MoxpaperClient {
    ipc: Ipc<crate::ipc::Client>,
    outputs: Vec<OutputInfo>,
}

/// Builder for configuring and setting wallpapers
#[cfg(feature = "client")]
pub struct WallpaperBuilder<'a> {
    client: &'a mut MoxpaperClient,
    data: Option<Data>,
    outputs: Vec<String>,
    resize: Option<ResizeStrategy>,
    transition: Option<Transition>,
}

#[cfg(feature = "client")]
impl<'a> WallpaperBuilder<'a> {
    /// Set the wallpaper source to a file path
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data = Some(Data::Path(path.into()));
        self
    }

    /// Set the wallpaper source to raw image data
    pub fn image(mut self, image_data: ImageData) -> Self {
        self.data = Some(Data::Image(image_data));
        self
    }

    /// Set the wallpaper source to a solid color
    pub fn color(mut self, color: [u8; 3]) -> Self {
        self.data = Some(Data::Color(color));
        self
    }

    pub fn http_data(mut self, url: String, headers: Option<Vec<(String, String)>>) -> Self {
        self.data = Some(Data::Http { url, headers });
        self
    }

    pub fn s3_url<T>(mut self, url: T) -> Self
    where
        T: Into<String>,
    {
        let url = url.into();
        let (bucket, key) =
            parse_s3_url(&url).unwrap_or_else(|_| panic!("Failed to parse S3 URL: {}", url));

        self.data = Some(Data::S3 { bucket, key });
        self
    }

    /// Set target outputs (empty vec means all outputs)
    pub fn outputs(mut self, outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.outputs = outputs.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Set resize strategy
    pub fn resize(mut self, resize: ResizeStrategy) -> Self {
        self.resize = Some(resize);
        self
    }

    /// Set transition configuration
    pub fn transition(mut self, transition: Transition) -> Self {
        self.transition = Some(transition);
        self
    }

    /// Apply the wallpaper configuration
    pub fn apply(self) -> anyhow::Result<()> {
        let data = self
            .data
            .ok_or_else(|| anyhow::anyhow!("Wallpaper source not set"))?;
        let resize = self.resize.unwrap_or(ResizeStrategy::Crop);
        let transition = self.transition.unwrap_or_default();

        // For color, default to No resize strategy
        let resize = match &data {
            Data::Color(_) => ResizeStrategy::No,
            _ => resize,
        };

        self.client.send_wallpaper_data(WallpaperData {
            outputs: self.outputs.into_iter().map(|s| s.into()).collect(),
            data,
            resize,
            transition,
        })
    }
}

#[cfg(feature = "client")]
impl MoxpaperClient {
    /// Connect to the moxpaper daemon and retrieve output information
    pub fn connect() -> anyhow::Result<Self> {
        let ipc = Ipc::connect().context("Failed to connect to IPC")?;
        let mut ipc_stream = ipc.get_stream();

        // Read output information
        let mut buf = String::new();
        let mut ipc_reader = BufReader::new(&mut ipc_stream);
        ipc_reader.read_line(&mut buf)?;

        let outputs: Vec<OutputInfo> =
            serde_json::from_str(&buf).context("Failed to parse output information from daemon")?;

        Ok(Self { ipc, outputs })
    }

    /// Get information about all available outputs
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }

    /// Create a builder for setting a wallpaper
    pub fn set(&mut self) -> WallpaperBuilder<'_> {
        WallpaperBuilder {
            client: self,
            data: None,
            outputs: Vec::new(),
            resize: None,
            transition: None,
        }
    }

    /// Helper method to send wallpaper data to the daemon
    fn send_wallpaper_data(&mut self, data: WallpaperData) -> anyhow::Result<()> {
        let mut stream = self.ipc.get_stream();
        let json = serde_json::to_string(&data).context("Failed to serialize wallpaper data")?;
        stream
            .write_all(json.as_bytes())
            .context("Failed to send wallpaper data to daemon")?;
        Ok(())
    }

    /// Build a transition configuration
    #[cfg(feature = "client")]
    pub fn transition(
        transition_type: Option<TransitionType>,
        fps: Option<u64>,
        duration: Option<u128>,
        bezier: Option<BezierChoice>,
    ) -> Transition {
        Transition {
            transition_type,
            fps,
            duration,
            bezier,
        }
    }
}
