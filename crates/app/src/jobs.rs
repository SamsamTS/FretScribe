//! One-shot background computations. Builders take owned snapshots, never the live store.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use fretscribe_analysis::beatfit::{
    combine_envelopes, default_weight, estimate_tempo, end_fits, fit_measures, onset_envelope,
    EnvelopeInput,
};
use fretscribe_analysis::pitchref::{estimate_reference, PitchRefEstimate, PitchRefOptions};
use fretscribe_analysis::sync::{assume_synced, estimate_offset, SyncCandidate, SyncOptions, DEFAULT_TOLERANCE_MS};
use fretscribe_audio::decode_file;
use fretscribe_core::{Measure, MeasureFit, TrackRole};

const MAX_SHIFT_BEATS: f64 = 0.25;
const TEMPO_RANGE: (f64, f64) = (40.0, 240.0);
pub const THUMBNAIL_MAX: usize = 64;

pub struct Job<T> {
    rx: Option<Receiver<T>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(work());
        });
        Job { rx: Some(rx) }
    }

    /// `Some` once, when the result arrives; a panicking worker just ends the job.
    pub fn poll(&mut self) -> Option<T> {
        match self.rx.as_ref()?.try_recv() {
            Ok(v) => {
                self.rx = None;
                Some(v)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.rx.is_some()
    }
}

/// One fit per measure; the barlines before index `from` and those of locked measures stay.
pub fn auto_fit(inputs: Vec<EnvelopeInput>, measures: Vec<Measure>, from: usize) -> Job<Vec<MeasureFit>> {
    Job::spawn(move || {
        let env = combine_envelopes(&inputs);
        // The fit starts from the grid the defined tempos give, not from an earlier fit's effective tempos, so repeated fits cannot drift away from them.
        let mut nominal = measures.clone();
        for j in from.max(1)..nominal.len() {
            if !nominal[j].locked {
                nominal[j].start_sec = nominal[j - 1].start_sec + nominal[j - 1].nominal_duration_sec();
            }
        }
        let mut fits = fit_measures(&env, &nominal, MAX_SHIFT_BEATS);
        for (i, (fit, m)) in fits.iter_mut().zip(&measures).enumerate() {
            if i <= from || m.locked {
                fit.new_start_sec = m.start_sec;
            }
        }
        end_fits(&measures, &fits)
    })
}

pub fn auto_tempo(inputs: Vec<EnvelopeInput>) -> Job<Vec<(f64, f64)>> {
    Job::spawn(move || estimate_tempo(&combine_envelopes(&inputs), TEMPO_RANGE))
}

pub fn needs_sync(ref_duration: f64, other_duration: f64) -> bool {
    !assume_synced(ref_duration, other_duration, DEFAULT_TOLERANCE_MS)
}

fn resample_linear(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() || from == 0 {
        return x.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    (0..(x.len() as f64 / step) as usize)
        .map(|i| {
            let p = i as f64 * step;
            let (k, f) = (p as usize, (p.fract()) as f32);
            let a = x[k];
            a + (x.get(k + 1).copied().unwrap_or(a) - a) * f
        })
        .collect()
}

pub fn sync(
    reference: (Arc<[f32]>, u32),
    other: (Arc<[f32]>, u32),
    options: SyncOptions<'static>,
) -> Job<Vec<SyncCandidate>> {
    Job::spawn(move || {
        let rate = reference.1.min(other.1);
        let (r, o) = (resample_linear(&reference.0, reference.1, rate), resample_linear(&other.0, other.1, rate));
        estimate_offset(&r, &o, rate, options)
    })
}

pub fn pitch_reference(mono: Arc<[f32]>, rate: u32, region: Option<(f64, f64)>) -> Job<PitchRefEstimate> {
    Job::spawn(move || estimate_reference(&mono, rate, region, PitchRefOptions::default()))
}

/// At most `n` (<= 64) bytes: the normalised peak level of each slice, 0..=255.
pub fn mono_peaks_thumbnail(mono: &[f32], n: usize) -> Vec<u8> {
    let n = n.min(THUMBNAIL_MAX);
    if n == 0 || mono.is_empty() {
        return Vec::new();
    }
    let peaks: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i * mono.len() / n, ((i + 1) * mono.len() / n).max(i * mono.len() / n + 1));
            mono[a..b.min(mono.len())].iter().fold(0.0, |m, s| s.abs().max(m))
        })
        .collect();
    let max = peaks.iter().copied().fold(0.0, f32::max);
    peaks.iter().map(|p| if max > 0.0 { (p / max * 255.0).round() as u8 } else { 0 }).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum EstimateUpdate {
    Bpm(Vec<(f64, f64)>),
    Reference(PitchRefEstimate),
    Duration { path: PathBuf, sec: f64 },
    Failed(String),
    Done,
}

/// New-project form estimates: duration per file, tempo from the beat tracks, A4 from the main
/// (first) file. Stale results of a restarted run are dropped.
pub struct FormEstimates {
    rx: Receiver<EstimateUpdate>,
    cancel: Arc<AtomicBool>,
}

impl FormEstimates {
    pub fn spawn(files: Vec<(PathBuf, TrackRole)>) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        thread::spawn(move || run_estimates(&files, &tx, &flag));
        FormEstimates { rx, cancel }
    }

    pub fn restart(&mut self, files: Vec<(PathBuf, TrackRole)>) {
        *self = Self::spawn(files);
    }

    pub fn poll(&mut self) -> Vec<EstimateUpdate> {
        self.rx.try_iter().collect()
    }
}

impl Drop for FormEstimates {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn run_estimates(files: &[(PathBuf, TrackRole)], tx: &mpsc::Sender<EstimateUpdate>, cancel: &AtomicBool) {
    let uses_beats = |role: TrackRole| matches!(role, TrackRole::Drums | TrackRole::Bass);
    let any_beats = files.iter().any(|f| uses_beats(f.1));
    let send = |u: EstimateUpdate| {
        if !cancel.load(Ordering::Relaxed) {
            let _ = tx.send(u);
        }
    };
    let inputs: Vec<Option<EnvelopeInput>> = thread::scope(|s| {
        let workers: Vec<_> = files
            .iter()
            .enumerate()
            .map(|(i, (path, role))| {
                let send = &send;
                s.spawn(move || {
                    let audio = match decode_file(path) {
                        Ok(a) if !cancel.load(Ordering::Relaxed) => a,
                        Ok(_) => return None,
                        Err(e) => {
                            send(EstimateUpdate::Failed(format!("{}: {e}", path.display())));
                            return None;
                        }
                    };
                    send(EstimateUpdate::Duration { path: path.clone(), sec: audio.duration_sec });
                    if i == 0 {
                        send(EstimateUpdate::Reference(estimate_reference(&audio.mono, audio.sample_rate, None, PitchRefOptions::default())));
                    }
                    (uses_beats(*role) || !any_beats).then(|| EnvelopeInput {
                        env: onset_envelope(&audio.mono, audio.sample_rate),
                        offset_sec: 0.0,
                        weight: default_weight(*role),
                    })
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap_or(None)).collect()
    });
    let inputs: Vec<EnvelopeInput> = inputs.into_iter().flatten().collect();
    if !inputs.is_empty() && !cancel.load(Ordering::Relaxed) {
        send(EstimateUpdate::Bpm(estimate_tempo(&combine_envelopes(&inputs), TEMPO_RANGE)));
    }
    send(EstimateUpdate::Done);
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/jobs.rs"]
mod tests;
