pub mod bezier;

use crate::{config::LuaTransitionEnv, Moxpaper};
use bezier::{Bezier, BezierBuilder};
use calloop::{
    timer::{TimeoutAction, Timer},
    LoopHandle,
};
use common::ipc::TransitionType;
use mlua::{IntoLua, Table};
use rand::prelude::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct Extents {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Extents {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            width: 1.,
            height: 1.,
        }
    }
}

impl IntoLua for Extents {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let table = lua.create_table()?;
        table.set("x", self.x)?;
        table.set("y", self.y)?;
        table.set("width", self.width)?;
        table.set("height", self.height)?;
        Ok(mlua::Value::Table(table))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Clip {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            left: 0.,
            right: 1.,
            top: 0.,
            bottom: 1.,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub clip: Clip,
    pub extents: Extents,
    pub opacity: f32,
    pub radius: [f32; 4],
    pub rotation: f32,
    pub blur: u32,
    pub blur_color: [f32; 4],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            radius: [0.0; 4],
            rotation: 0.0,
            clip: Clip::default(),
            extents: Extents::default(),
            blur: 0,
            blur_color: [0.; 4],
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransitionConfig {
    pub transition_type: TransitionType,
    pub fps: Option<u64>,
    pub duration: u128,
    pub bezier: Bezier,
    pub enabled_transition_types: Option<Arc<[TransitionType]>>,
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            enabled_transition_types: None,
            transition_type: TransitionType::default(),
            fps: None,
            duration: 300,
            bezier: BezierBuilder::new().ease_in(),
        }
    }
}

pub struct Animation {
    bezier: Option<Bezier>,
    transition_config: Option<TransitionConfig>,
    start_time: Option<Instant>,
    is_active: bool,
    progress: f32,
    time_factor: f32,
    handle: LoopHandle<'static, Moxpaper>,
    rand: Option<f32>,
    rand_transition: Option<TransitionType>,
    extents: Extents,
    lua_env: Option<LuaTransitionEnv>,
}

impl Animation {
    pub fn new(handle: LoopHandle<'static, Moxpaper>) -> Self {
        Self {
            handle,
            bezier: None,
            transition_config: None,
            start_time: None,
            is_active: false,
            time_factor: 0.0,
            progress: 0.0,
            rand: None,
            extents: Extents::default(),
            lua_env: None,
            rand_transition: None,
        }
    }

    pub fn start(
        &mut self,
        output_name: &str,
        transition_config: TransitionConfig,
        extents: Extents,
        lua_env: LuaTransitionEnv,
    ) {
        let mut rng = rand::rng();

        self.rand_transition = {
            let mut all_transitions = vec![
                TransitionType::None,
                TransitionType::Simple,
                TransitionType::Fade,
                TransitionType::Left,
                TransitionType::Right,
                TransitionType::Top,
                TransitionType::Bottom,
                TransitionType::Center,
                TransitionType::Outer,
                TransitionType::Any,
                TransitionType::Wipe,
                TransitionType::Wave,
                TransitionType::Grow,
            ];

            all_transitions.extend(
                lua_env
                    .transition_functions
                    .keys()
                    .map(|name| TransitionType::Custom(Arc::clone(name))),
            );

            let enabled_transitions: Vec<_> = all_transitions
                .into_iter()
                .filter(|transition_type| {
                    transition_config
                        .enabled_transition_types
                        .as_ref()
                        .is_none_or(|enabled| enabled.contains(transition_type))
                })
                .collect();

            if enabled_transitions.is_empty() {
                Some(TransitionType::None)
            } else {
                let random_index = rng.random_range(0..enabled_transitions.len());
                Some(enabled_transitions[random_index].clone())
            }
        };

        self.extents = extents;
        self.rand = Some(rng.random_range(0.0..=1.0));
        self.progress = 0.0;
        self.start_time = None;
        self.is_active = true;

        self.bezier = Some(transition_config.bezier.clone());
        self.transition_config = Some(transition_config);
        self.lua_env = Some(lua_env);

        let output_name = output_name.to_string();
        self.handle
            .insert_source(Timer::immediate(), move |_, _, state| {
                let output_name = output_name.clone();

                let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| *output.info.name == output_name)
                else {
                    return TimeoutAction::Drop;
                };

                output.animation.update();

                output.render();

                if output.animation.start_time.is_none() {
                    output.animation.start_time = Some(Instant::now());
                }

                if !output.animation.is_active() {
                    output.previous_image = output.target_image.take();
                    return TimeoutAction::Drop;
                }

                match output
                    .animation
                    .transition_config
                    .as_ref()
                    .and_then(|t| t.fps)
                {
                    Some(fps) => TimeoutAction::ToDuration(Duration::from_millis(1000 / fps)),
                    None => TimeoutAction::ToDuration(Duration::ZERO), // Vsync
                }
            })
            .unwrap();
    }

    pub fn update(&mut self) -> bool {
        if !self.is_active {
            return false;
        }

        let Some(start_time) = self.start_time else {
            return false;
        };

        let Some(transition_config) = self.transition_config.as_ref() else {
            return false;
        };

        let elapsed_ms = start_time.elapsed().as_millis();
        if elapsed_ms >= transition_config.duration {
            self.progress = 1.0;
            self.is_active = false;
            return true;
        }

        let linear_progress =
            start_time.elapsed().as_secs_f32() / (transition_config.duration / 1000) as f32;

        match &self.bezier {
            Some(bezier) => {
                let (time_factor, progress_value) = bezier.evaluate(linear_progress);

                self.progress = progress_value;
                self.time_factor = time_factor;
            }
            None => self.progress = linear_progress,
        };

        false
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn calculate_transform(&self) -> anyhow::Result<Transform> {
        let Some(transition_config) = &self.transition_config else {
            return Ok(Transform::default());
        };

        match &transition_config.transition_type {
            TransitionType::None => Ok(Transform::default()),

            TransitionType::Fade => Ok(Transform {
                opacity: self.progress,
                ..Default::default()
            }),

            TransitionType::Simple => Ok(Transform {
                opacity: self.progress,
                ..Default::default()
            }),

            TransitionType::Right => {
                let clip = Clip {
                    left: 1.0 - self.progress,
                    ..Default::default()
                };
                Ok(Transform {
                    clip,
                    ..Default::default()
                })
            }

            TransitionType::Left => {
                let clip = Clip {
                    right: self.progress,
                    ..Default::default()
                };

                Ok(Transform {
                    clip,
                    ..Default::default()
                })
            }

            TransitionType::Top => {
                let clip = Clip {
                    top: 1.0 - self.progress,
                    ..Default::default()
                };

                Ok(Transform {
                    clip,
                    ..Default::default()
                })
            }

            TransitionType::Bottom => {
                let clip = Clip {
                    bottom: self.progress,
                    ..Default::default()
                };
                Ok(Transform {
                    clip,
                    ..Default::default()
                })
            }

            TransitionType::Center => {
                let center = 0.5;
                let max_extent = self.progress * 0.5;

                let x_scale = (self.extents.height / self.extents.width).max(1.0);
                let y_scale = (self.extents.width / self.extents.height).max(1.0);

                let half_extent_x = max_extent * x_scale;
                let half_extent_y = max_extent * y_scale;

                let clip = Clip {
                    left: center - half_extent_x,
                    top: center - half_extent_y,
                    right: center + half_extent_x,
                    bottom: center + half_extent_y,
                };

                Ok(Transform {
                    clip,
                    radius: [(1.0 - self.progress) * (0.8 + 0.2 * (self.time_factor * 5.0).sin());
                        4],
                    ..Default::default()
                })
            }

            TransitionType::Any => {
                let rand = self.rand.unwrap_or(0.5);
                let clip = Clip {
                    left: rand - self.progress,
                    top: rand - self.progress,
                    right: rand + self.progress,
                    bottom: rand + self.progress,
                };

                Ok(Transform {
                    clip,
                    radius: [(1.0 - self.progress) * (0.8 + 0.2 * (self.time_factor * 5.0).sin());
                        4],
                    ..Default::default()
                })
            }

            TransitionType::Random => {
                if let Some(picked) = self.rand_transition.clone() {
                    let mut temp_config = transition_config.clone();
                    temp_config.transition_type = picked;
                    let saved_bezier = self.bezier.clone();
                    let saved_lua = self.lua_env.clone();

                    let temp_anim = Animation {
                        bezier: saved_bezier,
                        transition_config: Some(temp_config),
                        start_time: self.start_time,
                        is_active: self.is_active,
                        progress: self.progress,
                        time_factor: self.time_factor,
                        handle: self.handle.clone(),
                        rand: self.rand,
                        rand_transition: self.rand_transition.clone(),
                        extents: self.extents,
                        lua_env: saved_lua,
                    };

                    return temp_anim.calculate_transform();
                }

                Ok(Transform::default())
            }

            TransitionType::Custom(function_name) => {
                if let Some(lua_env) = self.lua_env.as_ref() {
                    let table = match lua_env.lua.create_table() {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!(
                                "Custom transition `{function_name}`: failed to create Lua table: {e}",
                            );
                            return Ok(Transform::default());
                        }
                    };
                    _ = table.set("progress", self.progress);
                    _ = table.set("time_factor", self.time_factor);
                    _ = table.set("random", self.rand);
                    _ = table.set("extents", self.extents);

                    if let Some(func) = lua_env.transition_functions.get(function_name) {
                        let result: mlua::Table =
                            func.call(table).map_err(|e| anyhow::anyhow!("{e}"))?;

                        let clip = match result.get::<Table>("clip") {
                            Ok(clip) => Clip {
                                left: clip.get("left").unwrap_or_default(),
                                top: clip.get("top").unwrap_or_default(),
                                right: clip.get("right").unwrap_or(1.),
                                bottom: clip.get("bottom").unwrap_or(1.),
                            },
                            Err(_) => Clip::default(),
                        };

                        let extents = match result.get::<Table>("extents") {
                            Ok(extents) => Extents {
                                x: extents.get("x").unwrap_or_default(),
                                y: extents.get("y").unwrap_or_default(),
                                width: extents.get("width").unwrap_or(1.),
                                height: extents.get("height").unwrap_or(1.),
                            },
                            Err(_) => Extents::default(),
                        };

                        Ok(Transform {
                            clip,
                            opacity: result.get("opacity").unwrap_or(1.0),
                            radius: result.get("radius").unwrap_or_default(),
                            rotation: result.get("rotation").unwrap_or_default(),
                            extents,
                            blur: result.get("blur").unwrap_or_default(),
                            blur_color: result.get("blur_color").unwrap_or_default(),
                        })
                    } else {
                        Ok(Transform::default())
                    }
                } else {
                    Ok(Transform::default())
                }
            }

            _ => Ok(Transform::default()),
        }
    }
}
