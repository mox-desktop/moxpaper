use mlua::{Lua, LuaSerdeExt};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {}

impl Config {
    pub fn load<T>(path: Option<T>) -> anyhow::Result<Self>
    where
        T: AsRef<Path>,
    {
        let lua_code = if let Some(p) = path {
            std::fs::read_to_string(p.as_ref()).unwrap_or_default()
        } else {
            let base = Self::xdg_config_dir()?;
            let candidates = [
                base.join("mox/moxnotify/config.lua"),
                base.join("moxnotify/config.lua"),
            ];

            candidates
                .iter()
                .find_map(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default()
        };

        let lua = Lua::new();
        let value = lua
            .load(&lua_code)
            .eval()
            .map_err(|e| anyhow::anyhow!("Lua eval error: {}", e))?;
        lua.from_value(value)
            .map_err(|e| anyhow::anyhow!("Config deserialization error: {}", e))
    }

    fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }
}
