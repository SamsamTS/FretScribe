//! Linear parameter ramp used to keep every gain change click-free.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ramp {
    pub cur: f32,
    pub target: f32,
    step: f32,
    left: u32,
}

impl Ramp {
    pub fn new(value: f32) -> Self {
        Self {
            cur: value,
            target: value,
            step: 0.0,
            left: 0,
        }
    }

    /// Linear ramp that lands exactly on `target` after `frames` calls to `next`.
    pub fn set(&mut self, target: f32, frames: f64) {
        self.target = target;
        self.left = frames.max(1.0).round() as u32;
        self.step = (f64::from(target - self.cur) / f64::from(self.left)) as f32;
    }

    pub fn snap(&mut self, value: f32) {
        *self = Self::new(value);
    }

    pub fn next(&mut self) -> f32 {
        match self.left {
            0 => {}
            1 => {
                self.left = 0;
                self.cur = self.target;
            }
            _ => {
                self.left -= 1;
                self.cur += self.step;
            }
        }
        self.cur
    }
}
