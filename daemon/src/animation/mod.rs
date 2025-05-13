pub mod bezier;

use crate::Moxpaper;
use bezier::Bezier;
use calloop::{
    timer::{TimeoutAction, Timer},
    LoopHandle,
};
use common::ipc::{Transition, TransitionType};
use rand::prelude::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default)]
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

#[derive(Clone, Copy)]
pub struct Transform {
    pub bounds: Bound,
    pub alpha: f32,
    pub radius: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            radius: 0.0,
            bounds: Bound::default(),
        }
    }
}

pub struct Animation {
    bezier: Option<Bezier>,
    transition: Option<Transition>,
    start_time: Option<Instant>,
    is_active: bool,
    progress: f32,
    time_factor: f32,
    handle: LoopHandle<'static, Moxpaper>,
    rand: Option<f32>,
    rand_transition: Option<TransitionType>,
}

impl Animation {
    pub fn new(handle: LoopHandle<'static, Moxpaper>) -> Self {
        Self {
            handle,
            bezier: None,
            transition: None,
            start_time: None,
            is_active: false,
            time_factor: 0.0,
            progress: 0.0,
            rand: None,
            rand_transition: None,
        }
    }

    pub fn start(&mut self, output_name: &str, transition: Transition, bezier: Bezier) {
        let mut rng = rand::rng();

        self.rand = Some(rng.random_range(0_f32..=1_f32));
        self.rand_transition = Some(rng.random());
        self.progress = 0.0;
        self.start_time = None;
        self.is_active = true;
        self.transition = Some(transition);
        self.bezier = Some(bezier);

        let output_name = output_name.into();
        self.handle
            .insert_source(Timer::immediate(), move |_, _, state| {
                let output_name = Arc::clone(&output_name);

                let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| output.info.name == output_name)
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

                match output.animation.transition.as_ref().and_then(|t| t.fps) {
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

        let Some(transition) = self.transition.as_ref() else {
            return false;
        };

        let elapsed_ms = start_time.elapsed().as_millis();
        if elapsed_ms >= transition.duration {
            self.progress = 1.0;
            self.is_active = false;
            return true;
        }

        let linear_progress =
            start_time.elapsed().as_secs_f32() / (transition.duration / 1000) as f32;

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
        let transition_type = match &self.transition {
            Some(transition) => transition.transition_type,
            None => TransitionType::None,
        };

        let transition = match transition_type {
            TransitionType::None => Transform::default(),

            TransitionType::Fade => Transform {
                alpha: self.progress,
                ..Default::default()
            },

            TransitionType::Simple => Transform {
                alpha: self.progress,
                ..Default::default()
            },

            TransitionType::Right => {
                let bounds = Bound::new(Some(1.0 - self.progress), None, None, None)?;

                Transform {
                    bounds,
                    ..Default::default()
                }
            }

            TransitionType::Left => {
                let bounds = Bound::new(None, None, Some(self.progress), None)?;

                Transform {
                    bounds,
                    ..Default::default()
                }
            }

            TransitionType::Top => {
                let bounds = Bound::new(None, Some(1.0 - self.progress), None, None)?;

                Transform {
                    bounds,
                    ..Default::default()
                }
            }

            TransitionType::Bottom => {
                let bounds = Bound::new(None, None, None, Some(self.progress))?;

                Transform {
                    bounds,
                    ..Default::default()
                }
            }

            TransitionType::Center => {
                let center = 0.5;
                let half_extent = 0.5 * self.progress;
                let bounds = Bound::new(
                    Some(center - half_extent),
                    Some(center - half_extent),
                    Some(center + half_extent),
                    Some(center + half_extent),
                )?;

                Transform {
                    bounds,
                    radius: 1.0 - self.progress,
                    ..Default::default()
                }
            }

            TransitionType::Any => {
                let rand = self.rand.unwrap_or(0.5);
                let bounds = Bound::new(
                    Some(rand - self.progress),
                    Some(rand - self.progress),
                    Some(rand + self.progress),
                    Some(rand + self.progress),
                )?;

                Transform {
                    bounds,
                    radius: (1.0 - self.progress) * (0.8 + 0.2 * (self.time_factor * 5.0).sin()),
                    ..Default::default()
                }
            }

            TransitionType::Random => Transform::default(),

            _ => Transform::default(),
        };

        Ok(transition)
    }
}

//TransitionType::Fade => {
//let angle = self.time_factor * std::f32::consts::PI * 4.0;
//let distance = (1.0 - progress) * 0.5;
//let center_x = 0.5 + distance * angle.cos();
//let center_y = 0.5 + distance * angle.sin();

//Transform {
//bound_left: Some(center_x - progress * 0.5),
//bound_top: Some(center_y - progress * 0.5),
//bound_right: Some(center_x + progress * 0.5),
//bound_bottom: Some(center_y + progress * 0.5),
//radius: 0.5 * (1.0 - self.time_factor),
//..Default::default()
//}
//}
//
//             TransitionType::Center => {
//    let center = 0.5;
//    let half_extent = 0.5 * self.progress;
//    Transform {
//        bound_left: Some(center - half_extent),
//        bound_top: Some(center - half_extent),
//        bound_right: Some(center + half_extent),
//        bound_bottom: Some(center + half_extent),
//        radius: 1.0 - self.progress,
//        ..Default::default()
//    }
//}
