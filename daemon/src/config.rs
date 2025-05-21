use common::ipc::{BezierChoice, ResizeStrategy, Transition, TransitionType};
use mlua::{Function, Lua, LuaSerdeExt, Table};
use serde::Deserialize;
use std::{
    collections::HashMap,
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPreference {
    HighPerformance,
    LowPerformance,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub power_preference: Option<PowerPreference>,
    pub enabled_transition_types: Option<Arc<[TransitionType]>>,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            power_preference: None,
            enabled_transition_types: None,
            default_transition_duration: 3000,
            default_transition_type: TransitionType::Simple,
            default_bezier: BezierChoice::Custom((0.54, 0., 0.34, 0.99)),
            default_fps: None,
            wallpaper: HashMap::new(),
            bezier: HashMap::new(),
            lua_env: LuaTransitionEnv::default(),
        }
    }
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

        let mut config: Config = match lua.from_value(value) {
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

    pub fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }
}
