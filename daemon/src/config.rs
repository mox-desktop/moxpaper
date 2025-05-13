use common::ipc::{ResizeStrategy, Transition, TransitionType};
use mlua::{Function, Lua, LuaSerdeExt, Table};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

// Define a new enum to represent either a built-in transition or a custom one
#[derive(Clone, Debug)]
pub enum TransitionFunction {
    Builtin(TransitionType),
    Custom(Arc<str>), // Reference to a named function in the lua environment
}

#[derive(Deserialize, Debug)]
pub struct Wallpaper {
    pub path: Box<Path>,
    #[serde(default)]
    pub resize: ResizeStrategy,
    #[serde(default)]
    pub transition: Transition,
}

#[derive(Debug, Default)]
pub struct LuaTransitionEnv {
    lua: Lua,
    transition_functions: HashMap<Arc<str>, mlua::Function>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config {
    pub wallpaper: HashMap<Arc<str>, Wallpaper>,
    pub bezier: HashMap<Box<str>, (f32, f32, f32, f32)>,
    #[serde(skip)]
    pub lua_env: Option<LuaTransitionEnv>,
}

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

        let globals = lua.globals();
        let _ = globals.set(
            "create_bound",
            lua.create_function(
                |_,
                 (left, top, right, bottom): (
                    Option<f32>,
                    Option<f32>,
                    Option<f32>,
                    Option<f32>,
                )| { Ok((left, top, right, bottom)) },
            )
            .unwrap(),
        );

        let value = match lua.load(&lua_code).eval() {
            Ok(v) => v,
            Err(e) => {
                log::error!("Lua eval error: {e}");
                return Config::default();
            }
        };

        let mut config = match lua.from_value(value) {
            Ok(config) => config,
            Err(e) => {
                log::error!("Config deserialization error: {e}");
                Config::default()
            }
        };

        if let Ok(transitions_table) = globals.get::<Table>("transitions") {
            let mut lua_env = LuaTransitionEnv {
                lua,
                transition_functions: HashMap::new(),
            };

            transitions_table
                .pairs::<String, Function>()
                .filter_map(|pair| pair.ok())
                .for_each(|(name, func)| {
                    println!("{name}");
                    lua_env.transition_functions.insert(name.into(), func);
                });

            config.lua_env = Some(lua_env);
        }

        config
    }

    fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }
}
