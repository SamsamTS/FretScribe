//! Waveform peak pyramid and its CPU-side colour mapping.
//!
//! Level 0 holds one min/max/sum-of-squares block per `BASE_BLOCK` samples; every further level
//! merges pairs of blocks of the previous one. A column of `peaks` reads the coarsest level whose
//! blocks still fit inside a pixel, so drawing costs O(pixels) at any zoom. The finest resolution
//! is one base block: zooming in further repeats blocks instead of reading raw samples.

use rayon::prelude::*;

use crate::display::apply_gamma;
use crate::lut::Lut;

pub const BASE_BLOCK: usize = 64;

/// Tuned by hand; not a setting and unrelated to the spectrogram's gamma.
pub const WAVEFORM_GAMMA: f32 = 1.25;

/// Colours are LUT positions. The envelope is the column's loudness level times `ENVELOPE_SHARE`;
/// the RMS body is that level raised by `BODY_LIFT`, so the two stay distinct even at full scale.
const ENVELOPE_SHARE: f32 = 0.8;
const BODY_LIFT: f32 = 0.3;
/// Vertical gradient of both: most intense on the centre line, `EDGE_SCALE` times that at the tips.
const EDGE_SCALE: f32 = 0.7;
const CENTRE_LINE_LEVEL: f32 = 0.2;

type Peak = (f32, f32, f32);

#[derive(Clone, Copy, Debug)]
struct Block {
    min: f32,
    max: f32,
    sum_sq: f64,
}

impl Block {
    const IDENTITY: Self = Self {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
        sum_sq: 0.0,
    };

    fn from_samples(samples: &[f32]) -> Self {
        samples.iter().fold(Self::IDENTITY, |acc, &v| {
            let v = if v.is_finite() { v } else { 0.0 };
            Self {
                min: acc.min.min(v),
                max: acc.max.max(v),
                sum_sq: acc.sum_sq + f64::from(v) * f64::from(v),
            }
        })
    }

    fn merged(blocks: &[Self]) -> Self {
        blocks.iter().fold(Self::IDENTITY, |acc, b| Self {
            min: acc.min.min(b.min),
            max: acc.max.max(b.max),
            sum_sq: acc.sum_sq + b.sum_sq,
        })
    }
}

#[derive(Clone, Debug)]
pub struct WaveformPyramid {
    levels: Vec<Vec<Block>>,
    sample_rate: u32,
    len: usize,
}

impl WaveformPyramid {
    pub fn build(mono: &[f32], sample_rate: u32) -> Self {
        let mut levels = Vec::new();
        if !mono.is_empty() {
            let mut current: Vec<Block> = mono
                .par_chunks(BASE_BLOCK)
                .map(Block::from_samples)
                .collect();
            while current.len() > 1 {
                let next = current.par_chunks(2).map(Block::merged).collect();
                levels.push(std::mem::replace(&mut current, next));
            }
            levels.push(current);
        }
        Self {
            levels,
            sample_rate,
            len: mono.len(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn duration_sec(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.len as f64 / f64::from(self.sample_rate)
        }
    }

    /// Exactly `pixel_width` columns of `(min, max, rms)` for the time range `(start, end)`.
    /// Parts of the range outside the audio, and invalid ranges, give silent `(0, 0, 0)` columns.
    pub fn peaks(&self, time_range_sec: (f64, f64), pixel_width: usize) -> Vec<Peak> {
        const SILENT: Peak = (0.0, 0.0, 0.0);
        let (start, end) = time_range_sec;
        let usable = !self.levels.is_empty()
            && self.sample_rate > 0
            && start.is_finite()
            && end.is_finite()
            && end > start;
        if !usable {
            return vec![SILENT; pixel_width];
        }

        let rate = f64::from(self.sample_rate);
        let per_pixel = (end - start) * rate / pixel_width as f64;
        let level = self.level_for(per_pixel);
        let size = BASE_BLOCK << level;
        let size_f = size as f64;
        let blocks = &self.levels[level];
        let count = blocks.len() as f64;

        (0..pixel_width)
            .map(|x| {
                let from = start * rate + x as f64 * per_pixel;
                if from + per_pixel <= 0.0 || from >= self.len as f64 {
                    return SILENT;
                }
                let first = (from / size_f).floor();
                let last = ((from + per_pixel) / size_f).ceil().max(first + 1.0);
                let lo = first.clamp(0.0, count) as usize;
                let hi = last.clamp(0.0, count) as usize;
                if lo >= hi {
                    return SILENT;
                }
                let block = Block::merged(&blocks[lo..hi]);
                let samples = (hi * size).min(self.len) - lo * size;
                let rms = (block.sum_sq / samples as f64).sqrt();
                (block.min, block.max, rms as f32)
            })
            .collect()
    }

    fn level_for(&self, samples_per_pixel: f64) -> usize {
        let mut level = 0;
        while level + 1 < self.levels.len()
            && ((BASE_BLOCK << (level + 1)) as f64) <= samples_per_pixel
        {
            level += 1;
        }
        level
    }
}

fn sanitise(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Image row range covering amplitudes `low..=high`; row 0 is amplitude +1.
fn rows_for(low: f32, high: f32, height: u32) -> (u32, u32) {
    let h = height as f32;
    let top = ((1.0 - high) / 2.0 * h).floor().clamp(0.0, h - 1.0);
    let bottom = (((1.0 - low) / 2.0 * h).ceil() - 1.0).clamp(top, h - 1.0);
    (top as u32, bottom as u32)
}

/// Width in pixels of the moving average applied to the column colours along the time axis
/// (window of `2 * (n / 2) + 1` pixels, 0 or 1 disables it). Only colours are smoothed: the
/// envelope outline always follows the peaks exactly.
pub const WAVEFORM_COLOR_SMOOTHING_PX: usize = 20;

struct Column {
    envelope: (u32, u32),
    body: Option<(u32, u32)>,
    low: f32,
    high: f32,
    level: f32,
}

fn column_for(peak: Peak, height: u32) -> Option<Column> {
    let (min, max, rms) = (sanitise(peak.0), sanitise(peak.1), sanitise(peak.2).abs());
    let (low, high) = (min.min(max), min.max(max));
    let loudness = low.abs().max(high.abs());
    if loudness <= 0.0 {
        return None;
    }
    let envelope = rows_for(low, high, height);
    let body = rows_for(-rms, rms, height);
    let body = (body.0.max(envelope.0), body.1.min(envelope.1));
    Some(Column {
        envelope,
        body: (body.0 <= body.1).then_some(body),
        low,
        high,
        level: apply_gamma(loudness, WAVEFORM_GAMMA),
    })
}

/// Moving average over the drawn columns only, so notes do not fade into the silence around them.
fn smoothed(columns: &[Option<Column>], pixels: usize, value: fn(&Column) -> f32) -> Vec<f32> {
    let radius = pixels / 2;
    let mut sums = vec![0.0f64];
    let mut counts = vec![0usize];
    for column in columns {
        let (v, drawn) = column
            .as_ref()
            .map_or((0.0, 0), |c| (f64::from(value(c)), 1));
        sums.push(sums[sums.len() - 1] + v);
        counts.push(counts[counts.len() - 1] + drawn);
    }
    (0..columns.len())
        .map(|x| {
            let (lo, hi) = (
                x.saturating_sub(radius),
                (x + radius + 1).min(columns.len()),
            );
            match counts[hi] - counts[lo] {
                0 => 0.0,
                n => ((sums[hi] - sums[lo]) / n as f64) as f32,
            }
        })
        .collect()
}

/// Distance of a pixel row from the centre line, 0 at the centre and 1 at the envelope tip on
/// its side.
fn distance_from_centre(row: u32, height: u32, column: &Column) -> f32 {
    let amplitude = 1.0 - (row as f32 + 0.5) * 2.0 / height as f32;
    let reach = if amplitude >= 0.0 {
        column.high
    } else {
        -column.low
    };
    if reach <= f32::EPSILON {
        1.0
    } else {
        (amplitude.abs() / reach).min(1.0)
    }
}

fn fade(distance: f32) -> f32 {
    1.0 - (1.0 - EDGE_SCALE) * distance
}

/// One RGBA column per peak, `height` rows, row 0 on top. The background is the LUT's lowest
/// colour; columns are coloured by their loudness (smoothed over `WAVEFORM_COLOR_SMOOTHING_PX`
/// pixels), with the RMS body brighter than the envelope and both fading from the centre line to
/// the tips.
pub fn render_rgba(peaks: &[Peak], height: u32, lut: &Lut) -> Vec<u8> {
    render_with_smoothing(peaks, height, lut, WAVEFORM_COLOR_SMOOTHING_PX)
}

fn render_with_smoothing(peaks: &[Peak], height: u32, lut: &Lut, smoothing_px: usize) -> Vec<u8> {
    let width = peaks.len();
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let stride = width * 4;
    let mut buffer: Vec<u8> = lut.colors[0].repeat(width * height as usize);

    let centre = (height / 2) as usize;
    let centre_colour = lut.color(CENTRE_LINE_LEVEL);
    for pixel in buffer[centre * stride..][..stride].chunks_exact_mut(4) {
        pixel.copy_from_slice(&centre_colour);
    }

    let columns: Vec<Option<Column>> = peaks.iter().map(|&p| column_for(p, height)).collect();
    let levels = smoothed(&columns, smoothing_px, |c| c.level);
    for (x, column) in columns.iter().enumerate() {
        let Some(column) = column else { continue };
        let envelope_level = levels[x] * ENVELOPE_SHARE;
        let body_level = (levels[x] + BODY_LIFT).min(1.0);
        for y in column.envelope.0..=column.envelope.1 {
            let in_body = column.body.is_some_and(|b| (b.0..=b.1).contains(&y));
            let level = if in_body { body_level } else { envelope_level };
            let colour = lut.color(level * fade(distance_from_centre(y, height, column)));
            let at = y as usize * stride + x * 4;
            buffer[at..at + 4].copy_from_slice(&colour);
        }
    }
    buffer
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/analysis/waveform.rs"]
mod tests;
