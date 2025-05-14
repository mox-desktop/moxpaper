use std::ops::Deref;

pub struct BezierBuilder;

impl BezierBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn linear(self) -> Bezier {
        Bezier::linear()
    }

    pub fn ease(self) -> Bezier {
        Bezier::ease()
    }

    pub fn ease_in(self) -> Bezier {
        Bezier::ease_in()
    }

    pub fn ease_out(self) -> Bezier {
        Bezier::ease_out()
    }

    pub fn ease_in_out(self) -> Bezier {
        Bezier::ease_in_out()
    }

    pub fn custom(self, x1: f32, y1: f32, x2: f32, y2: f32) -> Bezier {
        Bezier::custom((x1, y1, x2, y2))
    }
}

#[derive(Debug, Clone)]
pub struct Bezier((f32, f32, f32, f32));

impl Bezier {
    fn custom(curve: (f32, f32, f32, f32)) -> Self {
        Self(curve)
    }

    fn linear() -> Self {
        Self((0.0, 0.0, 1.0, 1.0))
    }

    fn ease() -> Self {
        Self((0.25, 0.1, 0.25, 1.0))
    }

    fn ease_in() -> Self {
        Self((0.42, 0.0, 1.0, 1.0))
    }

    fn ease_out() -> Self {
        Self((0.0, 0.0, 0.58, 1.0))
    }

    fn ease_in_out() -> Self {
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
