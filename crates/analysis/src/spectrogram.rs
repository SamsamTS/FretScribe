use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use fretscribe_core::{cents_of, PitchRef};
use rayon::prelude::*;

use crate::cqt::{Plan, Scratch};

const BLOCK_FRAMES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Hann,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpectrogramParams {
    /// Constant-Q quality factor: a window lasts about `q / frequency` seconds.
    pub q: f32,
    pub min_window_sec: f32,
    pub max_window_sec: f32,
    pub hop_size: usize,
    pub window: WindowKind,
    pub min_midi: f32,
    pub max_midi: f32,
    pub bins_per_semitone: u8,
    pub db_floor: f32,
    pub db_ceiling: f32,
}

impl Default for SpectrogramParams {
    fn default() -> Self {
        Self {
            q: 40.0,
            min_window_sec: 0.04,
            max_window_sec: 0.55,
            hop_size: 512,
            window: WindowKind::Hann,
            min_midi: 36.0,
            max_midi: 100.0,
            bins_per_semitone: 4,
            db_floor: -90.0,
            db_ceiling: 0.0,
        }
    }
}

impl SpectrogramParams {
    pub(crate) fn sanitized(&self) -> Self {
        let default = Self::default();
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let mut p = self.clone();
        p.q = finite(p.q, default.q).max(0.1);
        p.min_window_sec = finite(p.min_window_sec, default.min_window_sec).max(0.001);
        p.max_window_sec = finite(p.max_window_sec, default.max_window_sec).max(p.min_window_sec);
        p.hop_size = p.hop_size.max(1);
        p.bins_per_semitone = p.bins_per_semitone.max(1);
        p.min_midi = finite(p.min_midi, default.min_midi);
        p.max_midi = finite(p.max_midi, default.max_midi).max(p.min_midi + 1.0);
        p.db_floor = finite(p.db_floor, default.db_floor);
        p.db_ceiling = finite(p.db_ceiling, default.db_ceiling).max(p.db_floor + 1.0);
        p
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpectrogramInfo {
    pub n_frames: usize,
    pub n_rows: usize,
    pub hop_sec: f64,
    pub min_midi: f32,
    pub max_midi: f32,
    pub bins_per_semitone: u8,
}

/// Row-major, row 0 = lowest pitch, values normalised to 0..1 (0 = `db_floor`, 1 = `db_ceiling`).
/// Row `r` is centred on grid pitch `min_midi + r / bins_per_semitone`, where grid pitches are
/// relative to A4 = 440 Hz whatever the project's reference pitch is.
#[derive(Clone, Debug, PartialEq)]
pub struct Spectrogram {
    pub data: Vec<f32>,
    pub n_frames: usize,
    pub n_rows: usize,
    pub hop_sec: f64,
    pub min_midi: f32,
    pub max_midi: f32,
    pub bins_per_semitone: u8,
}

/// Frames `start_frame..start_frame + n_frames` for every row, row-major within the chunk.
#[derive(Clone, Debug)]
pub struct SpectrogramChunk {
    pub start_frame: usize,
    pub n_frames: usize,
    pub data: Vec<f32>,
}

pub struct SpectrogramStream {
    pub info: SpectrogramInfo,
    /// Chunks arrive in time order; dropping the receiver cancels the computation.
    pub chunks: Receiver<SpectrogramChunk>,
}

impl Spectrogram {
    pub fn empty(info: &SpectrogramInfo) -> Self {
        Self {
            data: vec![0.0; info.n_rows * info.n_frames],
            n_frames: info.n_frames,
            n_rows: info.n_rows,
            hop_sec: info.hop_sec,
            min_midi: info.min_midi,
            max_midi: info.max_midi,
            bins_per_semitone: info.bins_per_semitone,
        }
    }

    pub fn apply_chunk(&mut self, chunk: &SpectrogramChunk) {
        if chunk.start_frame + chunk.n_frames > self.n_frames {
            return;
        }
        for row in 0..self.n_rows {
            let source = &chunk.data[row * chunk.n_frames..][..chunk.n_frames];
            self.data[row * self.n_frames + chunk.start_frame..][..chunk.n_frames]
                .copy_from_slice(source);
        }
    }

    pub fn value(&self, row: usize, frame: usize) -> f32 {
        self.data[row * self.n_frames + frame]
    }

    pub fn row(&self, row: usize) -> &[f32] {
        &self.data[row * self.n_frames..][..self.n_frames]
    }

    pub fn frame_to_sec(&self, frame: f64) -> f64 {
        frame * self.hop_sec
    }

    pub fn sec_to_frame(&self, sec: f64) -> f64 {
        sec / self.hop_sec
    }

    /// Note number (under `pitch_ref`) of the pitch a possibly fractional row is centred on.
    pub fn row_to_midi(&self, row: f32, pitch_ref: &PitchRef) -> f32 {
        let grid = self.min_midi + row / f32::from(self.bins_per_semitone);
        grid - cents_of(pitch_ref) / 100.0
    }

    pub fn midi_to_row(&self, midi: f32, pitch_ref: &PitchRef) -> f32 {
        let grid = midi + cents_of(pitch_ref) / 100.0;
        (grid - self.min_midi) * f32::from(self.bins_per_semitone)
    }
}

fn run(plan: &Plan, mono: &[f32], mut sink: impl FnMut(SpectrogramChunk) -> bool) {
    let n_frames = plan.info().n_frames;
    let blocks: Vec<(usize, usize)> = (0..n_frames)
        .step_by(BLOCK_FRAMES)
        .map(|start| (start, (start + BLOCK_FRAMES).min(n_frames)))
        .collect();
    for group in blocks.chunks(rayon::current_num_threads().max(2)) {
        let chunks: Vec<SpectrogramChunk> = group
            .par_iter()
            .map_init(Scratch::default, |scratch, &(start, end)| {
                SpectrogramChunk {
                    start_frame: start,
                    n_frames: end - start,
                    data: plan.compute_block(mono, start, end, scratch),
                }
            })
            .collect();
        for chunk in chunks {
            if !sink(chunk) {
                return;
            }
        }
    }
}

pub fn compute_spectrogram(
    mono: &[f32],
    sample_rate: u32,
    params: &SpectrogramParams,
) -> Spectrogram {
    let plan = Plan::new(params, sample_rate, mono.len());
    let mut spectrogram = Spectrogram::empty(plan.info());
    run(&plan, mono, |chunk| {
        spectrogram.apply_chunk(&chunk);
        true
    });
    spectrogram
}

/// Runs on the rayon pool and returns immediately; the audio is shared, not copied.
pub fn compute_spectrogram_streaming(
    mono: Arc<[f32]>,
    sample_rate: u32,
    params: &SpectrogramParams,
) -> SpectrogramStream {
    let plan = Plan::new(params, sample_rate, mono.len());
    let info = plan.info().clone();
    let (sender, chunks) = mpsc::channel();
    rayon::spawn(move || run(&plan, &mono, |chunk| sender.send(chunk).is_ok()));
    SpectrogramStream { info, chunks }
}
