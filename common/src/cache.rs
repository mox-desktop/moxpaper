use crate::image_data::ImageData;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
pub enum CacheEntry {
    Path(Box<Path>),
    Image(ImageData),
    Color([u8; 3]),
}

impl From<PathBuf> for CacheEntry {
    fn from(value: PathBuf) -> Self {
        CacheEntry::Path(value.into())
    }
}

impl From<&Path> for CacheEntry {
    fn from(value: &Path) -> Self {
        CacheEntry::Path(value.into())
    }
}

impl From<ImageData> for CacheEntry {
    fn from(value: ImageData) -> Self {
        CacheEntry::Image(value)
    }
}

impl From<[u8; 3]> for CacheEntry {
    fn from(value: [u8; 3]) -> Self {
        CacheEntry::Color(value)
    }
}

pub fn store<T>(output_name: &str, cache_entry: T) -> anyhow::Result<()>
where
    T: Into<CacheEntry>,
{
    let mut filepath = cache_dir()?;
    filepath.push(output_name);

    let data = serde_json::to_string(&cache_entry.into())?;

    std::fs::File::create(filepath)?
        .write_all(data.as_bytes())
        .context("Failed to write to the cache")
}

pub fn load(output_name: &str) -> Option<CacheEntry> {
    let mut filepath = cache_dir().ok()?;

    filepath.push(output_name);

    let mut buf = Vec::with_capacity(64);
    std::fs::File::open(filepath)
        .ok()?
        .read_to_end(&mut buf)
        .ok()?;

    serde_json::from_slice(&buf).ok()
}

fn cache_dir() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("XDG_CACHE_HOME") {
        let mut path: PathBuf = path.into();
        path.push("moxpaper");
        _ = std::fs::create_dir(&path);
        Ok(path)
    } else if let Ok(path) = std::env::var("HOME") {
        let mut path: PathBuf = path.into();
        path.push(".cache");
        path.push("moxpaper");
        _ = std::fs::create_dir(&path);
        Ok(path)
    } else {
        Err(anyhow::anyhow!(
            "failed to read both $XDG_CACHE_HOME and $HOME environment variables"
        ))
    }
}
