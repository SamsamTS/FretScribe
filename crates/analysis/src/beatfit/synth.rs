//! Synthetic material for the tests and the `beatfit_bench` example.

use std::f64::consts::TAU;

use fretscribe_core::{generate_measures, BeatKind, Measure, TempoMap, TimeSignature};

use super::OnsetEnvelope;

pub const SAMPLE_RATE: u32 = 44_100;
pub const LEAD_SEC: f64 = 0.03;

pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn signed(&mut self, amplitude: f64) -> f64 {
        (self.unit() * 2.0 - 1.0) * amplitude
    }
}

/// True beat times: steady `bpm` plus a slow sinusoidal drift and per-beat jitter.
pub fn drifting_beats(count: usize, bpm: f64, drift_sec: f64, jitter_sec: f64, seed: u64) -> Vec<f64> {
    let mut rng = Lcg::new(seed);
    (0..count)
        .map(|k| {
            let drift = drift_sec * (TAU * k as f64 / 24.0).sin();
            LEAD_SEC + k as f64 * 60.0 / bpm + drift + rng.signed(jitter_sec)
        })
        .collect()
}

/// Decaying sine hits `(time, frequency)`.
pub fn render_hits(hits: &[(f64, f64)], len_sec: f64) -> Vec<f32> {
    let rate = f64::from(SAMPLE_RATE);
    let mut out = vec![0.0_f32; (len_sec * rate) as usize];
    for &(time, freq) in hits {
        let first = (time * rate) as usize;
        for (i, slot) in out.iter_mut().enumerate().skip(first).take((0.12 * rate) as usize) {
            let t = (i - first) as f64 / rate;
            *slot += ((TAU * freq * t).sin() * (-t / 0.02).exp()) as f32;
        }
    }
    out
}

/// Kick on beats 1 and 3, snare on 2 and 4 (4/4).
pub fn drum_hits(beats: &[f64]) -> Vec<(f64, f64)> {
    beats
        .iter()
        .enumerate()
        .map(|(k, &t)| (t, if k % 2 == 0 { 90.0 } else { 1800.0 }))
        .collect()
}

/// Mostly noise, a quarter of the beats and a handful of off-beat bumps.
pub fn noisy_guitar_envelope(beats: &[f64], hop_sec: f64, len_sec: f64, seed: u64) -> OnsetEnvelope {
    let mut rng = Lcg::new(seed);
    let mut values: Vec<f32> = (0..(len_sec / hop_sec) as usize).map(|_| rng.unit() as f32 * 0.3).collect();
    let mut bumps: Vec<(f64, f32)> = Vec::new();
    for &t in beats {
        if rng.unit() < 0.25 {
            bumps.push((t, 0.5));
        }
    }
    for _ in 0..beats.len() / 2 {
        bumps.push((rng.unit() * len_sec, 0.8));
    }
    for (t, height) in bumps {
        if let Some(v) = values.get_mut((t / hop_sec).round() as usize) {
            *v += height;
        }
    }
    OnsetEnvelope { values, hop_sec }
}

/// Rough 4/4 grid at `bpm` covering `len_sec`, starting at 0.
pub fn rough_grid(bpm: f64, len_sec: f64) -> Vec<Measure> {
    generate_measures(0.0, len_sec, bpm, TimeSignature::COMMON, &mut 1)
}

pub fn grid_beats(measures: &[Measure]) -> Vec<f64> {
    let map = TempoMap::new(measures);
    (0..map.len())
        .flat_map(|i| map.beat_lines(i))
        .filter(|l| l.kind != BeatKind::Subdivision)
        .map(|l| l.time_sec)
        .collect()
}

/// Mean distance between each true beat and the grid beat of the same index.
pub fn mean_beat_error(measures: &[Measure], truth: &[f64]) -> f64 {
    let grid = grid_beats(measures);
    let n = grid.len().min(truth.len());
    grid.iter().zip(truth).map(|(g, t)| (g - t).abs()).sum::<f64>() / n as f64
}
