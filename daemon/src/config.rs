use common::ipc::{ResizeStrategy, TransitionType};
use mlua::{Lua, LuaSerdeExt};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Debug)]
pub struct Wallpaper {
    pub path: Box<Path>,
    #[serde(default)]
    pub resize: ResizeStrategy,
    #[serde(default)]
    pub transition: TransitionType,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config(pub HashMap<Box<str>, Wallpaper>);

impl Config {
    pub fn load<T>(path: Option<T>) -> Self
    where
        T: AsRef<Path>,
    {
        let lua_code = if let Some(p) = path {
            match std::fs::read_to_string(p.as_ref()) {
                Ok(content) => content,
                Err(e) => {
                    log::error!("Failed to read config file: {e}");
                    return Config::default();
                }
            }
        } else {
            match Self::xdg_config_dir() {
                Ok(base) => {
                    let candidates = [
                        base.join("mox/moxpaper/config.lua"),
                        base.join("moxpaper/config.lua"),
                    ];

                    match candidates
                        .iter()
                        .find_map(|p| std::fs::read_to_string(p).ok())
                    {
                        Some(content) => content,
                        None => {
                            log::info!("Config file not found");
                            return Config::default();
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to determine config directory: {e}");
                    return Config::default();
                }
            }
        };

        let lua = Lua::new();

        let value = match lua.load(&lua_code).eval() {
            Ok(v) => v,
            Err(e) => {
                log::error!("Lua eval error: {e}");
                return Config::default();
            }
        };

        match lua.from_value(value) {
            Ok(config) => config,
            Err(e) => {
                log::error!("Config deserialization error: {e}");
                Config::default()
            }
        }
    }

    fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }
}
