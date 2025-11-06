#[cfg(any(feature = "server", feature = "client"))]
pub mod image_data;

#[cfg(feature = "server")]
pub mod ipc;

#[cfg(all(feature = "client", not(feature = "server")))]
mod ipc;

#[cfg(any(feature = "server", feature = "client"))]
mod types;

#[cfg(feature = "client")]
mod client;

#[cfg(any(feature = "server", feature = "client"))]
pub use image_data::ImageData;

#[cfg(any(feature = "server", feature = "client"))]
pub use types::{
    BezierChoice, Data, OutputInfo, ResizeStrategy, Transition, TransitionType, WallpaperData,
};

#[cfg(feature = "client")]
pub use client::{MoxpaperClient, WallpaperBuilder};
