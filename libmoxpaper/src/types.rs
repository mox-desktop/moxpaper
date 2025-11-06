#[cfg(any(feature = "server", feature = "client"))]
use crate::image_data::ImageData;
#[cfg(any(feature = "server", feature = "client"))]
use clap::ValueEnum;
#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

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

