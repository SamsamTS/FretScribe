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

const MAX_FRET: u8 = 24;
const LOW_CORNER_RATIO: f64 = 0.6;
const HIGH_CORNER_RATIO: f64 = 4.0;
const MAX_DEPTH_DB: f64 = 24.0;
const MIN_CORNER_HZ: f64 = 5.0;
const MAX_CORNER_RATE_RATIO: f64 = 0.45;
const BUTTERWORTH_4TH_Q: [f64; 2] = [0.541_196_100_146_197, 1.306_562_964_876_376_5];
const RETUNE_LOG_THRESHOLD: f64 = 0.03;
const FADE_SEC: f64 = 0.010;
const OFFLINE_PAD_SEC: f64 = 0.1;

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

/// Zero-phase version for analysis: forward-backward filtering leaves onset times where they are,
/// and squares the response, so half the depth is used to match the realtime cut.
pub fn apply_focus_offline(mono: &[f32], rate: u32, band: &FocusBand, strength: f32) -> Vec<f32> {
    if mono.len() < 2 || rate == 0 || strength.is_nan() || strength <= 0.0 {
        return mono.to_vec();
    }
    let amount = f64::from(blend_amount(strength.min(1.0) * 0.5));
    let pad = ((OFFLINE_PAD_SEC * f64::from(rate)) as usize).min(mono.len() - 1);
    let (first, last) = (f64::from(mono[0]), f64::from(mono[mono.len() - 1]));

    let mut signal: Vec<f64> = Vec::with_capacity(mono.len() + 2 * pad);
    signal.extend((1..=pad).rev().map(|k| 2.0 * first - f64::from(mono[k])));
    signal.extend(mono.iter().map(|&s| f64::from(s)));
    signal.extend((1..=pad).map(|k| 2.0 * last - f64::from(mono[mono.len() - 1 - k])));

    let design = corners(band, f64::from(rate), 1.0);
    for _ in 0..2 {
        let mut chain = Chain::new(design, f64::from(rate));
        for x in &mut signal {
            *x = chain.sample(amount, 0, *x);
        }
        signal.reverse();
    }
    signal[pad..pad + mono.len()].iter().map(|&x| x as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning(open: &[u8]) -> Tuning {
        Tuning {
            name: "test".to_owned(),
            open_strings: open.to_vec(),
        }
    }

    fn reference(hz: f32) -> PitchRef {
        PitchRef::new(hz).unwrap()
    }

    fn close(a: f32, b: f32, tolerance: f32) {
        assert!((a - b).abs() <= tolerance, "{a} vs {b}");
    }

    #[test]
    fn standard_tuning_at_440() {
        let band = focus_band(&Tuning::standard(), 0, &PitchRef::default());
        close(band.low_hz, 82.41, 0.01);
        close(band.high_hz, 1318.51, 0.05);
        assert_eq!(band, FocusBand::default());
    }

    #[test]
    fn alternate_tunings_and_string_counts() {
        let drop_d = focus_band(&tuning(&[38, 45, 50, 55, 59, 64]), 0, &PitchRef::default());
        close(drop_d.low_hz, 73.42, 0.01);
        close(drop_d.high_hz, 1318.51, 0.05);
        let seven = focus_band(&tuning(&[35, 40, 45, 50, 55, 59, 64]), 0, &PitchRef::default());
        close(seven.low_hz, 61.74, 0.01);
        let empty = focus_band(&tuning(&[]), 0, &PitchRef::default());
        assert_eq!(empty, FocusBand::default());
    }

    #[test]
    fn capo_shifts_both_edges_up() {
        let plain = focus_band(&Tuning::standard(), 0, &PitchRef::default());
        let capoed = focus_band(&Tuning::standard(), 2, &PitchRef::default());
        let two_semitones = 2f32.powf(2.0 / 12.0);
        close(capoed.low_hz / plain.low_hz, two_semitones, 1e-4);
        close(capoed.high_hz / plain.high_hz, two_semitones, 1e-4);
    }

    #[test]
    fn reference_pitch_scales_every_edge() {
        let plain = focus_band(&Tuning::standard(), 0, &PitchRef::default());
        let tuned = focus_band(&Tuning::standard(), 0, &reference(432.0));
        close(tuned.low_hz / plain.low_hz, 432.0 / 440.0, 1e-4);
        close(tuned.high_hz / plain.high_hz, 432.0 / 440.0, 1e-4);
    }

    #[test]
    fn absurd_capo_stays_finite() {
        let band = focus_band(&Tuning::standard(), 255, &PitchRef::default());
        assert!(band.low_hz.is_finite() && band.high_hz.is_finite());
    }

    #[test]
    fn strength_maps_to_a_monotonic_blend() {
        assert_eq!(blend_amount(0.0), 0.0);
        assert_eq!(blend_amount(-3.0), 0.0);
        let full = blend_amount(1.0);
        assert!((f64::from(1.0 - full) - 10f64.powf(-24.0 / 20.0)).abs() < 1e-6);
        assert_eq!(blend_amount(7.0), full);
        let steps: Vec<f32> = (0..=10).map(|i| blend_amount(i as f32 / 10.0)).collect();
        assert!(steps.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn corners_follow_the_band_and_stay_below_nyquist() {
        let band = FocusBand::default();
        let c = corners(&band, 48_000.0, 1.0);
        assert!((c.low - 0.6 * f64::from(band.low_hz)).abs() < 1e-6);
        assert!((c.high - 4.0 * f64::from(band.high_hz)).abs() < 1e-6);
        let slow = corners(&band, 48_000.0, 0.5);
        assert!((slow.low / c.low - 0.5).abs() < 1e-9 && (slow.high / c.high - 0.5).abs() < 1e-9);
        let low_rate = corners(&band, 8_000.0, 1.0);
        assert!(low_rate.high <= 0.45 * 8_000.0 && low_rate.low <= low_rate.high / 4.0);
        let extreme = corners(&band, 48_000.0, 0.25);
        assert!(extreme.low >= MIN_CORNER_HZ);
    }

    #[test]
    fn inactive_filter_leaves_the_buffer_untouched() {
        let mut filter = FocusFilter::new(48_000, FocusBand::default());
        let original: Vec<f32> = (0..256).map(|i| (i as f32 * 0.37).sin()).collect();
        let mut buffer = original.clone();
        filter.process(&mut buffer);
        assert_eq!(buffer, original);

        filter.set_enabled(true);
        filter.set_strength(0.0);
        filter.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn enabling_then_disabling_returns_to_a_bit_exact_bypass() {
        let mut filter = FocusFilter::new(48_000, FocusBand::default());
        let original: Vec<f32> = (0..2 * 4_800).map(|i| (i as f32 * 0.013).sin() * 0.3).collect();
        filter.set_enabled(true);
        let mut buffer = original.clone();
        filter.process(&mut buffer);
        assert_ne!(buffer, original);
        filter.set_enabled(false);
        let mut settle = original.clone();
        filter.process(&mut settle);
        let mut after = original.clone();
        filter.process(&mut after);
        assert_eq!(after, original);
        assert!(filter.current.is_none());
    }

    #[test]
    fn retunes_during_a_fade_keep_only_the_latest_request() {
        let mut filter = FocusFilter::new(48_000, FocusBand::default());
        filter.set_enabled(true);
        filter.set_speed_scale(0.5);
        assert!(filter.incoming.is_some());
        filter.set_speed_scale(0.8);
        filter.set_speed_scale(1.5);
        assert_eq!(filter.pending, Some(corners(&FocusBand::default(), 48_000.0, 1.5)));
        let mut buffer = vec![0.1; 2 * 4_800];
        filter.process(&mut buffer);
        let settled = filter.current.as_ref().map(|c| c.corners);
        assert_eq!(settled, Some(corners(&FocusBand::default(), 48_000.0, 1.5)));
        assert!(filter.incoming.is_none() && filter.pending.is_none());
    }

    #[test]
    fn tiny_retunes_are_ignored() {
        let mut filter = FocusFilter::new(48_000, FocusBand::default());
        filter.set_enabled(true);
        filter.set_speed_scale(1.01);
        assert!(filter.incoming.is_none());
    }

    fn sine(freq: f64, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| (TAU * freq * i as f64 / f64::from(rate)).sin() as f32).collect()
    }

    fn amplitude(x: &[f32], freq: f64, rate: u32) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in x.iter().enumerate() {
            let phase = TAU * freq * i as f64 / f64::from(rate);
            re += f64::from(s) * phase.cos();
            im += f64::from(s) * phase.sin();
        }
        2.0 * re.hypot(im) / x.len() as f64
    }

    #[test]
    fn offline_filter_keeps_onsets_in_place() {
        let mut impulse = vec![0.0f32; 30_000];
        impulse[12_345] = 1.0;
        let out = apply_focus_offline(&impulse, 48_000, &FocusBand::default(), 1.0);
        let peak = (0..out.len()).max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs())).unwrap();
        assert!((peak as i64 - 12_345).abs() <= 1, "{peak}");
        assert_eq!(out.len(), impulse.len());
    }

    #[test]
    fn offline_filter_cuts_out_of_band_and_keeps_the_band() {
        let band = FocusBand::default();
        let rate = 48_000;
        let window = 8_000..40_000;
        let through = |freq: f64| {
            let x = sine(freq, rate, 48_000);
            let y = apply_focus_offline(&x, rate, &band, 1.0);
            20.0 * (amplitude(&y[window.clone()], freq, rate) / amplitude(&x[window.clone()], freq, rate)).log10()
        };
        assert!(through(440.0).abs() < 0.5);
        assert!(through(30.0) < -15.0, "{}", through(30.0));
        assert!(through(11_000.0) < -15.0, "{}", through(11_000.0));
    }

    #[test]
    fn offline_filter_without_strength_is_a_copy() {
        let x = sine(100.0, 48_000, 1_000);
        assert_eq!(apply_focus_offline(&x, 48_000, &FocusBand::default(), 0.0), x);
        assert_eq!(apply_focus_offline(&x, 48_000, &FocusBand::default(), f32::NAN), x);
        assert!(apply_focus_offline(&[], 48_000, &FocusBand::default(), 1.0).is_empty());
        let tiny = apply_focus_offline(&[0.5, -0.5, 0.25], 48_000, &FocusBand::default(), 1.0);
        assert_eq!(tiny.len(), 3);
        assert!(tiny.iter().all(|s| s.is_finite()));
    }
}
