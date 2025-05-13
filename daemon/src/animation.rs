use crate::Moxpaper;
use calloop::{
    timer::{TimeoutAction, Timer},
    LoopHandle,
};
use common::{
    image_data::ImageData,
    ipc::{Transition, TransitionType},
};
use rand::prelude::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
pub struct Animation {
    pub transition: Transition,
    start_time: Option<Instant>,
    is_active: bool,
    pub progress: f32,
    pub previous_image: Option<ImageData>,
    pub target_image: Option<ImageData>,
    handle: LoopHandle<'static, Moxpaper>,
    rand: f32,
    rand_transition: TransitionType,
}

impl Animation {
    pub fn new(handle: LoopHandle<'static, Moxpaper>) -> Self {
        Self {
            handle,
            transition: Transition::default(),
            start_time: None,
            is_active: false,
            progress: 0.0,
            previous_image: None,
            target_image: None,
            rand: 0.,
            rand_transition: TransitionType::None,
        }
    }

    pub fn start(&mut self, target_image: ImageData, output_name: &str, transition: Transition) {
        self.progress = 0.0;
        self.previous_image = self.target_image.take();
        self.start_time = None;
        self.target_image = Some(target_image);
        self.is_active = true;
        self.transition = transition;
        let mut rng = rand::rng();
        self.rand = rng.random_range(0_f32..=1_f32);
        self.rand_transition = rng.random();

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
                    return TimeoutAction::Drop;
                }

                match output.animation.transition.fps {
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

        if start_time.elapsed().as_millis() >= self.transition.duration {
            self.progress = 1.0;
            self.is_active = false;
            self.previous_image = None;
            return true;
        }

        self.progress =
            start_time.elapsed().as_secs_f32() / (self.transition.duration / 1000) as f32;
        false
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn calculate_transform(&self) -> Transform {
        let progress = self.progress;

        match self.transition.transition_type {
            TransitionType::None => Transform::default(),

            TransitionType::Fade => Transform {
                alpha: progress,
                ..Default::default()
            },

            TransitionType::Simple => Transform {
                alpha: progress,
                ..Default::default()
            },

            TransitionType::Right => Transform {
                bound_left: Some(1.0 - progress),
                alpha: 1.0,
                ..Default::default()
            },

            TransitionType::Left => Transform {
                bound_right: Some(progress),
                ..Default::default()
            },

            TransitionType::Top => Transform {
                bound_top: Some(1.0 - progress),
                ..Default::default()
            },

            TransitionType::Bottom => Transform {
                bound_bottom: Some(progress),
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
                    radius: 1.0 - progress,
                    ..Default::default()
                }
            }

            TransitionType::Any => Transform {
                bound_left: Some(self.rand - self.progress),
                bound_top: Some(self.rand - self.progress),
                bound_right: Some(self.rand + self.progress),
                bound_bottom: Some(self.rand + self.progress),
                radius: 1.0 - progress,
                ..Default::default()
            },

            TransitionType::Random => Transform::default(),

            _ => Transform::default(),
        }
    }
}
