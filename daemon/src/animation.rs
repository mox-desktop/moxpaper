use crate::Moxpaper;
use calloop::{
    timer::{TimeoutAction, Timer},
    LoopHandle,
};
use common::ipc::{Transition, TransitionType};
use rand::prelude::*;
use std::{
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Bezier((f32, f32, f32, f32));

impl Bezier {
    pub fn custom(curve: (f32, f32, f32, f32)) -> Self {
        Self(curve)
    }

    pub fn linear() -> Self {
        Self((0.0, 0.0, 1.0, 1.0))
    }

    pub fn ease() -> Self {
        Self((0.25, 0.1, 0.25, 1.0))
    }

    pub fn ease_in() -> Self {
        Self((0.42, 0.0, 1.0, 1.0))
    }

    pub fn ease_out() -> Self {
        Self((0.0, 0.0, 0.58, 1.0))
    }

    pub fn ease_in_out() -> Self {
        Self((0.42, 0.0, 0.58, 1.0))
    }

    pub fn evaluate(&self, t: f32) -> (f32, f32) {
        let (x1, y1, x2, y2) = self.0;

        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;
        let t2 = t * t;
        let t3 = t2 * t;

        let x = 0.0 * mt3 + 3.0 * mt2 * t * x1 + 3.0 * mt * t2 * x2 + 1.0 * t3;
        let y = 0.0 * mt3 + 3.0 * mt2 * t * y1 + 3.0 * mt * t2 * y2 + 1.0 * t3;

        (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
    }
}

impl Deref for Bezier {
    type Target = (f32, f32, f32, f32);

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Copy)]
pub struct Transform {
    pub bound_left: Option<f32>,
    pub bound_top: Option<f32>,
    pub bound_right: Option<f32>,
    pub bound_bottom: Option<f32>,
    pub alpha: f32,
    pub radius: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            bound_left: None,
            bound_top: None,
            bound_right: None,
            bound_bottom: None,
            alpha: 1.0,
            radius: 0.0,
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

    pub fn calculate_transform(&self) -> Transform {
        let transition_type = match &self.transition {
            Some(transition) => transition.transition_type,
            None => TransitionType::None,
        };

        match transition_type {
            TransitionType::None => Transform::default(),

            TransitionType::Fade => Transform {
                alpha: self.progress,
                ..Default::default()
            },
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
            TransitionType::Simple => Transform {
                alpha: self.progress,
                ..Default::default()
            },

            TransitionType::Right => Transform {
                bound_left: Some(1.0 - self.progress),
                alpha: 1.0,
                ..Default::default()
            },

            TransitionType::Left => Transform {
                bound_right: Some(self.progress),
                ..Default::default()
            },

            TransitionType::Top => Transform {
                bound_top: Some(1.0 - self.progress),
                ..Default::default()
            },

            TransitionType::Bottom => Transform {
                bound_bottom: Some(self.progress),
                ..Default::default()
            },

            TransitionType::Center => {
                let center = 0.5;
                let half_extent = 0.5 * self.progress;
                Transform {
                    bound_left: Some(center - half_extent),
                    bound_top: Some(center - half_extent),
                    bound_right: Some(center + half_extent),
                    bound_bottom: Some(center + half_extent),
                    radius: 1.0 - self.progress,
                    ..Default::default()
                }
            }

            TransitionType::Any => {
                let rand = self.rand.unwrap_or(0.5);
                Transform {
                    bound_left: Some(rand - self.progress),
                    bound_top: Some(rand - self.progress),
                    bound_right: Some(rand + self.progress),
                    bound_bottom: Some(rand + self.progress),
                    radius: 1.0 - self.progress,
                    ..Default::default()
                }
            }

            TransitionType::Random => Transform::default(),

            _ => Transform::default(),
        }
    }
}
