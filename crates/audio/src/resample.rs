use std::sync::Arc;

use rubato::{FftFixedIn, Resampler};

use crate::decode::DecodedAudio;
use crate::error::{AudioError, Result};

const CHUNK_FRAMES: usize = 1024;

/// Stereo, interleaved, at the output sample rate. Mono is duplicated; with more than two
/// channels only the first two are kept.
pub(crate) struct Prepared {
    samples: Vec<f32>,
}

/// A decoded file made ready for playback at one output rate. Preparing is the expensive part of
/// loading (resampling), so do it on a worker thread; the handle is cheap to clone and to hand to
/// the player.
#[derive(Clone)]
pub struct PreparedAudio {
    inner: Arc<Prepared>,
    rate: u32,
}

impl std::fmt::Debug for PreparedAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedAudio")
            .field("sample_rate", &self.rate)
            .field("frames", &self.frames())
            .finish()
    }
}

impl PreparedAudio {
    /// `output_rate` must be the player's `sample_rate()`.
    pub fn new(audio: &DecodedAudio, output_rate: u32) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Prepared::from_decoded(audio, output_rate)?),
            rate: output_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub fn frames(&self) -> usize {
        self.inner.frames()
    }

    pub fn duration_sec(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }

    pub(crate) fn shared(&self) -> &Arc<Prepared> {
        &self.inner
    }
}

impl Prepared {
    pub fn from_decoded(audio: &DecodedAudio, rate: u32) -> Result<Self> {
        let samples = if audio.sample_rate == rate {
            to_stereo(audio)
        } else {
            resample_to_stereo(audio, rate)?
        };
        Ok(Self { samples })
    }

    #[cfg(test)]
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

/// One allocation, one pass; stereo input is a plain copy.
fn to_stereo(audio: &DecodedAudio) -> Vec<f32> {
    let width = usize::from(audio.channels);
    if width == 2 {
        return audio.interleaved.clone();
    }
    let right = width.min(2) - 1;
    let mut out = vec![0.0; 2 * audio.frames()];
    for (dst, src) in out.chunks_exact_mut(2).zip(audio.interleaved.chunks_exact(width)) {
        dst[0] = src[0];
        dst[1] = src[right];
    }
    out
}

fn resample_err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Resample(e.to_string())
}

/// Receives resampler output, drops the filter delay at the start and everything past the
/// expected length, and interleaves into the final buffer.
struct Sink {
    out: Vec<f32>,
    skip: usize,
    remaining: usize,
}

impl Sink {
    fn push(&mut self, channels: &[Vec<f32>], produced: usize) {
        let skipped = self.skip.min(produced);
        self.skip -= skipped;
        let take = (produced - skipped).min(self.remaining);
        self.remaining -= take;
        let (left, right) = (&channels[0][skipped..skipped + take], &channels[1][skipped..skipped + take]);
        for (&l, &r) in left.iter().zip(right) {
            self.out.push(l);
            self.out.push(r);
        }
    }
}

/// Streams the source in small chunks so no full-length per-channel buffer ever exists: the only
/// large allocation is the interleaved result.
fn resample_to_stereo(audio: &DecodedAudio, to: u32) -> Result<Vec<f32>> {
    let width = usize::from(audio.channels);
    let right = width.min(2) - 1;
    let frames = audio.frames();
    let mut resampler = FftFixedIn::<f32>::new(audio.sample_rate as usize, to as usize, CHUNK_FRAMES, 2, 2)
        .map_err(resample_err)?;
    let expected = (frames as f64 * f64::from(to) / f64::from(audio.sample_rate)).round() as usize;
    let mut sink = Sink {
        out: Vec::with_capacity(2 * expected),
        skip: resampler.output_delay(),
        remaining: expected,
    };
    let mut input = resampler.input_buffer_allocate(true);
    let mut output = resampler.output_buffer_allocate(true);

    let mut pos = 0;
    loop {
        let need = resampler.input_frames_next();
        if pos + need > frames {
            break;
        }
        let [left, right_channel] = &mut input[..] else {
            return Err(resample_err("unexpected channel count"));
        };
        let chunk = audio.interleaved[pos * width..(pos + need) * width].chunks_exact(width);
        for ((l, r), frame) in left.iter_mut().zip(right_channel.iter_mut()).zip(chunk) {
            *l = frame[0];
            *r = frame[right];
        }
        let (_, produced) = resampler
            .process_into_buffer(&input, &mut output, None)
            .map_err(resample_err)?;
        sink.push(&output, produced);
        pos += need;
    }

    if pos < frames {
        let rest = frames - pos;
        let [left, right_channel] = &mut input[..] else {
            return Err(resample_err("unexpected channel count"));
        };
        let chunk = audio.interleaved[pos * width..].chunks_exact(width);
        for ((l, r), frame) in left.iter_mut().zip(right_channel.iter_mut()).zip(chunk) {
            *l = frame[0];
            *r = frame[right];
        }
        let partial = [&input[0][..rest], &input[1][..rest]];
        let (_, produced) = resampler
            .process_partial_into_buffer(Some(&partial), &mut output, None)
            .map_err(resample_err)?;
        sink.push(&output, produced);
    }

    while sink.remaining > 0 {
        let (_, produced) = resampler
            .process_partial_into_buffer(None::<&[Vec<f32>]>, &mut output, None)
            .map_err(resample_err)?;
        if produced == 0 {
            return Err(resample_err("the resampler stopped producing output"));
        }
        sink.push(&output, produced);
    }
    Ok(sink.out)
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

    /// The original whole-buffer implementation, kept to prove the streaming one is equivalent.
    fn reference_resample(audio: &DecodedAudio, to: u32) -> Vec<f32> {
        let width = usize::from(audio.channels);
        let right = width.min(2) - 1;
        let (left, right): (Vec<f32>, Vec<f32>) = audio
            .interleaved
            .chunks_exact(width)
            .map(|frame| (frame[0], frame[right]))
            .unzip();
        let mut resampler =
            FftFixedIn::<f32>::new(audio.sample_rate as usize, to as usize, CHUNK_FRAMES, 2, 2).unwrap();
        let frames = left.len();
        let delay = resampler.output_delay();
        let expected = (frames as f64 * f64::from(to) / f64::from(audio.sample_rate)).round() as usize;
        let mut out = [Vec::new(), Vec::new()];
        let mut pos = 0;
        while pos + resampler.input_frames_next() <= frames {
            let need = resampler.input_frames_next();
            let input = [&left[pos..pos + need], &right[pos..pos + need]];
            for (dst, src) in out.iter_mut().zip(resampler.process(&input, None).unwrap()) {
                dst.extend(src);
            }
            pos += need;
        }
        if pos < frames {
            let input = [&left[pos..], &right[pos..]];
            for (dst, src) in out.iter_mut().zip(resampler.process_partial(Some(&input), None).unwrap()) {
                dst.extend(src);
            }
        }
        while out[0].len() < expected + delay {
            for (dst, src) in out
                .iter_mut()
                .zip(resampler.process_partial(None::<&[Vec<f32>]>, None).unwrap())
            {
                dst.extend(src);
            }
        }
        let [l, r] = out;
        l[delay..delay + expected]
            .iter()
            .zip(&r[delay..delay + expected])
            .flat_map(|(&a, &b)| [a, b])
            .collect()
    }

    #[test]
    fn streaming_resample_is_identical_to_the_reference_implementation() {
        let cases = [
            (44_100, 48_000, 1, 100_000),
            (44_100, 48_000, 2, 100_001),
            (22_050, 48_000, 2, 5_000),
            (48_000, 44_100, 2, 70_001),
            (96_000, 48_000, 3, 30_000),
            (44_100, 48_000, 2, 700),
            (44_100, 48_000, 2, 1_024),
        ];
        for (from, to, channels, frames) in cases {
            let samples: Vec<f32> = (0..frames * channels)
                .map(|i| ((i * 7919) % 2003) as f32 / 1001.5 - 1.0)
                .collect();
            let audio = DecodedAudio::from_interleaved(from, channels as u16, samples).unwrap();
            let streamed = resample_to_stereo(&audio, to).unwrap();
            let reference = reference_resample(&audio, to);
            assert_eq!(streamed.len(), reference.len(), "{from}->{to} ch{channels} n{frames}");
            let worst = streamed
                .iter()
                .zip(&reference)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(worst < 1e-6, "{from}->{to} ch{channels} n{frames}: {worst}");
        }
    }

    #[test]
    fn same_rate_preparation_is_a_plain_conversion() {
        let stereo = DecodedAudio::from_interleaved(48_000, 2, vec![0.1, 0.2, 0.3, 0.4]).unwrap();
        assert_eq!(to_stereo(&stereo), [0.1, 0.2, 0.3, 0.4]);
        let mono = DecodedAudio::from_interleaved(48_000, 1, vec![0.5, 0.6]).unwrap();
        assert_eq!(to_stereo(&mono), [0.5, 0.5, 0.6, 0.6]);
        let handle = PreparedAudio::new(&stereo, 48_000).unwrap();
        assert_eq!((handle.sample_rate(), handle.frames()), (48_000, 2));
        assert!((handle.duration_sec() - 2.0 / 48_000.0).abs() < 1e-12);
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
