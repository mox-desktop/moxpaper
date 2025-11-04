use common::ipc::{BezierChoice, ResizeStrategy, Transition, TransitionType};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tvix_serde::from_str;

#[derive(Deserialize, Debug)]
pub struct Wallpaper {
    pub path: Box<Path>,
    #[serde(default)]
    pub resize: ResizeStrategy,
    #[serde(default)]
    pub transition: Transition,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPreference {
    HighPerformance,
    LowPerformance,
}

#[derive(Deserialize, Debug)]
pub struct S3Bucket {
    pub url: String,
    pub region: Option<String>,

    pub access_key: Option<String>,
    pub access_key_file: Option<String>,

    pub secret_key: Option<String>,
    pub secret_key_file: Option<String>,
}

impl S3Bucket {
    pub fn get_access_key(&self) -> anyhow::Result<String> {
        match (&self.access_key, &self.access_key_file) {
            (Some(key), None) => Ok(key.clone()),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("Failed to read access key from {}: {}", file, e)),
            (Some(_), Some(_)) => Err(anyhow::anyhow!(
                "Both access_key and access_key_file are set, only one should be provided"
            )),
            (None, None) => Err(anyhow::anyhow!(
                "Either access_key or access_key_file must be provided"
            )),
        }
    }

    pub fn get_secret_key(&self) -> anyhow::Result<String> {
        match (&self.secret_key, &self.secret_key_file) {
            (Some(key), None) => Ok(key.clone()),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("Failed to read secret key from {}: {}", file, e)),
            (Some(_), Some(_)) => Err(anyhow::anyhow!(
                "Both secret_key and secret_key_file are set, only one should be provided"
            )),
            (None, None) => Err(anyhow::anyhow!(
                "Either secret_key or secret_key_file must be provided"
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub buckets: HashMap<String, S3Bucket>,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            buckets: HashMap::new(),
            power_preference: None,
            enabled_transition_types: None,
            default_transition_duration: 3000,
            default_transition_type: TransitionType::Simple,
            default_bezier: BezierChoice::Custom((0.54, 0., 0.34, 0.99)),
            default_fps: None,
            wallpaper: HashMap::new(),
            bezier: HashMap::new(),
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
        let nix_code = if let Some(p) = path {
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
                        base.join("mox/moxpaper/default.nix"),
                        base.join("mox/moxpaper.nix"),
                    ];
                    match candidates
                        .iter()
                        .find_map(|p| std::fs::read_to_string(p).ok())
                    {
                        Some(content) => content,
                        None => {
                            log::warn!("Config file not found");
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

        match from_str(&nix_code) {
            Ok(config) => config,
            Err(e) => {
                log::error!("{e}");
                Config::default()
            }
        }
    }

    pub fn xdg_config_dir() -> anyhow::Result<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map_err(Into::into)
    }
}
