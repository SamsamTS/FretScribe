//! Reference pitch (A4) estimation: the global offset of a recording from the 440 Hz equal-tempered grid.
//!
//! 1. Long-window STFT (Hann, `WINDOW_AT_44K` samples at 44.1 kHz, hop a quarter window, zero-padded
//!    twice) over the region, or over the most tonal `max_analysis_sec` stretch when no region is
//!    given and the file is longer.
//! 2. Per frame, spectral peaks between `min_hz` and `max_hz` (local maxima above an adaptive
//!    threshold), refined by parabolic interpolation of the log magnitude.
//! 3. Each peak gives a deviation from the nearest semitone of the 440 Hz grid, in cents. Its weight
//!    is its magnitude times a stability factor: peaks with no counterpart within a few cents in the
//!    neighbouring frames (vibrato, bends, noise) count less.
//! 4. The deviations fill a circular histogram of 1-cent bins, smoothed with a circular Gaussian; the
//!    mode is refined by a local weighted circular mean of the raw deviations.
//! 5. Confidence combines the prominence of the mode over the mean of the histogram and the share of
//!    the weight within +/-10 cents of it; a flat histogram (noise, drums, heavy vibrato) scores low.
//!
//! Offsets are resolved only within +/-50 cents of the 440 Hz grid: a recording a whole semitone down,
//! or played with a capo, reads about 0 cents. That belongs to the string tuning and the capo, not to
//! the reference pitch. A guitar that is itself out of tune lowers the confidence. Harmonics that
//! are not octaves of the fundamental sit a fixed number of cents away from the grid (the third
//! partial +2, the fifth -14) and slightly spread the histogram of even a perfect recording.

use fretscribe_core::{cents_off, ref_from_cents, PitchRef};
use rayon::prelude::*;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

pub const HISTOGRAM_BINS: usize = 100;

const WINDOW_AT_44K: f64 = 16384.0;
const ZERO_PAD: usize = 2;
const PEAK_MEDIAN_FACTOR: f32 = 3.0;
const PEAK_RELATIVE_MAX: f32 = 0.01;
const STABILITY_CENTS: f64 = 10.0;
const NEIGHBOUR_CENTS: f64 = 50.0;
const UNMATCHED_WEIGHT: f64 = 0.1;
const SMOOTHING_SIGMA_BINS: f64 = 3.0;
const REFINE_CENTS: f64 = 3.0;
const REFINE_ITERATIONS: usize = 3;
const SHARE_CENTS: f64 = 10.0;
const FLAT_SHARE: f64 = SHARE_CENTS * 2.0 / HISTOGRAM_BINS as f64;
const PROMINENCE_FULL: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchRefOptions {
    pub max_analysis_sec: f64,
    pub min_hz: f32,
    pub max_hz: f32,
}

impl Default for PitchRefOptions {
    fn default() -> Self {
        Self { max_analysis_sec: 90.0, min_hz: 80.0, max_hz: 2000.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PitchRefEstimate {
    /// Offset from A4 = 440 Hz within -50..=+50 (positive: the recording is sharp).
    pub cents: f32,
    pub a4_hz: f32,
    /// 0..1.
    pub confidence: f32,
    /// Smoothed weight per 1-cent bin over -50..+50 (bin `i` is centred on `i - 49.5` cents),
    /// normalised to a sum of 1 (all zero when nothing was found).
    pub histogram: Vec<f32>,
}

impl PitchRefEstimate {
    fn none() -> Self {
        Self::at(0.0, 0.0, vec![0.0; HISTOGRAM_BINS])
    }

    fn at(cents: f64, confidence: f64, histogram: Vec<f32>) -> Self {
        let cents = cents as f32;
        Self {
            cents,
            a4_hz: ref_from_cents(cents).a4_hz(),
            confidence: confidence as f32,
            histogram,
        }
    }
}

struct Peak {
    freq: f64,
    cents: f64,
    mag: f32,
}

fn wrap_cents(c: f64) -> f64 {
    (c + 50.0).rem_euclid(100.0) - 50.0
}

pub fn estimate_reference(
    mono: &[f32],
    sample_rate: u32,
    region: Option<(f64, f64)>,
    opts: PitchRefOptions,
) -> PitchRefEstimate {
    let rate = f64::from(sample_rate);
    let window = 1usize << (WINDOW_AT_44K * rate / 44_100.0).log2().round().clamp(9.0, 16.0) as u32;
    let hop = window / 4;
    let fft_len = window * ZERO_PAD;
    let bin_hz = rate / fft_len as f64;
    let lo = ((f64::from(opts.min_hz) / bin_hz).ceil() as usize).max(2);
    let hi = ((f64::from(opts.max_hz) / bin_hz).floor() as usize).min(fft_len / 2 - 2);
    let region = region.filter(|r| r.0.is_finite() && r.1.is_finite());
    let samples = match region {
        Some((a, b)) => {
            let at = |s: f64| ((s.max(0.0) * rate) as usize).min(mono.len());
            &mono[at(a.min(b))..at(a.max(b))]
        }
        None => mono,
    };
    if sample_rate == 0 || lo >= hi || samples.len() < window / 4 {
        return PitchRefEstimate::none();
    }

    let frames = if samples.len() < window { 1 } else { (samples.len() - window) / hop + 1 };
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(fft_len);
    let taper: Vec<f32> = (0..window)
        .map(|i| (0.5 - 0.5 * (2.0 * PI * i as f64 / window as f64).cos()) as f32)
        .collect();
    let peaks: Vec<Vec<Peak>> = (0..frames)
        .into_par_iter()
        .map_init(
            || (fft.make_input_vec(), fft.make_output_vec()),
            |(input, output), k| {
                let frame = &samples[(k * hop).min(samples.len())..];
                for (i, slot) in input.iter_mut().enumerate() {
                    *slot =
                        if i < window { frame.get(i).map_or(0.0, |s| s * taper[i]) } else { 0.0 };
                }
                if fft.process(input, output).is_err() {
                    return Vec::new();
                }
                let mags: Vec<f32> = output[lo - 1..=hi + 1].iter().map(|c| c.norm()).collect();
                frame_peaks(&mags, lo, bin_hz)
            },
        )
        .collect();

    let mut weighted = weigh(&peaks);
    let window_frames = (opts.max_analysis_sec * rate / hop as f64) as usize;
    if region.is_none() && window_frames >= 1 && weighted.len() > window_frames {
        let energy: Vec<f64> = weighted.iter().map(|f| f.iter().map(|p| p.1).sum()).collect();
        let mut sum: f64 = energy[..window_frames].iter().sum();
        let (mut best, mut start) = (sum, 0);
        for i in window_frames..energy.len() {
            sum += energy[i] - energy[i - window_frames];
            if sum > best {
                (best, start) = (sum, i + 1 - window_frames);
            }
        }
        weighted = weighted[start..start + window_frames].to_vec();
    }
    summarise(&weighted.concat())
}

/// Peaks of one frame; `mags[0]` is bin `first - 1`.
fn frame_peaks(mags: &[f32], first: usize, bin_hz: f64) -> Vec<Peak> {
    let inner = &mags[1..mags.len() - 1];
    let mut sorted = inner.to_vec();
    let median = *sorted.select_nth_unstable_by(inner.len() / 2, f32::total_cmp).1;
    let max = inner.iter().copied().fold(0.0, f32::max);
    let threshold = (PEAK_MEDIAN_FACTOR * median).max(PEAK_RELATIVE_MAX * max);
    let log = |m: f32| f64::from(m.max(1e-20)).ln();
    (1..mags.len() - 1)
        .filter(|&i| mags[i] > threshold && mags[i] > mags[i - 1] && mags[i] >= mags[i + 1])
        .filter_map(|i| {
            let (a, b, c) = (log(mags[i - 1]), log(mags[i]), log(mags[i + 1]));
            let curvature = a - 2.0 * b + c;
            (curvature < 0.0).then(|| {
                let delta = (0.5 * (a - c) / curvature).clamp(-0.5, 0.5);
                let freq = ((first + i - 1) as f64 + delta) * bin_hz;
                let cents = f64::from(cents_off(freq as f32, &PitchRef::default()).1);
                Peak { freq, cents, mag: mags[i] }
            })
        })
        .collect()
}

/// `(deviation in cents, weight)` per frame; weight = magnitude times frequency stability.
fn weigh(frames: &[Vec<Peak>]) -> Vec<Vec<(f64, f64)>> {
    let neighbours =
        |t: usize| frames[t.saturating_sub(1)..(t + 2).min(frames.len())].iter().enumerate();
    (0..frames.len())
        .map(|t| {
            frames[t]
                .iter()
                .map(|p| {
                    let nearest = neighbours(t)
                        .filter(|(j, _)| t.saturating_sub(1) + j != t)
                        .flat_map(|(_, f)| f.iter())
                        .map(|q| (1200.0 * (p.freq / q.freq).log2()).abs())
                        .fold(f64::INFINITY, f64::min);
                    let stability = if nearest <= NEIGHBOUR_CENTS {
                        1.0 / (1.0 + (nearest / STABILITY_CENTS).powi(2))
                    } else {
                        UNMATCHED_WEIGHT
                    };
                    (p.cents, f64::from(p.mag) * stability)
                })
                .collect()
        })
        .collect()
}

fn summarise(points: &[(f64, f64)]) -> PitchRefEstimate {
    let total: f64 = points.iter().map(|p| p.1).sum();
    if total.is_nan() || total <= 0.0 {
        return PitchRefEstimate::none();
    }
    let n = HISTOGRAM_BINS;
    let mut raw = vec![0.0_f64; n];
    for &(cents, w) in points {
        let x = cents + 50.0 - 0.5;
        let (i, frac) = (x.floor(), x - x.floor());
        raw[i.rem_euclid(n as f64) as usize] += w * (1.0 - frac);
        raw[(i + 1.0).rem_euclid(n as f64) as usize] += w * frac;
    }
    let radius = (3.0 * SMOOTHING_SIGMA_BINS).ceil() as isize;
    let kernel: Vec<f64> = (-radius..=radius)
        .map(|j| (-0.5 * (j as f64 / SMOOTHING_SIGMA_BINS).powi(2)).exp())
        .collect();
    let norm: f64 = kernel.iter().sum();
    let smooth: Vec<f64> = (0..n)
        .map(|i| {
            kernel
                .iter()
                .zip(-radius..=radius)
                .map(|(g, j)| g * raw[(i as isize + j).rem_euclid(n as isize) as usize])
                .sum::<f64>()
                / norm
        })
        .collect();
    let mode = (0..n).max_by(|&a, &b| smooth[a].total_cmp(&smooth[b])).unwrap_or(0);

    let mut cents = -50.0 + mode as f64 + 0.5;
    for _ in 0..REFINE_ITERATIONS {
        let (sum, weight) = points
            .iter()
            .map(|&(c, w)| (wrap_cents(c - cents), w))
            .filter(|(d, _)| d.abs() <= REFINE_CENTS)
            .fold((0.0, 0.0), |(s, t), (d, w)| (s + d * w, t + w));
        if weight > 0.0 {
            cents = wrap_cents(cents + sum / weight);
        }
    }

    let share = points
        .iter()
        .filter(|p| wrap_cents(p.0 - cents).abs() <= SHARE_CENTS)
        .map(|p| p.1)
        .sum::<f64>()
        / total;
    let prominence = smooth[mode] / (smooth.iter().sum::<f64>() / n as f64);
    let by_share = ((share - FLAT_SHARE) / (1.0 - FLAT_SHARE)).clamp(0.0, 1.0);
    let by_prominence = ((prominence - 1.0) / (PROMINENCE_FULL - 1.0)).clamp(0.0, 1.0);
    let histogram = smooth.iter().map(|v| (v / smooth.iter().sum::<f64>()) as f32).collect();
    PitchRefEstimate::at(cents, (by_share * by_prominence).sqrt(), histogram)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44_100;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32 - 0.5
        }
    }

    const MELODY: [u32; 8] = [45, 52, 57, 60, 64, 67, 71, 76];

    /// Decaying harmonic notes (partials 1..=8 at 1/n) tuned to `a4`, optionally with vibrato.
    fn tones(a4: f64, transpose: u32, vibrato_cents: f64) -> Vec<f32> {
        let (note_len, rate) = (1.2, f64::from(SR));
        let mut out = vec![0.0_f32; (MELODY.len() as f64 * note_len * rate) as usize];
        for (k, midi) in MELODY.iter().enumerate() {
            let f0 = a4 * 2f64.powf((f64::from(midi + transpose) - 69.0) / 12.0);
            let mut phase = 0.0;
            for j in 0..(note_len * rate) as usize {
                let t = j as f64 / rate;
                let bend = 2f64.powf(vibrato_cents / 1200.0 * (2.0 * PI * 5.0 * t).sin());
                phase += 2.0 * PI * f0 * bend / rate;
                let env = (1.0 - (-t / 0.01).exp()) * (-t / 0.8).exp();
                let sum: f64 = (1..=8).map(|n| (phase * f64::from(n)).sin() / f64::from(n)).sum();
                out[k * (note_len * rate) as usize + j] += (0.3 * env * sum) as f32;
            }
        }
        out
    }

    fn estimate(x: &[f32]) -> PitchRefEstimate {
        estimate_reference(x, SR, None, PitchRefOptions::default())
    }

    fn cents_of(hz: f64) -> f64 {
        1200.0 * (hz / 440.0).log2()
    }

    #[test]
    fn recovers_references_within_a_cent() {
        for hz in [432.0, 442.0, 448.0, 440.0] {
            let e = estimate(&tones(hz, 0, 0.0));
            assert!((f64::from(e.cents) - cents_of(hz)).abs() < 1.0, "{hz}: {e:?}");
            assert!((f64::from(e.a4_hz) - hz).abs() < 0.2, "{hz}: {e:?}");
            assert!(e.confidence > 0.6, "{hz}: {}", e.confidence);
        }
    }

    #[test]
    fn a_whole_semitone_up_reads_zero() {
        let e = estimate(&tones(440.0, 1, 0.0));
        assert!(e.cents.abs() < 1.0, "{e:?}");
    }

    #[test]
    fn noise_and_vibrato_lower_the_confidence() {
        let clean = estimate(&tones(432.0, 0, 0.0)).confidence;
        let mut rng = Lcg(3);
        let mut noisy = tones(432.0, 0, 0.0);
        noisy.iter_mut().for_each(|s| *s += 40.0 * rng.next());
        let noisy = estimate(&noisy).confidence;
        assert!(noisy < clean - 0.1, "clean {clean} noisy {noisy}");
        let vibrato = estimate(&tones(432.0, 0, 30.0)).confidence;
        assert!(vibrato < clean - 0.2, "clean {clean} vibrato {vibrato}");
    }

    #[test]
    fn noise_and_drums_have_low_confidence() {
        let mut rng = Lcg(11);
        let noise: Vec<f32> = (0..10 * SR as usize).map(|_| rng.next()).collect();
        assert!(estimate(&noise).confidence < 0.3, "{}", estimate(&noise).confidence);

        let mut drums = vec![0.0_f32; 10 * SR as usize];
        for hit in 0..40 {
            for j in 0..(0.15 * f64::from(SR)) as usize {
                let t = j as f32 / SR as f32;
                let kick =
                    (2.0 * std::f32::consts::PI * (120.0 * (-t * 20.0).exp() + 50.0) * t).sin();
                let body = if hit % 2 == 0 { kick } else { rng.next() * 2.0 };
                drums[hit * SR as usize / 4 + j] += body * (-t / 0.03).exp();
            }
        }
        assert!(estimate(&drums).confidence < 0.3, "{}", estimate(&drums).confidence);
    }

    #[test]
    fn region_limits_the_analysis() {
        let mut x = tones(432.0, 0, 0.0);
        x.extend(tones(448.0, 0, 0.0));
        let seconds = MELODY.len() as f64 * 1.2;
        let opts = PitchRefOptions::default();
        let first = estimate_reference(&x, SR, Some((0.5, seconds - 0.5)), opts);
        let second = estimate_reference(&x, SR, Some((seconds + 0.5, 2.0 * seconds - 0.5)), opts);
        assert!((f64::from(first.cents) - cents_of(432.0)).abs() < 1.0, "{first:?}");
        assert!((f64::from(second.cents) - cents_of(448.0)).abs() < 1.0, "{second:?}");
    }

    #[test]
    fn long_files_use_the_tonal_stretch() {
        let mut rng = Lcg(5);
        let mut x: Vec<f32> = (0..12 * SR as usize).map(|_| 0.3 * rng.next()).collect();
        x.extend(tones(432.0, 0, 0.0));
        let opts = PitchRefOptions { max_analysis_sec: 10.0, ..PitchRefOptions::default() };
        let e = estimate_reference(&x, SR, None, opts);
        assert!((f64::from(e.cents) - cents_of(432.0)).abs() < 1.0, "{e:?}");
    }

    #[test]
    fn degenerate_input_is_inert() {
        for x in [&[][..], &[0.0; 100][..], &vec![0.0; 50_000][..]] {
            let e = estimate(x);
            assert_eq!((e.cents, e.confidence, e.histogram.len()), (0.0, 0.0, HISTOGRAM_BINS));
        }
        assert_eq!(
            estimate_reference(&[0.5; 50_000], 0, None, PitchRefOptions::default()).confidence,
            0.0
        );
    }

    #[test]
    fn histogram_is_normalised() {
        let e = estimate(&tones(442.0, 0, 0.0));
        assert!((e.histogram.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }
}
