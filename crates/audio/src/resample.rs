use rubato::{FftFixedIn, Resampler};

use crate::decode::DecodedAudio;
use crate::error::{AudioError, Result};

const CHUNK_FRAMES: usize = 1024;

/// Stereo, interleaved, at the output sample rate. Mono is duplicated; with more than two
/// channels only the first two are kept.
pub(crate) struct Prepared {
    samples: Vec<f32>,
}

impl Prepared {
    pub fn from_decoded(audio: &DecodedAudio, rate: u32) -> Result<Self> {
        let (left, right) = split_stereo(audio);
        let (left, right) = if audio.sample_rate == rate {
            (left, right)
        } else {
            resample_pair(&left, &right, audio.sample_rate, rate)?
        };
        Ok(Self::from_channels(&left, &right))
    }

    pub fn from_channels(left: &[f32], right: &[f32]) -> Self {
        let samples = left.iter().zip(right).flat_map(|(&l, &r)| [l, r]).collect();
        Self { samples }
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn frame(&self, index: i64) -> [f32; 2] {
        match usize::try_from(index) {
            Ok(i) if i < self.frames() => [self.samples[2 * i], self.samples[2 * i + 1]],
            _ => [0.0; 2],
        }
    }

    /// Copies from frame `start` (may be negative or past the end); outside the audio is silence.
    pub fn fill(&self, start: i64, out: &mut [f32]) {
        out.fill(0.0);
        let want = (out.len() / 2) as i64;
        let from = start.max(0);
        let to = (start + want).min(self.frames() as i64);
        if from >= to {
            return;
        }
        let src = &self.samples[2 * from as usize..2 * to as usize];
        let dst = 2 * (from - start) as usize;
        out[dst..dst + src.len()].copy_from_slice(src);
    }

    /// Catmull-Rom interpolation; exact at integer positions.
    pub fn cubic(&self, pos: f64) -> [f32; 2] {
        let floor = pos.floor();
        let t = (pos - floor) as f32;
        let i = floor as i64;
        if t == 0.0 {
            return self.frame(i);
        }
        let (p0, p1, p2, p3) = (
            self.frame(i - 1),
            self.frame(i),
            self.frame(i + 1),
            self.frame(i + 2),
        );
        let mut out = [0.0; 2];
        for c in 0..2 {
            let (a, b, cc, d) = (p0[c], p1[c], p2[c], p3[c]);
            out[c] = 0.5
                * (2.0 * b
                    + (cc - a) * t
                    + (2.0 * a - 5.0 * b + 4.0 * cc - d) * t * t
                    + (3.0 * b - a - 3.0 * cc + d) * t * t * t);
        }
        out
    }
}

fn split_stereo(audio: &DecodedAudio) -> (Vec<f32>, Vec<f32>) {
    let width = usize::from(audio.channels);
    match width {
        1 => (audio.interleaved.clone(), audio.interleaved.clone()),
        _ => audio
            .interleaved
            .chunks_exact(width)
            .map(|frame| (frame[0], frame[1]))
            .unzip(),
    }
}

fn resample_err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Resample(e.to_string())
}

fn resample_pair(left: &[f32], right: &[f32], from: u32, to: u32) -> Result<(Vec<f32>, Vec<f32>)> {
    let mut resampler =
        FftFixedIn::<f32>::new(from as usize, to as usize, CHUNK_FRAMES, 2, 2).map_err(resample_err)?;
    let frames = left.len();
    let delay = resampler.output_delay();
    let expected = (frames as f64 * f64::from(to) / f64::from(from)).round() as usize;
    let mut out = [
        Vec::with_capacity(expected + delay + CHUNK_FRAMES),
        Vec::with_capacity(expected + delay + CHUNK_FRAMES),
    ];
    fn append(out: &mut [Vec<f32>; 2], chunk: Vec<Vec<f32>>) {
        for (dst, src) in out.iter_mut().zip(chunk) {
            dst.extend(src);
        }
    }

    let mut pos = 0;
    loop {
        let need = resampler.input_frames_next();
        if pos + need > frames {
            break;
        }
        let input = [&left[pos..pos + need], &right[pos..pos + need]];
        append(&mut out, resampler.process(&input, None).map_err(resample_err)?);
        pos += need;
    }
    if pos < frames {
        let input = [&left[pos..], &right[pos..]];
        let chunk = resampler.process_partial(Some(&input), None).map_err(resample_err)?;
        append(&mut out, chunk);
    }
    while out[0].len() < expected + delay {
        let chunk = resampler
            .process_partial(None::<&[Vec<f32>]>, None)
            .map_err(resample_err)?;
        append(&mut out, chunk);
    }

    let [mut l, mut r] = out;
    for channel in [&mut l, &mut r] {
        channel.drain(..delay);
        channel.truncate(expected);
    }
    Ok((l, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (std::f64::consts::TAU * freq * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    pub(crate) fn zero_crossing_freq(x: &[f32], rate: u32) -> f64 {
        let mut crossings = Vec::new();
        for i in 1..x.len() {
            if x[i - 1] < 0.0 && x[i] >= 0.0 {
                crossings.push(i as f64 - 1.0 + f64::from(-x[i - 1]) / f64::from(x[i] - x[i - 1]));
            }
        }
        let n = crossings.len() - 1;
        f64::from(rate) * n as f64 / (crossings[n] - crossings[0])
    }

    #[test]
    fn resampling_keeps_length_and_pitch() {
        let mono = sine(440.0, 44_100, 44_100);
        let audio = DecodedAudio::from_interleaved(44_100, 1, mono).unwrap();
        let prepared = Prepared::from_decoded(&audio, 48_000).unwrap();
        assert_eq!(prepared.frames(), 48_000);
        let left: Vec<f32> = (0..48_000).map(|i| prepared.frame(i)[0]).collect();
        assert!((zero_crossing_freq(&left[2_000..46_000], 48_000) - 440.0).abs() < 0.2);
        let peak = left[2_000..46_000].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 0.02, "{peak}");
    }

    #[test]
    fn resampling_stays_aligned_with_the_source() {
        let mut mono = vec![0.0; 44_100];
        mono[22_050] = 1.0;
        let audio = DecodedAudio::from_interleaved(44_100, 1, mono).unwrap();
        let prepared = Prepared::from_decoded(&audio, 48_000).unwrap();
        let loudest = (0..prepared.frames() as i64)
            .max_by(|&a, &b| prepared.frame(a)[0].abs().total_cmp(&prepared.frame(b)[0].abs()))
            .unwrap();
        assert!((loudest - 24_000).abs() <= 1, "{loudest}");
    }

    #[test]
    fn channel_layouts() {
        let mono = DecodedAudio::from_interleaved(48_000, 1, vec![0.25, 0.5]).unwrap();
        let p = Prepared::from_decoded(&mono, 48_000).unwrap();
        assert_eq!(p.frame(1), [0.5, 0.5]);
        let surround = DecodedAudio::from_interleaved(48_000, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        let p = Prepared::from_decoded(&surround, 48_000).unwrap();
        assert_eq!((p.frame(0), p.frame(1)), ([1.0, 2.0], [4.0, 5.0]));
    }

    #[test]
    fn fill_pads_with_silence_and_cubic_is_exact_on_samples() {
        let p = Prepared::from_channels(&[1.0, 2.0, 3.0], &[-1.0, -2.0, -3.0]);
        let mut out = [9.0; 10];
        p.fill(-1, &mut out);
        assert_eq!(out, [0.0, 0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 0.0, 0.0]);
        let mut out = [9.0; 4];
        p.fill(10, &mut out);
        assert_eq!(out, [0.0; 4]);
        assert_eq!(p.cubic(1.0), [2.0, -2.0]);

        let ramp = Prepared::from_channels(&[0.0, 1.0, 2.0, 3.0, 4.0], &[0.0, -1.0, -2.0, -3.0, -4.0]);
        let mid = ramp.cubic(2.25);
        assert!((mid[0] - 2.25).abs() < 1e-5 && (mid[1] + 2.25).abs() < 1e-5);
    }
}
