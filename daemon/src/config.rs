use common::ipc::{BezierChoice, ResizeStrategy, Transition, TransitionType};
use inotify::{Inotify, WatchMask};
use mlua::{Function, Lua, LuaSerdeExt, Table};
use serde::Deserialize;
use std::{
    collections::HashMap,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize, Debug)]
pub struct Wallpaper {
    pub path: Box<Path>,
    #[serde(default)]
    pub resize: ResizeStrategy,
    #[serde(default)]
    pub transition: Transition,
}

#[derive(Debug, Default, Clone)]
pub struct LuaTransitionEnv {
    pub lua: Lua,
    pub transition_functions: HashMap<Arc<str>, mlua::Function>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config {
    #[serde(default = "get_default_transition_duration")]
    pub default_transition_duration: u128,
    #[serde(default = "get_default_transition_type")]
    pub default_transition_type: TransitionType,
    #[serde(default = "get_default_bezier")]
    pub default_bezier: BezierChoice,
    pub default_fps: Option<u64>,
    pub wallpaper: HashMap<Arc<str>, Wallpaper>,
    pub bezier: HashMap<Box<str>, (f32, f32, f32, f32)>,
    #[serde(skip)]
    pub lua_env: LuaTransitionEnv,
}

fn get_default_transition_duration() -> u128 {
    3000
}

fn get_default_transition_type() -> TransitionType {
    TransitionType::Simple
}

fn get_default_bezier() -> BezierChoice {
    BezierChoice::Custom((0.54, 0., 0.34, 0.99))
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
                    lua_env.transition_functions.insert(name.into(), func);
                });

            config.lua_env = lua_env;
        }

        config
    }

    fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }

    pub fn watch(&self) -> anyhow::Result<i32> {
        let config_path = Self::xdg_config_dir()?;

        let candidates = [
            config_path.join("mox/moxpaper/config.lua"),
            config_path.join("moxpaper/config.lua"),
        ];

        let inotify = Inotify::init().expect("Failed to initialize inotify");

        candidates.iter().for_each(|candidate| {
            _ = inotify.watches().add(
                candidate.parent().unwrap(),
                WatchMask::CREATE
                    | WatchMask::CLOSE_WRITE
                    | WatchMask::MODIFY
                    | WatchMask::DELETE
                    | WatchMask::MOVE,
            );
        });

        let fd = inotify.as_raw_fd();
        Box::leak(Box::new(inotify));

        Ok(fd)
    }
}
