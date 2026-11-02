//! Constant-Q style transform built from half-octave bands.
//!
//! Every band has its own FFT size and window length: `q / f` seconds for the band's lowest row,
//! clamped, so no row gets less frequency resolution than `q` promises. All bands share the output hop grid and window centres are aligned on the frame times, so
//! frames line up in time. Bands with long windows are only evaluated every `step` frames and
//! linearly interpolated in between (their windows are far longer than the hop). Row values are
//! read between FFT bins with a quadratic fit on the dB magnitudes.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

use crate::spectrogram::{SpectrogramInfo, SpectrogramParams};

const BAND_SEMITONES: usize = 6;
const HOPS_PER_WINDOW: f64 = 8.0;
const DB_CLIP_MARGIN: f32 = 30.0;
const GRID_A4_HZ: f64 = 440.0;
const A4_MIDI: f64 = 69.0;
const POWER_EPSILON: f32 = 1e-12;

struct RowTap {
    bin: usize,
    offset: f32,
    inside: bool,
}

struct Band {
    first_row: usize,
    n_rows: usize,
    window_len: usize,
    fft_len: usize,
    window: Vec<f32>,
    gain: f32,
    step: usize,
    plan: Arc<dyn RealToComplex<f32>>,
    bin_lo: usize,
    bin_hi: usize,
    taps: Vec<RowTap>,
}

pub(crate) struct Plan {
    bands: Vec<Band>,
    info: SpectrogramInfo,
    hop: usize,
    db_floor: f32,
    db_ceiling: f32,
}

#[derive(Default)]
pub(crate) struct Scratch {
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    fft: Vec<Complex<f32>>,
    db: Vec<f32>,
    coarse: Vec<f32>,
}

pub(crate) fn row_frequency_hz(min_midi: f32, bins_per_semitone: usize, row: usize) -> f64 {
    let midi = f64::from(min_midi) + row as f64 / bins_per_semitone as f64;
    GRID_A4_HZ * ((midi - A4_MIDI) / 12.0).exp2()
}

impl Plan {
    pub(crate) fn new(params: &SpectrogramParams, sample_rate: u32, len: usize) -> Self {
        let p = params.sanitized();
        let rate = f64::from(sample_rate.max(1));
        let bps = usize::from(p.bins_per_semitone);
        let n_rows = ((p.max_midi - p.min_midi) * bps as f32).floor() as usize + 1;
        let hop = p.hop_size;
        let info = SpectrogramInfo {
            n_frames: len.div_ceil(hop),
            n_rows,
            hop_sec: hop as f64 / rate,
            min_midi: p.min_midi,
            max_midi: p.min_midi + (n_rows - 1) as f32 / bps as f32,
            bins_per_semitone: p.bins_per_semitone,
        };

        let mut planner = RealFftPlanner::<f32>::new();
        let rows_per_band = BAND_SEMITONES * bps;
        let bands = (0..n_rows.div_ceil(rows_per_band))
            .map(|b| {
                let first_row = b * rows_per_band;
                let n_rows = rows_per_band.min(info.n_rows - first_row);
                let lowest_hz = row_frequency_hz(p.min_midi, bps, first_row);
                let seconds = (f64::from(p.q) / lowest_hz)
                    .clamp(f64::from(p.min_window_sec), f64::from(p.max_window_sec));
                let window_len = (((seconds * rate).round() as usize).max(8)) & !1;
                let fft_len = window_len.next_power_of_two();
                let step = ((window_len as f64 / HOPS_PER_WINDOW / hop as f64) as usize).max(1);
                let window = hann(window_len);
                let gain = 2.0 / window.iter().sum::<f32>();
                let taps: Vec<RowTap> = (0..n_rows)
                    .map(|r| {
                        let hz = row_frequency_hz(p.min_midi, bps, first_row + r);
                        let position = hz * fft_len as f64 / rate;
                        let bin = (position.round() as usize).max(1);
                        RowTap {
                            bin,
                            offset: (position - bin as f64) as f32,
                            inside: bin < fft_len / 2,
                        }
                    })
                    .collect();
                let inside = || taps.iter().filter(|t| t.inside);
                let bin_lo = inside().map(|t| t.bin - 1).min().unwrap_or(0);
                let bin_hi = inside().map(|t| t.bin + 1).max().unwrap_or(0);
                Band {
                    first_row,
                    n_rows,
                    window_len,
                    fft_len,
                    window,
                    gain,
                    step,
                    plan: planner.plan_fft_forward(fft_len),
                    bin_lo,
                    bin_hi,
                    taps,
                }
            })
            .collect();

        Self {
            bands,
            info,
            hop,
            db_floor: p.db_floor,
            db_ceiling: p.db_ceiling,
        }
    }

    pub(crate) fn info(&self) -> &SpectrogramInfo {
        &self.info
    }

    /// Row-major `n_rows x (end - start)` normalised values for the frames `start..end`.
    pub(crate) fn compute_block(
        &self,
        mono: &[f32],
        start: usize,
        end: usize,
        scratch: &mut Scratch,
    ) -> Vec<f32> {
        let frames = end - start;
        let mut out = vec![0.0; self.info.n_rows * frames];
        let mut coarse = std::mem::take(&mut scratch.coarse);
        for band in &self.bands {
            let first = start / band.step;
            let last = (end - 1).div_ceil(band.step);
            coarse.clear();
            coarse.resize((last - first + 1) * band.n_rows, 0.0);
            for (i, values) in coarse.chunks_exact_mut(band.n_rows).enumerate() {
                let centre = (first + i) * band.step * self.hop;
                self.band_frame(band, mono, centre, scratch, values);
            }
            let scale = 1.0 / (self.db_ceiling - self.db_floor);
            for row in 0..band.n_rows {
                let out_row = &mut out[(band.first_row + row) * frames..][..frames];
                for (slot, frame) in out_row.iter_mut().zip(start..end) {
                    let at = (frame / band.step - first) * band.n_rows + row;
                    let phase = frame % band.step;
                    let mut db = coarse[at];
                    if phase != 0 {
                        let next = coarse[at + band.n_rows];
                        db += (next - db) * phase as f32 / band.step as f32;
                    }
                    *slot = ((db - self.db_floor) * scale).clamp(0.0, 1.0);
                }
            }
        }
        scratch.coarse = coarse;
        out
    }

    fn band_frame(
        &self,
        band: &Band,
        mono: &[f32],
        centre: usize,
        s: &mut Scratch,
        out: &mut [f32],
    ) {
        let clip = self.db_floor - DB_CLIP_MARGIN;
        s.input.clear();
        s.input.resize(band.fft_len, 0.0);
        let origin = centre as isize - (band.window_len / 2) as isize;
        let lo = (-origin).max(0) as usize;
        let hi = band
            .window_len
            .min((mono.len() as isize - origin).max(0) as usize);
        for i in lo..hi {
            s.input[i] = mono[(origin + i as isize) as usize] * band.window[i];
        }
        s.output.clear();
        s.output
            .resize(band.fft_len / 2 + 1, Complex::new(0.0, 0.0));
        s.fft.clear();
        s.fft
            .resize(band.plan.get_scratch_len(), Complex::new(0.0, 0.0));
        if band
            .plan
            .process_with_scratch(&mut s.input, &mut s.output, &mut s.fft)
            .is_err()
        {
            out.fill(clip);
            return;
        }

        let gain2 = band.gain * band.gain;
        s.db.clear();
        s.db.extend(
            s.output[band.bin_lo..=band.bin_hi]
                .iter()
                .map(|c| (10.0 * (c.norm_sqr() * gain2 + POWER_EPSILON).log10()).max(clip)),
        );
        for (value, tap) in out.iter_mut().zip(&band.taps) {
            *value = if tap.inside {
                let at = tap.bin - band.bin_lo;
                let (below, centre, above) = (s.db[at - 1], s.db[at], s.db[at + 1]);
                let d = tap.offset;
                centre + 0.5 * d * (above - below) + 0.5 * d * d * (above + below - 2.0 * centre)
            } else {
                clip
            };
        }
    }
}

fn hann(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / len as f64).cos() as f32)
        .collect()
}
