//! Beat-grid fitting: nudges measure starts onto the beats of the recording.
//!
//! 1. `onset_envelope`: log-magnitude STFT (1024 window, 512 hop, 60 Hz..5 kHz), half-wave
//!    rectified frame-to-frame difference summed over bins, minus a moving average (adaptive
//!    threshold). Frame `k` is at `k * hop_sec` in the track's own audio time.
//! 2. `combine_envelopes`: resamples every track's envelope into project time (offset), scales it to
//!    a peak of 1 and sums with weights, so helper tracks (drums, bass) stabilise the analysis.
//! 3. `fit_measures`: Viterbi over a shift candidate per measure (half-hop steps within
//!    +/- `max_shift_beats`). Emission is the weighted onset strength at the measure's beat lines
//!    (downbeat 2, beat 1, subdivision 0.5); transitions pay a quadratic penalty on the shift
//!    change and forbid shrinking a measure below half its length. Low-confidence measures keep
//!    their position.
//! 4. `estimate_tempo`: normalised autocorrelation of the envelope times a mild tempo prior, peaks refined
//!    by parabolic interpolation. Multiples of the true tempo score high too: see `octave_ratio`.

mod fit;
#[doc(hidden)]
pub mod synth;
#[cfg(test)]
mod tests;

use std::f32::consts::PI;

use fretscribe_core::{Track, TrackRole};
use rayon::prelude::*;
use realfft::RealFftPlanner;

pub use fit::{fit_measures, fitted_grid, MeasureFit};

const FFT_SIZE: usize = 1024;
pub const HOP: usize = 512;
const BAND_HZ: (f32, f32) = (60.0, 5000.0);
const LOG_GAIN: f32 = 100.0;
const THRESHOLD_WINDOW_SEC: f64 = 0.5;
const PEAK_RADIUS: usize = 3;
const PEAK_MIN_RELATIVE: f32 = 0.1;
const MAX_TEMPO_CANDIDATES: usize = 3;
const MIN_TEMPO_SEPARATION: f64 = 0.03;
const OCTAVE_TOLERANCE: f64 = 0.04;
/// Mild preference among octave-related candidates: log-Gaussian around `PRIOR_BPM`.
const PRIOR_BPM: f64 = 120.0;
const PRIOR_OCTAVE_SIGMA: f64 = 1.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnsetEnvelope {
    pub values: Vec<f32>,
    pub hop_sec: f64,
}

impl OnsetEnvelope {
    /// Linear interpolation; zero outside the envelope.
    pub fn at(&self, sec: f64) -> f32 {
        let pos = sec / self.hop_sec;
        if pos.is_nan() || pos < 0.0 || self.hop_sec <= 0.0 {
            return 0.0;
        }
        let i = pos.floor() as usize;
        let frac = (pos - pos.floor()) as f32;
        match (self.values.get(i), self.values.get(i + 1)) {
            (Some(&a), Some(&b)) => a + (b - a) * frac,
            (Some(&a), None) if frac == 0.0 => a,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnvelopeInput {
    pub env: OnsetEnvelope,
    pub offset_sec: f64,
    pub weight: f32,
}

pub fn default_weight(role: TrackRole) -> f32 {
    match role {
        TrackRole::Drums => 1.0,
        TrackRole::Bass => 0.7,
        TrackRole::Guitar | TrackRole::Other => 0.5,
    }
}

/// `None` when the track is excluded through `use_for_beats`.
pub fn envelope_input(track: &Track, env: OnsetEnvelope) -> Option<EnvelopeInput> {
    track.use_for_beats.then(|| EnvelopeInput {
        env,
        offset_sec: track.offset_sec,
        weight: default_weight(track.role),
    })
}

pub fn onset_envelope(mono: &[f32], sample_rate: u32) -> OnsetEnvelope {
    if mono.is_empty() || sample_rate == 0 {
        return OnsetEnvelope::default();
    }
    let hop_sec = HOP as f64 / f64::from(sample_rate);
    let frames = mono.len().div_ceil(HOP);
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_SIZE as f32).cos())
        .collect();
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let top = FFT_SIZE / 2;
    let hi = ((BAND_HZ.1 / bin_hz) as usize).min(top);
    let lo = ((BAND_HZ.0 / bin_hz).ceil() as usize).min(hi);
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);

    let spectra: Vec<Vec<f32>> = (0..frames)
        .into_par_iter()
        .map_init(
            || (fft.make_input_vec(), fft.make_output_vec()),
            |(input, output), k| {
                let first = (k * HOP) as isize - (FFT_SIZE / 2) as isize;
                for (i, slot) in input.iter_mut().enumerate() {
                    let sample = usize::try_from(first + i as isize)
                        .ok()
                        .and_then(|j| mono.get(j))
                        .copied()
                        .unwrap_or(0.0);
                    *slot = sample * window[i];
                }
                if fft.process(input, output).is_err() {
                    output.fill(Default::default());
                }
                output[lo..=hi]
                    .iter()
                    .map(|c| (1.0 + LOG_GAIN * c.norm() / FFT_SIZE as f32).ln())
                    .collect()
            },
        )
        .collect();

    let flux: Vec<f32> = (0..frames)
        .map(|k| {
            if k == 0 {
                return 0.0;
            }
            spectra[k]
                .iter()
                .zip(&spectra[k - 1])
                .map(|(cur, prev)| (cur - prev).max(0.0))
                .sum()
        })
        .collect();
    OnsetEnvelope {
        values: subtract_moving_average(&flux, (THRESHOLD_WINDOW_SEC / hop_sec / 2.0) as usize),
        hop_sec,
    }
}

fn subtract_moving_average(values: &[f32], radius: usize) -> Vec<f32> {
    let mut prefix = vec![0.0_f64];
    prefix.extend(values.iter().scan(0.0, |acc, &v| {
        *acc += f64::from(v);
        Some(*acc)
    }));
    (0..values.len())
        .map(|k| {
            let (a, b) = (k.saturating_sub(radius), (k + radius + 1).min(values.len()));
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (f64::from(values[k]) - mean).max(0.0) as f32
        })
        .collect()
}

/// Local maxima over `PEAK_RADIUS` frames that reach `PEAK_MIN_RELATIVE` of the global maximum.
pub fn detect_onsets(env: &OnsetEnvelope) -> Vec<f64> {
    let v = &env.values;
    let max = v.iter().copied().fold(0.0, f32::max);
    (0..v.len())
        .filter(|&k| {
            let (a, b) = (k.saturating_sub(PEAK_RADIUS), (k + PEAK_RADIUS + 1).min(v.len()));
            v[k] > 0.0
                && v[k] >= PEAK_MIN_RELATIVE * max
                && v[a..k].iter().all(|&o| o < v[k])
                && v[k + 1..b].iter().all(|&o| o <= v[k])
        })
        .map(|k| k as f64 * env.hop_sec)
        .collect()
}

/// Output hop is the finest input hop; frames before project time 0 are dropped.
pub fn combine_envelopes(inputs: &[EnvelopeInput]) -> OnsetEnvelope {
    let hops = inputs.iter().map(|i| i.env.hop_sec).filter(|&h| h > 0.0);
    let Some(hop_sec) = hops.min_by(f64::total_cmp) else {
        return OnsetEnvelope::default();
    };
    let end = inputs
        .iter()
        .filter(|i| !i.env.values.is_empty())
        .map(|i| (i.env.values.len() - 1) as f64 * i.env.hop_sec + i.offset_sec)
        .fold(f64::NEG_INFINITY, f64::max);
    if !end.is_finite() || end < 0.0 {
        return OnsetEnvelope { values: Vec::new(), hop_sec };
    }
    let mut values = vec![0.0_f32; (end / hop_sec).floor() as usize + 1];
    for input in inputs {
        let peak = input.env.values.iter().copied().fold(0.0, f32::max);
        if peak <= 0.0 || input.env.hop_sec <= 0.0 {
            continue;
        }
        let scale = input.weight / peak;
        for (j, out) in values.iter_mut().enumerate() {
            *out += scale * input.env.at(j as f64 * hop_sec - input.offset_sec);
        }
    }
    OnsetEnvelope { values, hop_sec }
}

/// Up to three `(bpm, score)` candidates, best first; the score is the normalised autocorrelation weighted by the tempo prior.
pub fn estimate_tempo(env: &OnsetEnvelope, range_bpm: (f64, f64)) -> Vec<(f64, f64)> {
    let (lo, hi) = range_bpm;
    let n = env.values.len();
    if n < 8 || env.hop_sec <= 0.0 || !(lo > 0.0 && hi >= lo) {
        return Vec::new();
    }
    let lag_of = |bpm: f64| 60.0 / (bpm * env.hop_sec);
    let min_lag = (lag_of(hi).floor() as usize).max(2);
    let max_lag = (lag_of(lo).ceil() as usize).min(n - 2);
    if min_lag >= max_lag {
        return Vec::new();
    }
    let mean = env.values.iter().sum::<f32>() / n as f32;
    let x: Vec<f32> = env.values.iter().map(|v| v - mean).collect();
    let energy = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
    if energy <= 0.0 {
        return Vec::new();
    }
    let acf: Vec<f64> = (min_lag - 1..=max_lag + 1)
        .map(|lag| {
            let sum: f32 = x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
            f64::from(sum / (n - lag) as f32 / energy)
        })
        .collect();

    let mut peaks: Vec<(f64, f64)> = (1..acf.len() - 1)
        .filter(|&i| acf[i] > 0.0 && acf[i] > acf[i - 1] && acf[i] >= acf[i + 1])
        .map(|i| {
            let curvature = acf[i - 1] - 2.0 * acf[i] + acf[i + 1];
            let shift = if curvature < 0.0 { 0.5 * (acf[i - 1] - acf[i + 1]) / curvature } else { 0.0 };
            let lag = (min_lag - 1 + i) as f64 + shift;
            let bpm = 60.0 / (lag * env.hop_sec);
            let octaves = (bpm / PRIOR_BPM).log2() / PRIOR_OCTAVE_SIGMA;
            (bpm, acf[i] * (-0.5 * octaves * octaves).exp())
        })
        .collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut chosen: Vec<(f64, f64)> = Vec::new();
    for peak in peaks {
        if chosen.iter().all(|c| (c.0 / peak.0 - 1.0).abs() > MIN_TEMPO_SEPARATION) {
            chosen.push(peak);
        }
        if chosen.len() == MAX_TEMPO_CANDIDATES {
            break;
        }
    }
    chosen
}

/// `Some(2.0)` / `Some(0.5)` when `b` is double / half of `a`: such candidates are ambiguous.
pub fn octave_ratio(a: f64, b: f64) -> Option<f64> {
    [2.0, 0.5]
        .into_iter()
        .find(|r| (b / (a * r) - 1.0).abs() < OCTAVE_TOLERANCE)
}
