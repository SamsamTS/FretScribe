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
/// Copies interleaved `width`-channel frames into the two planar input buffers (left = first channel).
fn load(input: &mut [Vec<f32>], interleaved: &[f32], width: usize, right: usize) -> Result<()> {
    let [left_in, right_in] = input else {
        return Err(resample_err("unexpected channel count"));
    };
    for ((l, r), frame) in left_in.iter_mut().zip(right_in.iter_mut()).zip(interleaved.chunks_exact(width)) {
        *l = frame[0];
        *r = frame[right];
    }
    Ok(())
}

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
        load(&mut input, &audio.interleaved[pos * width..(pos + need) * width], width, right)?;
        let (_, produced) = resampler
            .process_into_buffer(&input, &mut output, None)
            .map_err(resample_err)?;
        sink.push(&output, produced);
        pos += need;
    }

    if pos < frames {
        let rest = frames - pos;
        load(&mut input, &audio.interleaved[pos * width..], width, right)?;
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

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/resample.rs"]
mod tests;

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/resample_support.rs"]
mod test_support;
