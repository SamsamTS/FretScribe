//! Display-only parameters applied at render time; changing them never recomputes the spectrogram.

use crate::spectrogram::SpectrogramParams;

pub const MIN_LEVELS_GAP: f32 = 0.02;
pub const GAMMA_MIN: f32 = 0.2;
pub const GAMMA_MAX: f32 = 5.0;
pub const GAMMA_DEFAULT: f32 = 1.0;
/// Starting display values for the application (tuned by eye), distinct from the identity defaults.
pub const APP_GAMMA: f32 = 0.25;
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
        let default = Self::default();
        let black = if self.black.is_nan() {
            default.black
        } else {
            self.black.clamp(0.0, 1.0)
        };
        let mut white = if self.white.is_nan() {
            default.white
        } else {
            self.white.clamp(0.0, 1.0)
        };
        if white - black >= MIN_LEVELS_GAP {
            return Self { black, white };
        }
        white = black + MIN_LEVELS_GAP;
        if white > 1.0 {
            return Self {
                black: 1.0 - MIN_LEVELS_GAP,
                white: 1.0,
            };
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_levels_are_identity() {
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            assert!((apply_levels(v, Levels::default()) - v).abs() < 1e-6);
        }
    }

    #[test]
    fn levels_cut_and_saturate() {
        let l = Levels::new(0.3, 0.8);
        assert_eq!(apply_levels(0.0, l), 0.0);
        assert_eq!(apply_levels(0.3, l), 0.0);
        assert_eq!(apply_levels(0.8, l), 1.0);
        assert_eq!(apply_levels(1.0, l), 1.0);
        assert!((apply_levels(0.55, l) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn clamped_levels_are_always_valid() {
        let cases = [
            (-1.0, 2.0),
            (f32::NAN, f32::NAN),
            (0.5, 0.5),
            (0.9, 0.1),
            (1.0, 1.0),
            (0.995, 1.0),
            (f32::INFINITY, f32::NEG_INFINITY),
            (0.0, 0.0),
        ];
        for (b, w) in cases {
            let l = Levels::new(b, w);
            assert!(
                (0.0..=1.0).contains(&l.black) && (0.0..=1.0).contains(&l.white),
                "{b} {w}"
            );
            assert!(
                l.white - l.black >= MIN_LEVELS_GAP - 1e-6,
                "{b} {w} -> {l:?}"
            );
        }
        assert_eq!(Levels::new(f32::NAN, f32::NAN), Levels::default());
        assert_eq!(Levels::new(1.0, 1.0).white, 1.0);
        assert_eq!(Levels::new(0.5, 0.2).clamped(), Levels::new(0.5, 0.2));
    }

    #[test]
    fn levels_to_db() {
        let params = SpectrogramParams::default();
        assert_eq!(
            Levels::default().to_db(&params),
            (params.db_floor, params.db_ceiling)
        );
        let (b, w) = Levels::new(0.5, 1.0).to_db(&params);
        assert!((b - (-45.0)).abs() < 1e-4 && (w - 0.0).abs() < 1e-4);
    }

    #[test]
    fn gamma_properties() {
        for i in 0..=20 {
            let v = i as f32 / 20.0;
            assert!((apply_gamma(v, 1.0) - v).abs() < 1e-6);
        }
        for g in [0.2, 0.5, 1.0, 2.0, 5.0, 100.0, -3.0] {
            assert_eq!(apply_gamma(0.0, g), 0.0);
            assert_eq!(apply_gamma(1.0, g), 1.0);
        }
        for v in [0.1, 0.4, 0.5, 0.9] {
            assert!(apply_gamma(v, 0.5) < v);
            assert!(apply_gamma(v, 2.0) > v);
        }
        for g in [0.3, 1.0, 3.0] {
            let mut last = -1.0;
            for i in 0..=100 {
                let y = apply_gamma(i as f32 / 100.0, g);
                assert!(y >= last);
                last = y;
            }
        }
        assert_eq!(apply_gamma(0.5, 100.0), apply_gamma(0.5, GAMMA_MAX));
        assert_eq!(apply_gamma(0.5, 0.001), apply_gamma(0.5, GAMMA_MIN));
        assert_eq!(apply_gamma(0.5, f32::NAN), 0.5);
    }
}
