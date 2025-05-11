use crate::{image_data::ImageData, ipc::ResizeStrategy};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize, Serialize, Clone)]
pub enum CacheEntry {
    Path {
        path: Arc<Path>,
        resize: ResizeStrategy,
    },
    Image {
        image: ImageData,
        resize: ResizeStrategy,
    },
    Color([u8; 3]),
}

pub fn store(output_name: &str, cache_entry: CacheEntry) -> anyhow::Result<()> {
    let mut filepath = cache_dir()?;
    filepath.push(output_name);

    let data = serde_json::to_string(&cache_entry)?;

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
