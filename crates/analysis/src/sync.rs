//! Automatic track synchronisation: the time offset of a track relative to a reference track.
//!
//! Convention: `project_time = audio_time + offset_sec`; a positive offset means the track starts
//! later than the reference. Tracks of equal length are assumed to be in sync (`assume_synced`).
//!
//! 1. Onset envelopes in three bands (80-300 Hz, 300-2 kHz, 2-8 kHz): spectral flux of a
//!    log-magnitude STFT (hop `beatfit::HOP`), local mean removed, unit variance. Normalising per
//!    band lets different timbres (guitar, bass, drums) correlate.
//! 2. FFT cross-correlation of the envelopes, summed over the bands, over +/- `max_lag_sec`; the
//!    three highest peaks (at least `PEAK_SEPARATION_SEC` apart) are the candidates.
//! 3. Each peak is refined by correlating the band-passed (zero-phase), unit-RMS signals within
//!    +/- `REFINE_HOPS` hops and interpolating parabolically. A refinement that is not clearly
//!    peaked (different timbres do not correlate at sample level) is dropped and the envelope
//!    estimate, itself interpolated, is kept.
//! 4. Onset patterns repeat every beat. When the best two candidates are comparable (the lower is at
//!    least `AMBIGUITY_RATIO` of the higher) and their gap is about a multiple of the reference beat
//!    period, the best confidence is multiplied by `1 - ratio` and both get an "ambiguous by ..."
//!    note. With a `grid_hint`, the candidate whose strongest onsets sit closer to the measure and
//!    beat lines (measure lines weigh more) wins when it scores `GRID_DECISIVE` times better; the
//!    ambiguity is then resolved and not penalised.
//! 5. Tempi come from `beatfit::estimate_tempo`. If they differ by more than 1% (after folding
//!    double and half time), every candidate carries a drift note with the ratio.
//! 6. Confidence: peak height over the median absolute correlation, mapped to 0..1 (`confidence`).

use fretscribe_core::{BeatKind, Measure, TempoMap};
use realfft::num_complex::Complex;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

use crate::beatfit::{detect_onsets, estimate_tempo, octave_ratio, onset_envelope, HOP};

pub const DEFAULT_TOLERANCE_MS: f64 = 20.0;
pub const DEFAULT_MAX_LAG_SEC: f64 = 15.0;

const FFT_SIZE: usize = 2048;
const BANDS_HZ: [(f64, f64); 3] = [(80.0, 300.0), (300.0, 2000.0), (2000.0, 8000.0)];
const COMPRESSION: f32 = 1000.0;
const LOCAL_MEAN_SEC: f64 = 0.5;
const MAX_CANDIDATES: usize = 3;
const PEAK_SEPARATION_SEC: f64 = 0.1;
const REFINE_HOPS: usize = 2;
const REFINE_MIN_PROMINENCE: f32 = 12.0;
const AMBIGUITY_RATIO: f32 = 0.6;
const BEAT_MULTIPLES: [(f64, &str); 5] = [
    (0.5, "half a beat"),
    (1.0, "one beat"),
    (2.0, "two beats"),
    (3.0, "three beats"),
    (4.0, "one bar (four beats)"),
];
const GRID_SIGMA_SEC: f64 = 0.02;
const GRID_BEAT_WEIGHT: f64 = 0.2;
const GRID_DECISIVE: f64 = 1.25;
const GRID_MAX_ONSETS: usize = 100;
const TEMPO_RANGE_BPM: (f64, f64) = (50.0, 220.0);
const DRIFT_THRESHOLD: f64 = 0.01;
const CONFIDENCE_FLOOR: f32 = 10.0;
const CONFIDENCE_SCALE: f32 = 15.0;
const SNAP_DEDUP_SEC: f64 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub struct SyncOptions<'a> {
    pub max_lag_sec: f64,
    pub grid_hint: Option<&'a [Measure]>,
}

impl Default for SyncOptions<'_> {
    fn default() -> Self {
        Self { max_lag_sec: DEFAULT_MAX_LAG_SEC, grid_hint: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyncCandidate {
    pub offset_sec: f64,
    pub confidence: f32,
    pub note: Option<String>,
}

pub fn assume_synced(ref_duration_sec: f64, other_duration_sec: f64, tolerance_ms: f64) -> bool {
    (ref_duration_sec - other_duration_sec).abs() * 1000.0 <= tolerance_ms
}

/// Offsets near `current_offset_sec` (within `max_dist_sec`) that align an onset with a target, nearest first.
pub fn snap_offsets(
    other_onsets: &[f64],
    targets: &[f64],
    current_offset_sec: f64,
    max_dist_sec: f64,
) -> Vec<f64> {
    let mut sorted: Vec<f64> = targets.iter().copied().filter(|t| t.is_finite()).collect();
    sorted.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    for &onset in other_onsets.iter().filter(|o| o.is_finite()) {
        let centre = onset + current_offset_sec;
        let from = sorted.partition_point(|&t| t < centre - max_dist_sec);
        out.extend(
            sorted[from..].iter().take_while(|&&t| t <= centre + max_dist_sec).map(|t| t - onset),
        );
    }
    out.sort_by(f64::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < SNAP_DEDUP_SEC);
    out.sort_by(|a, b| (a - current_offset_sec).abs().total_cmp(&(b - current_offset_sec).abs()));
    out
}

/// Best-first candidates (at most 3); empty when the signals are too short or unusable.
pub fn estimate_offset(
    reference: &[f32],
    other: &[f32],
    sample_rate: u32,
    opts: SyncOptions,
) -> Vec<SyncCandidate> {
    if sample_rate == 0
        || opts.max_lag_sec.is_nan()
        || opts.max_lag_sec <= 0.0
        || reference.len() < FFT_SIZE
        || other.len() < FFT_SIZE
    {
        return Vec::new();
    }
    let bands = bands(sample_rate);
    if bands.is_empty() {
        return Vec::new();
    }
    let (sr, hop_sec) = (f64::from(sample_rate), HOP as f64 / f64::from(sample_rate));
    let env_r = envelopes(reference, sample_rate, &bands);
    let env_o = envelopes(other, sample_rate, &bands);
    let max_lag = (opts.max_lag_sec / hop_sec).ceil() as usize;
    let corr = xcorr(&env_r, &env_o, max_lag);
    let noise = median_abs(&corr).max(1e-9);
    let peaks = pick_peaks(&corr, (PEAK_SEPARATION_SEC / hop_sec).ceil() as usize);
    if peaks.is_empty() {
        return Vec::new();
    }

    let sig_lag = (max_lag + REFINE_HOPS) * HOP;
    let sig = xcorr(
        &band_signals(reference, sample_rate, &bands),
        &band_signals(other, sample_rate, &bands),
        sig_lag,
    );
    let onsets = opts.grid_hint.filter(|g| !g.is_empty()).map(|g| {
        let env = onset_envelope(other, sample_rate);
        (
            detect_onsets(&env).into_iter().map(|t| (t, f64::from(env.at(t)))).collect::<Vec<_>>(),
            grid_lines(g),
        )
    });

    struct Cand {
        offset: f64,
        height: f32,
        grid: f64,
    }
    let mut cands: Vec<Cand> = peaks
        .iter()
        .map(|&i| {
            let coarse = (i as f64 - max_lag as f64 + parabolic(&corr, i)) * HOP as f64;
            let offset = refine(&sig, sig_lag, coarse).unwrap_or(coarse) / sr;
            let grid = onsets.as_ref().map_or(0.0, |(o, lines)| grid_score(o, lines, offset));
            Cand { offset, height: corr[i], grid }
        })
        .collect();

    let (tempos_r, tempos_o) = (tempos(reference, sample_rate), tempos(other, sample_rate));
    let mut notes: Vec<Vec<String>> = vec![Vec::new(); cands.len()];
    let mut conf: Vec<f32> = cands.iter().map(|c| confidence(c.height / noise)).collect();

    if let [a, b, ..] = cands.as_slice() {
        let ratio = a.height.min(b.height) / a.height.max(b.height);
        let gap = (a.offset - b.offset).abs();
        let label = tempos_r.first().and_then(|bpm| beat_multiple_label(gap, 60.0 / bpm));
        if let (true, Some(label)) = (ratio >= AMBIGUITY_RATIO, label) {
            let decisive = onsets.is_some()
                && (a.grid > GRID_DECISIVE * b.grid || b.grid > GRID_DECISIVE * a.grid);
            let mut text = format!("ambiguous by {label} (about {gap:.2} s)");
            if decisive {
                text.push_str(", resolved with the measure grid");
                if b.grid > a.grid {
                    cands.swap(0, 1);
                    conf.swap(0, 1);
                }
            } else {
                conf[0] *= 1.0 - ratio;
            }
            notes[0].push(text.clone());
            notes[1].push(text);
        }
    }

    if let Some(ratio) = tempo_ratio(&tempos_r, &tempos_o) {
        if (ratio - 1.0).abs() > DRIFT_THRESHOLD {
            let text = format!("tempo ratio {ratio:.3}: a constant offset cannot align the tracks over their whole length (drift)");
            notes.iter_mut().for_each(|n| n.push(text.clone()));
        }
    }

    cands
        .iter()
        .zip(conf)
        .zip(notes)
        .map(|((c, confidence), n)| SyncCandidate {
            offset_sec: c.offset,
            confidence: confidence.clamp(0.0, 1.0),
            note: (!n.is_empty()).then(|| n.join("; ")),
        })
        .collect()
}

fn confidence(ratio: f32) -> f32 {
    1.0 - (-(ratio - CONFIDENCE_FLOOR).max(0.0) / CONFIDENCE_SCALE).exp()
}

fn tempos(x: &[f32], sample_rate: u32) -> Vec<f64> {
    let candidates = estimate_tempo(&onset_envelope(x, sample_rate), TEMPO_RANGE_BPM);
    candidates.into_iter().map(|t| t.0).collect()
}

/// Ratio of the closest pair of tempo candidates (other / reference), folded for double and half time.
fn tempo_ratio(reference: &[f64], other: &[f64]) -> Option<f64> {
    reference
        .iter()
        .flat_map(|&r| other.iter().map(move |&o| (o / r, octave_ratio(r, o))))
        .map(|(ratio, octave)| octave.map_or(ratio, |k| ratio / k))
        .min_by(|a, b| (a - 1.0).abs().total_cmp(&(b - 1.0).abs()))
}

fn beat_multiple_label(gap: f64, period: f64) -> Option<&'static str> {
    let tolerance = (0.05 * period).max(0.03);
    BEAT_MULTIPLES
        .iter()
        .map(|&(m, label)| ((gap - m * period).abs(), label))
        .filter(|d| d.0 <= tolerance)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|d| d.1)
}

fn grid_lines(measures: &[Measure]) -> Vec<(f64, f64)> {
    let map = TempoMap::new(measures);
    let mut lines: Vec<(f64, f64)> = (0..map.len())
        .flat_map(|i| map.beat_lines(i))
        .filter_map(|l| match l.kind {
            BeatKind::Measure => Some((l.time_sec, 1.0)),
            BeatKind::MainBeat => Some((l.time_sec, GRID_BEAT_WEIGHT)),
            BeatKind::Subdivision => None,
        })
        .collect();
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines
}

/// Strength-weighted closeness of the strongest onsets (moved by `offset`) to the grid lines.
fn grid_score(onsets: &[(f64, f64)], lines: &[(f64, f64)], offset: f64) -> f64 {
    let mut strongest: Vec<(f64, f64)> = onsets.to_vec();
    strongest.sort_by(|a, b| b.1.total_cmp(&a.1));
    strongest.truncate(GRID_MAX_ONSETS);
    let total: f64 = strongest.iter().map(|o| o.1).sum();
    if total <= 0.0 || lines.is_empty() {
        return 0.0;
    }
    let score: f64 = strongest
        .iter()
        .map(|&(t, strength)| {
            let x = t + offset;
            let i = lines.partition_point(|l| l.0 < x);
            let (d, w) = [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|j| lines.get(j))
                .map(|l| ((l.0 - x).abs(), l.1))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap_or((f64::INFINITY, 0.0));
            strength * w * (-0.5 * (d / GRID_SIGMA_SEC).powi(2)).exp()
        })
        .sum();
    score / total
}

fn bands(sample_rate: u32) -> Vec<(f64, f64)> {
    let nyquist = f64::from(sample_rate) * 0.45;
    BANDS_HZ.iter().map(|&(lo, hi)| (lo, hi.min(nyquist))).filter(|b| b.0 < 0.8 * b.1).collect()
}

fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos()) as f32).collect()
}

/// One normalised flux envelope per band; frame `k` is at `k * HOP` samples.
fn envelopes(x: &[f32], sample_rate: u32, bands: &[(f64, f64)]) -> Vec<Vec<f32>> {
    let frames = x.len() / HOP + 1;
    let mut env = vec![vec![0.0_f32; frames]; bands.len()];
    let peak = x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    if !(peak > 0.0 && peak.is_finite()) {
        return env;
    }
    let scale = COMPRESSION / (peak * FFT_SIZE as f32 / 4.0);
    let bin = |hz: f64| {
        ((hz * FFT_SIZE as f64 / f64::from(sample_rate)).round() as usize).min(FFT_SIZE / 2)
    };
    let ranges: Vec<(usize, usize)> = bands.iter().map(|&(lo, hi)| (bin(lo), bin(hi))).collect();
    let (k_lo, k_hi) = (ranges[0].0, ranges[ranges.len() - 1].1);

    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
    let (window, mut input, mut spectrum) =
        (hann(FFT_SIZE), fft.make_input_vec(), fft.make_output_vec());
    let mut previous: Option<Vec<f32>> = None;
    for t in 0..frames {
        let first = (t * HOP) as isize - (FFT_SIZE / 2) as isize;
        for (j, slot) in input.iter_mut().enumerate() {
            let sample = usize::try_from(first + j as isize).ok().and_then(|i| x.get(i)).copied();
            *slot = sample.map_or(0.0, |s| s * window[j]);
        }
        let processed = fft.process(&mut input, &mut spectrum);
        debug_assert!(processed.is_ok());
        let current: Vec<f32> =
            spectrum[k_lo..=k_hi].iter().map(|c| (1.0 + c.norm() * scale).ln()).collect();
        if let Some(prev) = &previous {
            for (band_env, &(lo, hi)) in env.iter_mut().zip(&ranges) {
                band_env[t] =
                    (lo..=hi).map(|k| (current[k - k_lo] - prev[k - k_lo]).max(0.0)).sum();
            }
        }
        previous = Some(current);
    }
    let radius = (LOCAL_MEAN_SEC * f64::from(sample_rate) / HOP as f64 / 2.0) as usize;
    env.iter_mut().for_each(|e| normalise(e, radius));
    env
}

/// Subtracts the local mean (window `2 * radius + 1`) and scales to unit variance.
fn normalise(env: &mut [f32], radius: usize) {
    let mut prefix = vec![0.0_f64];
    prefix.extend(env.iter().scan(0.0, |acc, &v| {
        *acc += f64::from(v);
        Some(*acc)
    }));
    let n = env.len();
    let means: Vec<f64> = (0..n)
        .map(|k| {
            let (a, b) = (k.saturating_sub(radius), (k + radius + 1).min(n));
            (prefix[b] - prefix[a]) / (b - a) as f64
        })
        .collect();
    env.iter_mut().zip(&means).for_each(|(v, m)| *v = (f64::from(*v) - m) as f32);
    let sd = (env.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / n.max(1) as f64).sqrt();
    env.iter_mut().for_each(|v| *v = if sd > 1e-9 { (f64::from(*v) / sd) as f32 } else { 0.0 });
}

/// Zero-phase second-order band-pass, scaled to unit RMS.
fn band_signals(x: &[f32], sample_rate: u32, bands: &[(f64, f64)]) -> Vec<Vec<f32>> {
    bands
        .iter()
        .map(|&(lo, hi)| {
            let mut y = bandpass(x, sample_rate, lo, hi);
            y.reverse();
            let mut y = bandpass(&y, sample_rate, lo, hi);
            y.reverse();
            let rms = (y.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>()
                / y.len().max(1) as f64)
                .sqrt();
            let gain = if rms > 1e-9 { (1.0 / rms) as f32 } else { 0.0 };
            y.iter_mut().for_each(|v| *v *= gain);
            y
        })
        .collect()
}

fn bandpass(x: &[f32], sample_rate: u32, lo: f64, hi: f64) -> Vec<f32> {
    let centre = (lo * hi).sqrt();
    let w0 = 2.0 * PI * centre / f64::from(sample_rate);
    let alpha = w0.sin() / (2.0 * centre / (hi - lo));
    let a0 = 1.0 + alpha;
    let (b0, b2, a1, a2) = (alpha / a0, -alpha / a0, -2.0 * w0.cos() / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    x.iter()
        .map(|&v| {
            let v = f64::from(v);
            let y = b0 * v + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y as f32
        })
        .collect()
}

/// `c[max_lag + d] = sum over bands and n of a[n + d] * b[n]`, zero where the signals do not overlap.
fn xcorr(a: &[Vec<f32>], b: &[Vec<f32>], max_lag: usize) -> Vec<f32> {
    let (la, lb) = (a[0].len(), b[0].len());
    let n = (la + lb).next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let (forward, inverse) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let transform = |x: &[f32]| -> Vec<Complex<f32>> {
        let mut input = forward.make_input_vec();
        input[..x.len()].copy_from_slice(x);
        let mut output = forward.make_output_vec();
        let processed = forward.process(&mut input, &mut output);
        debug_assert!(processed.is_ok());
        output
    };
    let mut sum = inverse.make_input_vec();
    for (x, y) in a.iter().zip(b) {
        for ((s, fx), fy) in sum.iter_mut().zip(transform(x)).zip(transform(y)) {
            *s += fx * fy.conj();
        }
    }
    let mut circular = inverse.make_output_vec();
    let processed = inverse.process(&mut sum, &mut circular);
    debug_assert!(processed.is_ok());
    let (lo, hi) = (-(lb as isize - 1), la as isize - 1);
    (-(max_lag as isize)..=max_lag as isize)
        .map(|d| {
            if d < lo || d > hi {
                0.0
            } else {
                circular[d.rem_euclid(n as isize) as usize] / n as f32
            }
        })
        .collect()
}

fn median_abs(values: &[f32]) -> f32 {
    let mut abs: Vec<f32> = values.iter().map(|v| v.abs()).collect();
    if abs.is_empty() {
        return 0.0;
    }
    let mid = abs.len() / 2;
    *abs.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Strictly positive local maxima, highest first, at least `min_separation` apart.
fn pick_peaks(c: &[f32], min_separation: usize) -> Vec<usize> {
    let mut maxima: Vec<usize> = (1..c.len().saturating_sub(1))
        .filter(|&i| c[i] > 0.0 && c[i] > c[i - 1] && c[i] >= c[i + 1])
        .collect();
    maxima.sort_by(|&a, &b| c[b].total_cmp(&c[a]));
    let mut chosen: Vec<usize> = Vec::new();
    for i in maxima {
        if chosen.iter().all(|&o| o.abs_diff(i) >= min_separation) {
            chosen.push(i);
        }
        if chosen.len() == MAX_CANDIDATES {
            break;
        }
    }
    chosen
}

/// Vertex offset (-0.5..0.5) of the parabola through the three samples around `i`.
fn parabolic(y: &[f32], i: usize) -> f64 {
    if i == 0 || i + 1 >= y.len() {
        return 0.0;
    }
    let (a, b, c) = (f64::from(y[i - 1]), f64::from(y[i]), f64::from(y[i + 1]));
    let curvature = a - 2.0 * b + c;
    if curvature.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (a - c) / curvature).clamp(-0.5, 0.5)
    }
}

/// Lag in samples of the signal correlation peak near `coarse`, or `None` if it is not clearly peaked.
fn refine(sig: &[f32], sig_lag: usize, coarse: f64) -> Option<f64> {
    let span = (REFINE_HOPS * HOP) as isize;
    let centre = coarse.round() as isize;
    let lo = (centre - span).max(-(sig_lag as isize));
    let hi = (centre + span).min(sig_lag as isize);
    let window: Vec<f32> = (lo..=hi).map(|d| sig[(d + sig_lag as isize) as usize]).collect();
    let (best, &peak) = window.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let mean = window.iter().map(|v| v.abs()).sum::<f32>() / window.len() as f32;
    (peak > 0.0 && peak >= REFINE_MIN_PROMINENCE * mean)
        .then(|| (lo + best as isize) as f64 + parabolic(&window, best))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 22_050;
    const SECONDS: f64 = 20.0;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    #[derive(Clone, Copy)]
    enum Timbre {
        Sine,
        Noise,
    }

    /// Sparse hits at irregular times, so the correlation peak is unique.
    fn sparse() -> Vec<(f64, f32)> {
        let mut rng = Lcg(5);
        let mut t = 0.5;
        let mut hits = Vec::new();
        while t < SECONDS - 4.0 {
            hits.push((t, 0.4 + 0.6 * rng.next()));
            t += 0.15 + 0.6 * f64::from(rng.next());
        }
        hits
    }

    /// Random hits on a `step`-second grid: a steady tempo.
    fn rhythm(step: f64) -> Vec<(f64, f32)> {
        let mut rng = Lcg(7);
        let mut hits = Vec::new();
        for k in 0..(SECONDS / step) as usize {
            if rng.next() < 0.5 {
                hits.push((k as f64 * step, 0.4 + 0.6 * rng.next()));
            }
        }
        hits
    }

    /// Hit at reference time `t` appears at `t - offset` in the rendered audio.
    fn render(hits: &[(f64, f32)], offset: f64, timbre: Timbre) -> Vec<f32> {
        let mut x = vec![0.0_f32; (SECONDS * f64::from(SR)) as usize];
        let mut rng = Lcg(99);
        for &(t, amp) in hits {
            let start = ((t - offset) * f64::from(SR)).round();
            if start < 0.0 {
                continue;
            }
            for j in 0..(0.1 * f64::from(SR)) as usize {
                let tau = j as f64 / f64::from(SR);
                let body = match timbre {
                    Timbre::Sine => (2.0 * PI * 1000.0 * tau).sin() as f32,
                    Timbre::Noise => rng.next() * 2.0 - 1.0,
                };
                if let Some(slot) = x.get_mut(start as usize + j) {
                    *slot += amp * body * (-tau / 0.02).exp() as f32;
                }
            }
        }
        x
    }

    fn noise(seed: u64) -> Vec<f32> {
        let mut rng = Lcg(seed);
        (0..(SECONDS * f64::from(SR)) as usize).map(|_| rng.next() - 0.5).collect()
    }

    fn best(reference: &[f32], other: &[f32]) -> SyncCandidate {
        estimate_offset(reference, other, SR, SyncOptions::default()).remove(0)
    }

    #[test]
    fn recovers_known_offsets() {
        let hits = sparse();
        let reference = render(&hits, 0.0, Timbre::Sine);
        for offset in [0.25, -0.08, 3.2] {
            let c = best(&reference, &render(&hits, offset, Timbre::Sine));
            assert!((c.offset_sec - offset).abs() < 0.002, "{offset}: {c:?}");
            assert!(c.confidence > 0.5, "{offset}: {c:?}");
        }
    }

    #[test]
    fn different_timbre_is_still_aligned() {
        let hits = sparse();
        let c = best(&render(&hits, 0.0, Timbre::Sine), &render(&hits, 0.25, Timbre::Noise));
        assert!((c.offset_sec - 0.25).abs() < 0.015, "{c:?}");
    }

    #[test]
    fn equal_durations_are_assumed_synced() {
        assert!(assume_synced(10.0, 10.0, DEFAULT_TOLERANCE_MS));
        assert!(assume_synced(10.0, 10.015, DEFAULT_TOLERANCE_MS));
        assert!(!assume_synced(10.0, 10.5, DEFAULT_TOLERANCE_MS));
    }

    #[test]
    fn unrelated_noise_has_low_confidence() {
        let c = best(&noise(11), &noise(0x9E37_79B9_7F4A_7C15));
        assert!(c.confidence < 0.3, "{c:?}");
    }

    #[test]
    fn tempo_mismatch_adds_a_drift_note() {
        let reference = render(&rhythm(0.25), 0.0, Timbre::Sine);
        let c = best(&reference, &render(&rhythm(0.25 * 1.06), 0.0, Timbre::Sine));
        assert!(c.note.is_some_and(|n| n.contains("drift")));
        assert!(best(&reference, &reference).note.is_none_or(|n| !n.contains("drift")));
    }

    #[test]
    fn periodic_pattern_is_ambiguous_and_the_grid_resolves_it() {
        let drums: Vec<(f64, f32)> = (0..40).map(|k| (k as f64 * 0.5, 1.0)).collect();
        let bass: Vec<(f64, f32)> = (0..10).map(|k| (k as f64 * 2.0, 1.0)).collect();
        let (reference, other) =
            (render(&drums, 0.0, Timbre::Sine), render(&bass, 0.0, Timbre::Sine));
        let plain = estimate_offset(&reference, &other, SR, SyncOptions::default());
        assert!(plain[0].note.as_ref().is_some_and(|n| n.contains("ambiguous")), "{plain:?}");
        assert!(plain[0].confidence < 0.5, "{plain:?}");

        let grid = fretscribe_core::generate_measures(
            0.0,
            SECONDS,
            120.0,
            fretscribe_core::TimeSignature::COMMON,
            &mut 1,
        );
        let opts = SyncOptions { grid_hint: Some(&grid), ..SyncOptions::default() };
        let hinted = estimate_offset(&reference, &other, SR, opts);
        let bar = (hinted[0].offset_sec / 2.0).round() * 2.0;
        assert!((hinted[0].offset_sec - bar).abs() < 0.005, "{hinted:?}");
    }

    #[test]
    fn short_or_invalid_input_gives_no_candidates() {
        assert!(estimate_offset(&[0.0; 10], &[0.0; 10], SR, SyncOptions::default()).is_empty());
        assert!(estimate_offset(&noise(11), &noise(12), 0, SyncOptions::default()).is_empty());
    }

    #[test]
    fn snap_offsets_are_nearest_first_and_bounded() {
        let offsets = snap_offsets(&[1.0, 2.0], &[1.3, 2.5, 5.0], 0.3, 0.3);
        assert_eq!(offsets.len(), 2);
        assert!(
            (offsets[0] - 0.3).abs() < 1e-12 && (offsets[1] - 0.5).abs() < 1e-12,
            "{offsets:?}"
        );
        assert!(snap_offsets(&[1.0], &[1.0, 1.00005], 0.0, 0.1).len() == 1);
        assert!(snap_offsets(&[], &[1.0], 0.0, 1.0).is_empty());
    }
}
