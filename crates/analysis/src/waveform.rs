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
pub const WAVEFORM_GAMMA: f32 = 1.0;

/// Colours are LUT positions. The envelope is the column's loudness level times `ENVELOPE_SHARE`;
/// the RMS body is that level raised by `BODY_LIFT`, so the two stay distinct even at full scale.
const ENVELOPE_SHARE: f32 = 0.8;
const BODY_LIFT: f32 = 0.3;
/// Vertical gradient of both: most intense on the centre line, `EDGE_SCALE` times that at the tips.
const EDGE_SCALE: f32 = 0.4;
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
pub const WAVEFORM_COLOR_SMOOTHING_PX: usize = 15;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn grey_lut() -> Lut {
        let mut lut = Lut::from_stops("Grey", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])]);
        lut.name = "Grey".to_owned();
        lut
    }

    fn pixel(buffer: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let at = (y * width + x) * 4;
        [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]]
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }

    /// Block `k` alternates between `+(k+1)/100` and `-(k+1)/100`.
    fn stepped(blocks: usize) -> Vec<f32> {
        (0..blocks * BASE_BLOCK)
            .map(|i| {
                let amp = ((i / BASE_BLOCK) + 1) as f32 / 100.0;
                if i % 2 == 0 {
                    amp
                } else {
                    -amp
                }
            })
            .collect()
    }

    fn reference(samples: &[f32]) -> Peak {
        let min = samples.iter().copied().fold(f32::INFINITY, f32::min);
        let max = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_sq = samples
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            / samples.len() as f64;
        (min, max, mean_sq.sqrt() as f32)
    }

    fn noise(len: usize) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as f32 / u32::MAX as f32 * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn base_level_min_max_rms_match_a_known_signal() {
        let pyramid = WaveformPyramid::build(&stepped(10), 6400);
        let peaks = pyramid.peaks((0.0, 0.1), 10);
        assert_eq!(peaks.len(), 10);
        for (k, &(min, max, rms)) in peaks.iter().enumerate() {
            let amp = (k + 1) as f32 / 100.0;
            close(min, -amp);
            close(max, amp);
            close(rms, amp);
        }
    }

    #[test]
    fn whole_file_in_one_column_is_exact_for_any_length() {
        for len in [1, 63, 64, 65, 1000, 4097, 100_000] {
            let signal = noise(len);
            let pyramid = WaveformPyramid::build(&signal, 1000);
            let (min, max, rms) = pyramid.peaks((0.0, len as f64 / 1000.0), 1)[0];
            let (rmin, rmax, rrms) = reference(&signal);
            assert_eq!((min, max), (rmin, rmax), "len {len}");
            close(rms, rrms);
        }
    }

    #[test]
    fn coarse_columns_keep_the_global_extremes_and_energy() {
        let signal = noise(50_000);
        let pyramid = WaveformPyramid::build(&signal, 1000);
        let (rmin, rmax, _) = reference(&signal);
        for width in [2, 7, 100, 333, 2000] {
            let peaks = pyramid.peaks((0.0, 50.0), width);
            assert_eq!(peaks.len(), width);
            let min = peaks.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let max = peaks.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            assert_eq!((min, max), (rmin, rmax), "width {width}");
            assert!(peaks.iter().all(|p| p.0 <= p.2 && p.2 <= p.1.max(-p.0)));
        }
    }

    #[test]
    fn zooming_in_past_one_block_never_leaves_gaps() {
        let pyramid = WaveformPyramid::build(&stepped(4), 6400);
        let peaks = pyramid.peaks((0.0, 0.04), 400);
        assert!(peaks.iter().all(|p| p.1 > 0.0 && p.0 < 0.0));
        close(peaks[0].1, 0.01);
        close(peaks[399].1, 0.04);
    }

    #[test]
    fn ranges_outside_the_audio_are_silent_columns() {
        let signal = vec![0.5; 100];
        let pyramid = WaveformPyramid::build(&signal, 100);
        let silent = (0.0, 0.0, 0.0);

        let before = pyramid.peaks((-1.0, 1.0), 4);
        assert_eq!(&before[..2], [silent; 2]);
        assert!(before[2..].iter().all(|p| p.1 == 0.5));

        let after = pyramid.peaks((0.5, 1.5), 4);
        assert!(after[..2].iter().all(|p| p.1 == 0.5));
        assert_eq!(&after[2..], [silent; 2]);

        assert_eq!(pyramid.peaks((5.0, 6.0), 3), [silent; 3]);
        assert_eq!(pyramid.peaks((-9.0, -8.0), 3), [silent; 3]);
    }

    #[test]
    fn zero_width_and_invalid_ranges() {
        let pyramid = WaveformPyramid::build(&noise(1000), 1000);
        assert!(pyramid.peaks((0.0, 1.0), 0).is_empty());
        let silent = (0.0, 0.0, 0.0);
        assert_eq!(pyramid.peaks((1.0, 1.0), 2), [silent; 2]);
        assert_eq!(pyramid.peaks((2.0, 1.0), 2), [silent; 2]);
        assert_eq!(pyramid.peaks((f64::NAN, 1.0), 2), [silent; 2]);
        assert_eq!(pyramid.peaks((0.0, f64::INFINITY), 2), [silent; 2]);
    }

    #[test]
    fn empty_input_and_zero_sample_rate_are_inert() {
        let empty = WaveformPyramid::build(&[], 44_100);
        assert_eq!(empty.duration_sec(), 0.0);
        assert_eq!(empty.peaks((0.0, 1.0), 3), [(0.0, 0.0, 0.0); 3]);
        let no_rate = WaveformPyramid::build(&[0.5; 10], 0);
        assert_eq!(no_rate.duration_sec(), 0.0);
        assert_eq!(no_rate.peaks((0.0, 1.0), 2), [(0.0, 0.0, 0.0); 2]);
    }

    #[test]
    fn non_finite_samples_count_as_silence() {
        let signal = [f32::NAN, 0.5, f32::INFINITY, -0.25];
        let (min, max, rms) = WaveformPyramid::build(&signal, 4).peaks((0.0, 1.0), 1)[0];
        assert_eq!((min, max), (-0.25, 0.5));
        assert!(rms.is_finite());
    }

    #[test]
    fn duration_follows_length_and_rate() {
        let pyramid = WaveformPyramid::build(&vec![0.0; 22_050], 44_100);
        close(pyramid.duration_sec() as f32, 0.5);
        assert_eq!(pyramid.sample_rate(), 44_100);
    }

    #[test]
    fn ten_minutes_of_audio_builds_and_stays_exact() {
        let rate = 44_100;
        let len = 600 * rate as usize;
        let signal: Vec<f32> = (0..len).map(|i| (i % 1000) as f32 / 1000.0 - 0.5).collect();
        let pyramid = WaveformPyramid::build(&signal, rate);
        let (min, max, _) = pyramid.peaks((0.0, 600.0), 1)[0];
        assert_eq!((min, max), (-0.5, 0.499));
        assert_eq!(pyramid.peaks((100.0, 200.0), 1600).len(), 1600);
    }

    #[test]
    fn render_buffer_size_and_background() {
        let lut = Lut::from_stops("Tint", &[(0.0, [10, 20, 30]), (1.0, [250, 250, 250])]);
        let peaks = [(0.0, 0.0, 0.0); 7];
        let buffer = render_rgba(&peaks, 11, &lut);
        assert_eq!(buffer.len(), 7 * 11 * 4);
        assert_eq!(pixel(&buffer, 7, 3, 0), [10, 20, 30, 255]);
        assert_eq!(pixel(&buffer, 7, 6, 10), [10, 20, 30, 255]);
        assert!(render_rgba(&[], 10, &lut).is_empty());
        assert!(render_rgba(&peaks, 0, &lut).is_empty());
    }

    #[test]
    fn silence_leaves_only_background_and_centre_line() {
        let lut = grey_lut();
        let (width, height) = (5, 9);
        let buffer = render_rgba(&vec![(0.0, 0.0, 0.0); width], height, &lut);
        let centre = lut.color(CENTRE_LINE_LEVEL);
        for y in 0..height as usize {
            for x in 0..width {
                let expected = if y == height as usize / 2 {
                    centre
                } else {
                    lut.colors[0]
                };
                assert_eq!(pixel(&buffer, width, x, y), expected, "({x}, {y})");
            }
        }
        assert_ne!(centre, lut.colors[0]);
    }

    #[test]
    fn full_scale_reaches_top_and_bottom_rows() {
        let lut = grey_lut();
        for height in [8, 9, 300] {
            let buffer = render_rgba(&[(-1.0, 1.0, 0.7), (0.0, 0.0, 0.0)], height, &lut);
            let last = height as usize - 1;
            assert_ne!(pixel(&buffer, 2, 0, 0), lut.colors[0], "height {height}");
            assert_ne!(pixel(&buffer, 2, 0, last), lut.colors[0], "height {height}");
            assert_eq!(pixel(&buffer, 2, 1, 0), lut.colors[0]);
        }
    }

    #[test]
    fn envelope_is_limited_to_min_max_and_the_centre_is_brighter() {
        let lut = grey_lut();
        let height = 100;
        let buffer = render_rgba(&[(-0.5, 0.5, 0.2)], height, &lut);
        let at = |y| pixel(&buffer, 1, 0, y)[0];
        assert_eq!(at(0), 0);
        assert_eq!(at(24), 0);
        assert!(at(26) > 0 && at(73) > 0);
        assert_eq!(at(76), 0);
        assert_eq!(at(99), 0);
        assert!(at(50) > at(30), "centre {} tip {}", at(50), at(30));
    }

    #[test]
    fn louder_columns_are_brighter() {
        let lut = grey_lut();
        let buffer = render_with_smoothing(&[(-0.2, 0.2, 0.1), (-0.8, 0.8, 0.1)], 100, &lut, 0);
        let top_of = |x: usize| (0..100).find(|&y| pixel(&buffer, 2, x, y)[0] > 0).unwrap();
        assert!(top_of(1) < top_of(0));
        assert!(pixel(&buffer, 2, 1, top_of(1))[0] > pixel(&buffer, 2, 0, top_of(0))[0]);
    }

    #[test]
    fn garbage_peaks_do_not_panic() {
        let lut = grey_lut();
        let peaks = [
            (f32::NAN, f32::INFINITY, -3.0),
            (5.0, -5.0, 9.0),
            (0.3, 0.1, 0.0),
        ];
        let buffer = render_rgba(&peaks, 16, &lut);
        assert_eq!(buffer.len(), 3 * 16 * 4);
    }

    #[test]
    fn renders_with_the_repository_palettes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT");
        for lut in crate::lut::discover_luts(&dir) {
            let buffer = render_rgba(&[(-0.9, 0.9, 0.4)], 64, &lut);
            assert_eq!(pixel(&buffer, 1, 0, 0), lut.colors[0]);
            assert_ne!(pixel(&buffer, 1, 0, 3), lut.colors[0]);
            assert_ne!(pixel(&buffer, 1, 0, 3), pixel(&buffer, 1, 0, 32));
            assert_eq!(buffer[3], 255);
        }
    }

    fn drawn_mask(buffer: &[u8], lut: &Lut) -> Vec<bool> {
        buffer
            .chunks_exact(4)
            .map(|p| p != lut.colors[0].as_slice())
            .collect()
    }

    fn alternating(columns: usize) -> Vec<Peak> {
        (0..columns)
            .map(|x| {
                if x % 2 == 0 {
                    (-0.9, 0.9, 0.4)
                } else {
                    (-0.3, 0.3, 0.1)
                }
            })
            .collect()
    }

    #[test]
    fn smoothing_changes_colours_but_not_the_contour() {
        let lut = grey_lut();
        let peaks = alternating(60);
        let sharp = render_with_smoothing(&peaks, 80, &lut, 0);
        let smooth = render_with_smoothing(&peaks, 80, &lut, 15);
        assert_ne!(sharp, smooth);
        assert_eq!(drawn_mask(&sharp, &lut), drawn_mask(&smooth, &lut));
    }

    #[test]
    fn smoothing_evens_out_neighbouring_columns() {
        let lut = grey_lut();
        let peaks = alternating(60);
        let row = 40;
        let spread = |smoothing| {
            let buffer = render_with_smoothing(&peaks, 80, &lut, smoothing);
            let a = i32::from(pixel(&buffer, 60, 30, row)[0]);
            let b = i32::from(pixel(&buffer, 60, 31, row)[0]);
            (a, b)
        };
        let (a, b) = spread(0);
        assert!(a > 0 && b > 0 && (a - b).abs() > 40, "{a} {b}");
        let (a, b) = spread(15);
        assert!((a - b).abs() <= 20, "{a} {b}");
    }

    #[test]
    fn smoothing_of_zero_or_one_pixel_is_the_identity() {
        let lut = grey_lut();
        let peaks = alternating(20);
        let off = render_with_smoothing(&peaks, 50, &lut, 0);
        assert_eq!(render_with_smoothing(&peaks, 50, &lut, 1), off);
    }

    #[test]
    fn silence_does_not_dim_the_edges_of_a_note() {
        let lut = grey_lut();
        let mut peaks = vec![(0.0, 0.0, 0.0); 20];
        peaks.extend(vec![(-0.6, 0.6, 0.3); 20]);
        peaks.extend(vec![(0.0, 0.0, 0.0); 20]);
        let sharp = render_with_smoothing(&peaks, 60, &lut, 0);
        let smooth = render_with_smoothing(&peaks, 60, &lut, 15);
        assert_eq!(sharp, smooth);
    }

    #[test]
    fn default_smoothing_applies_to_render_rgba() {
        let lut = grey_lut();
        let peaks = alternating(60);
        assert_eq!(
            render_rgba(&peaks, 80, &lut),
            render_with_smoothing(&peaks, 80, &lut, WAVEFORM_COLOR_SMOOTHING_PX)
        );
    }
    #[test]
    fn envelope_and_body_each_fade_from_the_centre_to_their_tips() {
        let lut = grey_lut();
        let buffer = render_with_smoothing(&[(-0.8, 0.8, 0.4)], 100, &lut, 0);
        let at = |y: usize| i32::from(pixel(&buffer, 1, 0, y)[0]);
        // Body rows 30..=69, envelope-only rows 10..=29 and 70..=89.
        assert!((30..49).all(|y| at(y) <= at(y + 1)), "upper body");
        assert!((50..69).all(|y| at(y) >= at(y + 1)), "lower body");
        assert!((10..29).all(|y| at(y) <= at(y + 1)), "upper envelope");
        assert!((70..89).all(|y| at(y) >= at(y + 1)), "lower envelope");
        assert!(at(49) > at(30) + 40 && at(50) > at(69) + 40);
        assert!(at(29) > at(10) + 20 && at(70) > at(89) + 20);
        assert!((10..=89).all(|y| at(y) == at(99 - y)));
    }

    #[test]
    fn body_and_envelope_stay_distinct_where_they_meet() {
        let lut = grey_lut();
        for (peak, rms) in [(0.8, 0.4), (1.0, 0.5), (0.3, 0.1)] {
            let buffer = render_with_smoothing(&[(-peak, peak, rms)], 200, &lut, 0);
            let at = |y: usize| i32::from(pixel(&buffer, 1, 0, y)[0]);
            // Inside a part, neighbouring rows differ by a few levels; the body edge is a step.
            let step = (1..100).map(|y| at(y) - at(y - 1)).max().unwrap_or(0);
            assert!(step > 15, "peak {peak} rms {rms}: largest step {step}");
        }
    }

    #[test]
    fn one_sided_columns_do_not_misbehave() {
        let lut = grey_lut();
        let buffer = render_rgba(&[(0.2, 0.7, 0.4), (-0.7, -0.2, 0.4)], 64, &lut);
        assert_eq!(buffer.len(), 2 * 64 * 4);
    }
}
