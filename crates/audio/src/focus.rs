//! "Guitar focus": a listening aid that thins out everything outside the guitar's range.
//!
//! The band is the span of fundamentals the project's guitar can play. Below the low corner and
//! above the high corner the mix is attenuated by up to `MAX_DEPTH_DB`, in the passband it is
//! untouched. Each side is a 4th-order Butterworth filter blended with the dry signal
//! (`y = x + amount * (filtered - x)`), which gives a flat, steep-edged cut whose depth is set by
//! `amount` alone, so strength changes are plain gain ramps that never disturb the filter state.
//! Shelving filters were measured and rejected: at the same corner they cost the lowest open
//! string 4.8 dB.

use std::f64::consts::TAU;

use fretscribe_core::{midi_to_freq, PitchRef, Tuning};

use crate::ramp::Ramp;

use fretscribe_core::MAX_FRET;
const LOW_CORNER_RATIO: f64 = 0.6;
const HIGH_CORNER_RATIO: f64 = 4.0;
const MAX_DEPTH_DB: f64 = 24.0;
const MIN_CORNER_HZ: f64 = 5.0;
const MAX_CORNER_RATE_RATIO: f64 = 0.45;
const BUTTERWORTH_4TH_Q: [f64; 2] = [0.541_196_100_146_197, 1.306_562_964_876_376_5];
const RETUNE_LOG_THRESHOLD: f64 = 0.03;
const FADE_SEC: f64 = 0.010;

/// Range of fundamentals the guitar can play, from the lowest open string to the highest string
/// at the 24th fret.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusBand {
    pub low_hz: f32,
    pub high_hz: f32,
}

impl Default for FocusBand {
    fn default() -> Self {
        focus_band(&Tuning::standard(), 0, &PitchRef::default())
    }
}

/// An empty tuning falls back to standard tuning. Notes are capped at MIDI 127.
pub fn focus_band(tuning: &Tuning, capo: u8, pitch_ref: &PitchRef) -> FocusBand {
    let standard = Tuning::standard();
    let strings = if tuning.open_strings.is_empty() {
        &standard.open_strings
    } else {
        &tuning.open_strings
    };
    let lowest = f32::from(strings.iter().min().copied().unwrap_or(40)) + f32::from(capo);
    let highest = f32::from(strings.iter().max().copied().unwrap_or(64)) + f32::from(capo) + f32::from(MAX_FRET);
    FocusBand {
        low_hz: midi_to_freq(lowest.min(127.0), pitch_ref),
        high_hz: midi_to_freq(highest.min(127.0), pitch_ref),
    }
}

/// Blend amount for a strength in 0..=1: 0 is bypass, 1 cuts `MAX_DEPTH_DB` outside the band.
pub(crate) fn blend_amount(strength: f32) -> f32 {
    let depth_db = MAX_DEPTH_DB * f64::from(strength.clamp(0.0, 1.0));
    (1.0 - 10f64.powf(-depth_db / 20.0)) as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Corners {
    pub low: f64,
    pub high: f64,
}

/// `scale` follows the playback speed when the pitch follows it too (ChangePitch).
pub(crate) fn corners(band: &FocusBand, rate: f64, scale: f64) -> Corners {
    let high = (f64::from(band.high_hz) * HIGH_CORNER_RATIO * scale)
        .clamp(4.0 * MIN_CORNER_HZ, MAX_CORNER_RATE_RATIO * rate);
    let low = (f64::from(band.low_hz) * LOW_CORNER_RATIO * scale).clamp(MIN_CORNER_HZ, high / 4.0);
    Corners { low, high }
}

fn differs(a: &Corners, b: &Corners) -> bool {
    (a.low / b.low).ln().abs() > RETUNE_LOG_THRESHOLD || (a.high / b.high).ln().abs() > RETUNE_LOG_THRESHOLD
}

#[derive(Clone, Copy)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    fn design(high_pass: bool, freq: f64, q: f64, rate: f64) -> Self {
        let (sin, cos) = (TAU * freq / rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2) = if high_pass {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        let a0 = 1.0 + alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// Transposed direct form II with f64 state: the low corner is about 50 Hz at 48 kHz, where f32
/// state would be noisy.
#[derive(Clone, Copy, Default)]
struct Section {
    z1: [f64; 2],
    z2: [f64; 2],
}

impl Section {
    fn run(&mut self, c: &Coeffs, channel: usize, x: f64) -> f64 {
        let y = c.b0 * x + self.z1[channel];
        self.z1[channel] = c.b1 * x - c.a1 * y + self.z2[channel];
        self.z2[channel] = c.b2 * x - c.a2 * y;
        y
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Chain {
    corners: Corners,
    high_pass: [Coeffs; 2],
    low_pass: [Coeffs; 2],
    high_state: [Section; 2],
    low_state: [Section; 2],
}

impl Chain {
    pub fn new(corners: Corners, rate: f64) -> Self {
        let design = |high_pass: bool, freq: f64| {
            [
                Coeffs::design(high_pass, freq, BUTTERWORTH_4TH_Q[0], rate),
                Coeffs::design(high_pass, freq, BUTTERWORTH_4TH_Q[1], rate),
            ]
        };
        Self {
            corners,
            high_pass: design(true, corners.low),
            low_pass: design(false, corners.high),
            high_state: [Section::default(); 2],
            low_state: [Section::default(); 2],
        }
    }

    fn sample(&mut self, amount: f64, channel: usize, x: f64) -> f64 {
        let mut high = x;
        for (c, s) in self.high_pass.iter().zip(&mut self.high_state) {
            high = s.run(c, channel, high);
        }
        let y = x + amount * (high - x);
        let mut low = y;
        for (c, s) in self.low_pass.iter().zip(&mut self.low_state) {
            low = s.run(c, channel, low);
        }
        y + amount * (low - y)
    }

    fn frame(&mut self, amount: f64, input: [f64; 2]) -> [f64; 2] {
        [self.sample(amount, 0, input[0]), self.sample(amount, 1, input[1])]
    }
}

/// The realtime filter on the master bus. While it is inactive `process` returns immediately and
/// the buffer is left bit-exact. Strength and on/off are ramps of the blend amount; a retune
/// (tuning, capo, reference pitch, or speed in ChangePitch) crossfades to a fresh chain, and the
/// latest request wins if several arrive during a fade.
pub(crate) struct FocusFilter {
    rate: f64,
    fade_frames: u32,
    band: FocusBand,
    speed_scale: f64,
    enabled: bool,
    strength: f32,
    amount: Ramp,
    current: Option<Chain>,
    incoming: Option<Chain>,
    fade_left: u32,
    pending: Option<Corners>,
}

impl FocusFilter {
    pub fn new(sample_rate: u32, band: FocusBand) -> Self {
        let rate = f64::from(sample_rate);
        Self {
            rate,
            fade_frames: (FADE_SEC * rate).round().max(1.0) as u32,
            band,
            speed_scale: 1.0,
            enabled: false,
            strength: 0.5,
            amount: Ramp::new(0.0),
            current: None,
            incoming: None,
            fade_left: 0,
            pending: None,
        }
    }

    fn desired(&self) -> Corners {
        corners(&self.band, self.rate, self.speed_scale)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.retarget();
    }

    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength.clamp(0.0, 1.0);
        self.retarget();
    }

    pub fn set_band(&mut self, band: FocusBand) {
        self.band = band;
        self.retune();
    }

    pub fn set_speed_scale(&mut self, scale: f64) {
        self.speed_scale = scale;
        self.retune();
    }

    fn retarget(&mut self) {
        let target = if self.enabled { blend_amount(self.strength) } else { 0.0 };
        if (target - self.amount.target).abs() > f32::EPSILON {
            self.amount.set(target, f64::from(self.fade_frames));
        }
        if target > 0.0 && self.current.is_none() {
            self.current = Some(Chain::new(self.desired(), self.rate));
        }
    }

    fn retune(&mut self) {
        let desired = self.desired();
        let Some(current) = &self.current else {
            return;
        };
        match &self.incoming {
            None if differs(&desired, &current.corners) => {
                self.incoming = Some(Chain::new(desired, self.rate));
                self.fade_left = self.fade_frames;
            }
            Some(incoming) => {
                self.pending = differs(&desired, &incoming.corners).then_some(desired);
            }
            None => {}
        }
    }

    /// Playback starting or stopping: start the filter from silence rather than stale ringing.
    pub fn reset_state(&mut self) {
        self.incoming = None;
        self.pending = None;
        self.fade_left = 0;
        self.current = (self.amount.cur > 0.0 || self.amount.target > 0.0)
            .then(|| Chain::new(self.desired(), self.rate));
    }

    pub fn process(&mut self, buffer: &mut [f32]) {
        for frame in buffer.chunks_exact_mut(2) {
            let Some(current) = self.current.as_mut() else {
                return;
            };
            let amount = f64::from(self.amount.next());
            let input = [f64::from(frame[0]), f64::from(frame[1])];
            let mut out = current.frame(amount, input);
            if let Some(incoming) = self.incoming.as_mut() {
                let weight = 1.0 - f64::from(self.fade_left) / f64::from(self.fade_frames);
                let other = incoming.frame(amount, input);
                for (o, n) in out.iter_mut().zip(other) {
                    *o += weight * (n - *o);
                }
                self.fade_left -= 1;
                if self.fade_left == 0 {
                    self.current = self.incoming.take();
                    if let Some(next) = self.pending.take() {
                        self.incoming = Some(Chain::new(next, self.rate));
                        self.fade_left = self.fade_frames;
                    }
                }
            }
            frame[0] = out[0] as f32;
            frame[1] = out[1] as f32;
            if self.amount.cur == 0.0 && self.amount.target == 0.0 && self.incoming.is_none() {
                self.current = None;
            }
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/focus.rs"]
mod tests;
