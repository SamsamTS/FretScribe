//! Display-only parameters applied at render time; changing them never recomputes the spectrogram.

use crate::spectrogram::SpectrogramParams;

use fretscribe_core::persist::{clamp_levels, GAMMA_RANGE, LEVELS_MIN_GAP};

pub const MIN_LEVELS_GAP: f32 = LEVELS_MIN_GAP;
pub const GAMMA_MIN: f32 = *GAMMA_RANGE.start();
pub const GAMMA_MAX: f32 = *GAMMA_RANGE.end();
pub const GAMMA_DEFAULT: f32 = 1.0;
/// Starting display values for the application (tuned by eye), distinct from the identity defaults.
pub const APP_GAMMA: f32 = 0.8;
pub const APP_WHITE: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub black: f32,
    pub white: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            black: 0.0,
            white: 1.0,
        }
    }
}

impl Levels {
    pub fn new(black: f32, white: f32) -> Self {
        Self { black, white }.clamped()
    }

    pub fn clamped(self) -> Self {
        let (black, white) = clamp_levels(self.black, self.white);
        Self { black, white }
    }

    pub fn to_db(self, params: &SpectrogramParams) -> (f32, f32) {
        let range = params.db_ceiling - params.db_floor;
        (
            params.db_floor + self.black * range,
            params.db_floor + self.white * range,
        )
    }
}

pub fn apply_levels(v: f32, levels: Levels) -> f32 {
    if v.is_nan() {
        return 0.0;
    }
    let span = (levels.white - levels.black).max(f32::EPSILON);
    ((v - levels.black) / span).clamp(0.0, 1.0)
}

pub fn clamp_gamma(gamma: f32) -> f32 {
    if gamma.is_nan() {
        GAMMA_DEFAULT
    } else {
        gamma.clamp(GAMMA_MIN, GAMMA_MAX)
    }
}

pub fn apply_gamma(v: f32, gamma: f32) -> f32 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    v.powf(1.0 / clamp_gamma(gamma))
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/analysis/display.rs"]
mod tests;
