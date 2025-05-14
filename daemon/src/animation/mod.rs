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
     time::{Duration, Instant}
};

/// Represents the rectangular dimensions of an element
#[derive(Debug, Clone, Copy, Default)]
pub struct Extents {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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

/// Defines boundaries for a transform
#[derive(Debug, Clone, Copy, Default)]
pub struct Bound {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
}

impl Bound {
    pub fn is_valid(&self) -> bool {
        if let (Some(left), Some(right)) = (self.left, self.right) {
            if left > right {
                return false;
            }
        }

        if let (Some(top), Some(bottom)) = (self.top, self.bottom) {
            if top > bottom {
                return false;
            }
        }

        true
    }

    pub fn new(
        left: Option<f32>,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
    ) -> anyhow::Result<Self> {
        let bound = Self {
            left,
            top,
            right,
            bottom,
        };
        if bound.is_valid() {
            Ok(bound)
        } else {
            Err(anyhow::anyhow!(
                "Invalid bounds: left must be less than right and top must be less than bottom"
            ))
        }
    }
}

/// Represents a transformation to be applied during an animation
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub bounds: Bound,
    pub alpha: f32,
    pub radius: f32,
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            radius: 0.0,
            rotation: 0.0,
            bounds: Bound::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransitionConfig {
    pub transition_type: TransitionType,
    pub fps: Option<u64>,
    pub duration: u128,
    pub bezier: Bezier,
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            transition_type: TransitionType::default(),
            fps: Some(60),
            duration: 500,
            bezier: BezierBuilder::new().ease_in(),
        }
    }
}

/// Core animation system
pub struct Animation {
    bezier: Option<Bezier>,
    transition_config: Option<TransitionConfig>,
    start_time: Option<Instant>,
    is_active: bool,
    progress: f32,
    time_factor: f32,
    handle: LoopHandle<'static, Moxpaper>,
    rand: Option<f32>,
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
        }
    }

    pub fn start(
        &mut self,
        output_name: &str,
        transition_config: TransitionConfig,
        extents: Extents,
        lua_env: Option<LuaTransitionEnv>,
    ) {
        let mut rng = rand::rng();

        self.extents = extents;
        self.rand = Some(rng.random_range(0.0..=1.0));
        self.progress = 0.0;
        self.start_time = None;
        self.is_active = true;
        
        self.transition_config = Some(transition_config.clone());
        self.bezier = Some(transition_config.bezier);
        self.lua_env = lua_env;

        let output_name = output_name.to_string();
        self.handle
            .insert_source(Timer::immediate(), move |_, _, state| {
                let output_name = output_name.clone();

                let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| *output.info.name ==output_name)
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

                match output.animation.transition_config.as_ref().and_then(|t| t.fps) {
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
                alpha: self.progress,
                ..Default::default()
            }),

            TransitionType::Simple => Ok(Transform {
                alpha: self.progress,
                ..Default::default()
            }),

            TransitionType::Right => {
                let bounds = Bound::new(Some(1.0 - self.progress), None, None, None)?;
                Ok(Transform {
                    bounds,
                    ..Default::default()
                })
            }

            TransitionType::Left => {
                let bounds = Bound::new(None, None, Some(self.progress), None)?;
                Ok(Transform {
                    bounds,
                    ..Default::default()
                })
            }

            TransitionType::Top => {
                let bounds = Bound::new(None, Some(1.0 - self.progress), None, None)?;
                Ok(Transform {
                    bounds,
                    ..Default::default()
                })
            }

            TransitionType::Bottom => {
                let bounds = Bound::new(None, None, None, Some(self.progress))?;
                Ok(Transform {
                    bounds,
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

                let bounds = Bound::new(
                    Some(center - half_extent_x),
                    Some(center - half_extent_y),
                    Some(center + half_extent_x),
                    Some(center + half_extent_y),
                )?;

                Ok(Transform {
                    bounds,
                    radius: (1.0 - self.progress) * (0.8 + 0.2 * (self.time_factor * 5.0).sin()),
                    ..Default::default()
                })
            }

            TransitionType::Any => {
                let rand = self.rand.unwrap_or(0.5);
                let bounds = Bound::new(
                    Some(rand - self.progress),
                    Some(rand - self.progress),
                    Some(rand + self.progress),
                    Some(rand + self.progress),
                )?;

                Ok(Transform {
                    bounds,
                    radius: (1.0 - self.progress) * (0.8 + 0.2 * (self.time_factor * 5.0).sin()),
                    ..Default::default()
                })
            }

            TransitionType::Random => Ok(Transform::default()),

            TransitionType::Custom(function_name) => {
                if let Some(lua_env) = self.lua_env.as_ref() {
                    let table = lua_env.lua.create_table().unwrap();
                    _ = table.set("progress", self.progress);
                    _ = table.set("time_factor", self.time_factor);
                    _ = table.set("random", self.rand);
                    _ = table.set("extents", self.extents);
                    
                    if let Some(func) = lua_env.transition_functions.get(function_name) {
                        let result: mlua::Table = func.call(table).unwrap();

                        let bounds = match result.get::<Table>("bounds") {
                            Ok(bounds) => Bound::new(
                                bounds.get("left").ok(),
                                bounds.get("top").ok(),
                                bounds.get("right").ok(),
                                bounds.get("bottom").ok(),
                            )
                            .unwrap_or_default(),
                            Err(_) => Bound::default(),
                        };

                        Ok(Transform {
                            bounds,
                            alpha: result.get("alpha").unwrap_or(1.0),
                            radius: result.get("radius").unwrap_or_default(),
                            rotation: result.get("rotation").unwrap_or_default(),
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
