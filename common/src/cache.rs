use std::{
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::Context;

pub fn store(output_name: &str, img_path: &str) -> anyhow::Result<()> {
    let mut filepath = cache_dir()?;
    filepath.push(output_name);
    std::fs::File::create(filepath)?
        .write_all(img_path.as_bytes())
        .context("Failed to write to the cache")
}

pub fn load(output_name: &str) -> Option<String> {
    let mut filepath = cache_dir().ok()?;

    filepath.push(output_name);

    let mut buf = Vec::with_capacity(64);
    std::fs::File::open(filepath)
        .ok()?
        .read_to_end(&mut buf)
        .ok()?;

    String::from_utf8(buf).ok()
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
