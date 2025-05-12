use crate::Moxpaper;
use calloop::{
    timer::{TimeoutAction, Timer},
    LoopHandle,
};
use common::{image_data::ImageData, ipc::TransitionType};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Animation {
    pub transition_type: TransitionType,
    duration: Duration,
    start_time: Option<Instant>,
    is_active: bool,
    progress: f32,
    previous_image: Option<ImageData>,
    pub target_image: Option<ImageData>,
    handle: LoopHandle<'static, Moxpaper>,
}

impl Animation {
    pub fn new(
        handle: LoopHandle<'static, Moxpaper>,
        transition_type: TransitionType,
        duration_ms: u64,
    ) -> Self {
        Self {
            handle,
            transition_type,
            duration: Duration::from_millis(duration_ms),
            start_time: None,
            is_active: false,
            progress: 0.0,
            previous_image: None,
            target_image: None,
        }
    }

    pub fn start(
        &mut self,
        target_image: ImageData,
        output_name: &str,
        transition_type: TransitionType,
    ) {
        self.progress = 0.0;
        self.previous_image = self.target_image.take();
        self.start_time = None;
        self.target_image = Some(target_image);
        self.is_active = true;
        self.transition_type = transition_type;

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
                if !output.animation.is_active() {
                    return TimeoutAction::Drop;
                }

                if output.animation.start_time.is_none() {
                    output.animation.start_time = Some(Instant::now());
                }

                output.render();

                TimeoutAction::ToDuration(Duration::from_millis(16))
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

        if start_time.elapsed() >= self.duration {
            self.progress = 1.0;
            self.is_active = false;
            self.previous_image = None;
            return true;
        }

        self.progress = start_time.elapsed().as_secs_f32() / self.duration.as_secs_f32();
        false
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn calculate_transform(&self) -> (f32, f32, f32, f32) {
        let progress = self.progress;
        match self.transition_type {
            TransitionType::None => (0., 0., 1., 1.),
            TransitionType::Fade => (0.0, 0.0, 1.0, progress),
            TransitionType::Right => ((1.0 - progress), 0.0, 1.0, 1.0),
            //TransitionType::Left => ((progress - 1.0), 0.0, 1.0, 1.0),
            //TransitionType::Top => (0.0, (1.0 - progress), 1.0, 1.0),
            //TransitionType::Bottom => (0.0, (progress - 1.0), 1.0, 1.0),
            _ => (0., 0., 1., 1.),
        }
    }
}
