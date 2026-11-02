//! Spectrogram window -> RGBA image.
//!
//! Each pixel is a weighted sum of data samples: a bilinear pair when zoomed in, a plain average
//! when a pixel covers more than one sample (per axis). Levels and gamma are applied afterwards,
//! through one table that maps the averaged value straight to a palette colour.

use fretscribe_core::PitchRef;
use rayon::prelude::*;

use crate::display::{apply_gamma, apply_levels, Levels};
use crate::lut::{Lut, LUT_SIZE};
use crate::spectrogram::Spectrogram;

const PALETTE_STEPS: usize = 1024;

struct Taps {
    first: usize,
    len: usize,
    first_weight: f32,
    rest_weight: f32,
}

impl Taps {
    const NONE: Self = Self {
        first: 0,
        len: 0,
        first_weight: 0.0,
        rest_weight: 0.0,
    };

    /// Samples sit at integer positions; a pixel covers `lo..hi` in sample units.
    fn new(lo: f64, hi: f64, count: usize) -> Self {
        if count == 0 {
            return Self::NONE;
        }
        if hi - lo <= 1.0 {
            let centre = (lo + hi) / 2.0;
            if centre < -0.5 || centre > count as f64 - 0.5 {
                return Self::NONE;
            }
            let centre = centre.clamp(0.0, (count - 1) as f64);
            let first = centre.floor() as usize;
            let weight = (centre - first as f64) as f32;
            if first + 1 >= count || weight == 0.0 {
                return Self {
                    first,
                    len: 1,
                    first_weight: 1.0,
                    rest_weight: 0.0,
                };
            }
            return Self {
                first,
                len: 2,
                first_weight: 1.0 - weight,
                rest_weight: weight,
            };
        }
        let first = lo.ceil().max(0.0) as usize;
        let end = (hi.ceil().max(0.0) as usize).min(count);
        if end <= first {
            return Self::NONE;
        }
        let weight = 1.0 / (end - first) as f32;
        Self {
            first,
            len: end - first,
            first_weight: weight,
            rest_weight: weight,
        }
    }

    fn weight(&self, k: usize) -> f32 {
        if k == 0 {
            self.first_weight
        } else {
            self.rest_weight
        }
    }
}

fn palette(lut: &Lut, levels: Levels, gamma: f32) -> Vec<[u8; 4]> {
    (0..PALETTE_STEPS)
        .map(|i| {
            let v = i as f32 / (PALETTE_STEPS - 1) as f32;
            let shaped = apply_gamma(apply_levels(v, levels), gamma);
            lut.colors[(shaped * (LUT_SIZE - 1) as f32).round() as usize]
        })
        .collect()
}

impl Spectrogram {
    /// Row 0 of the image is the highest pitch of `midi_range` (low, high); notes are placed
    /// through `pitch_ref`. Parts of the window outside the data are the weakest colour.
    #[allow(clippy::too_many_arguments)]
    pub fn render_rgba(
        &self,
        time_range_sec: (f64, f64),
        pixel_width: usize,
        pixel_height: usize,
        midi_range: (f32, f32),
        pitch_ref: &PitchRef,
        lut: &Lut,
        levels: Levels,
        gamma: f32,
    ) -> Vec<u8> {
        let mut image = vec![0u8; pixel_width * pixel_height * 4];
        if pixel_width == 0 || pixel_height == 0 {
            return image;
        }
        let palette = palette(lut, levels.clamped(), gamma);

        let (t0, t1) = time_range_sec;
        let frame_start = self.sec_to_frame(t0);
        let frames_per_pixel = (self.sec_to_frame(t1) - frame_start) / pixel_width as f64;
        let columns: Vec<Taps> = (0..pixel_width)
            .map(|x| {
                let lo = frame_start + x as f64 * frames_per_pixel;
                Taps::new(lo, lo + frames_per_pixel, self.n_frames)
            })
            .collect();

        let (low, high) = midi_range;
        let midi_per_pixel = (high - low) / pixel_height as f32;
        let lines: Vec<Taps> = (0..pixel_height)
            .map(|y| {
                let top = high - y as f32 * midi_per_pixel;
                let a = f64::from(self.midi_to_row(top - midi_per_pixel, pitch_ref));
                let b = f64::from(self.midi_to_row(top, pitch_ref));
                Taps::new(a.min(b), a.max(b), self.n_rows)
            })
            .collect();

        image
            .par_chunks_mut(pixel_width * 4)
            .zip(&lines)
            .for_each(|(line, rows)| {
                for (pixel, columns) in line.chunks_exact_mut(4).zip(&columns) {
                    let mut sum = 0.0;
                    for r in 0..rows.len {
                        let samples = &self.row(rows.first + r)[columns.first..][..columns.len];
                        let row_sum: f32 = samples
                            .iter()
                            .enumerate()
                            .map(|(k, v)| v * columns.weight(k))
                            .sum();
                        sum += row_sum * rows.weight(r);
                    }
                    let index = (sum.clamp(0.0, 1.0) * (PALETTE_STEPS - 1) as f32).round() as usize;
                    pixel.copy_from_slice(&palette[index]);
                }
            });
        image
    }
}
