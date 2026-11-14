//! Background loading of audio tracks into the `AudioStore`. Workers decode and analyse; the
//! caller's `poll` installs results and applies streamed spectrogram chunks within a time budget.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fretscribe_analysis::beatfit::{detect_onsets, onset_envelope};
use fretscribe_analysis::{compute_spectrogram_streaming, Spectrogram, SpectrogramChunk, SpectrogramParams, SpectrogramStream, WaveformPyramid};
use fretscribe_audio::{decode_file, PreparedAudio};

use crate::store::{AudioStore, TrackData};

const POLL_BUDGET: Duration = Duration::from_millis(4);
/// Share of a track's progress taken by decoding and analysis; the spectrogram takes the rest.
const READY_SHARE: f32 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub enum LoadEvent {
    Progress { fraction: f32, label: String },
    TrackReady { id: u64, duration_sec: f64 },
    SpectrogramUpdated { id: u64 },
    SpectrogramDone { id: u64 },
    TrackFailed { id: u64, error: String },
    Finished,
}

enum Msg {
    Ready(u64, Box<TrackData>, SpectrogramStream),
    Failed(u64, String),
}

struct Streaming {
    id: u64,
    chunks: Receiver<SpectrogramChunk>,
    frames: usize,
    done: usize,
}

pub struct LoadJob {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    total: usize,
    waiting: usize,
    finished_tracks: usize,
    streams: Vec<Streaming>,
    last_fraction: f32,
    finished: bool,
}

impl LoadJob {
    /// One worker per track; `output_rate` is the player's `output_rate()`.
    pub fn spawn(tracks: Vec<(u64, PathBuf)>, output_rate: u32) -> LoadJob {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let total = tracks.len();
        for (id, path) in tracks {
            let (tx, cancel) = (tx.clone(), cancel.clone());
            thread::spawn(move || {
                let msg = match load_track(&path, output_rate, &cancel) {
                    Ok(Some((data, stream))) => Msg::Ready(id, Box::new(data), stream),
                    Ok(None) => return,
                    Err(error) => Msg::Failed(id, error),
                };
                let _ = tx.send(msg);
            });
        }
        LoadJob { rx, cancel, total, waiting: total, finished_tracks: 0, streams: Vec::new(), last_fraction: -1.0, finished: false }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn poll(&mut self, store: &mut AudioStore) -> Vec<LoadEvent> {
        let mut events = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(Msg::Ready(id, data, stream)) => {
                    self.waiting -= 1;
                    let mut data = *data;
                    data.spectrogram = Spectrogram::empty(&stream.info);
                    events.push(LoadEvent::TrackReady { id, duration_sec: data.duration_sec });
                    store.tracks.insert(id, data);
                    store.revision += 1;
                    self.streams.push(Streaming { id, chunks: stream.chunks, frames: stream.info.n_frames.max(1), done: 0 });
                }
                Ok(Msg::Failed(id, error)) => {
                    self.waiting -= 1;
                    self.finished_tracks += 1;
                    events.push(LoadEvent::TrackFailed { id, error });
                }
                Err(_) => break,
            }
        }
        self.apply_chunks(store, &mut events);
        if !self.finished && self.waiting == 0 && self.streams.is_empty() {
            self.finished = true;
            events.push(LoadEvent::Progress { fraction: 1.0, label: "Done".into() });
            events.push(LoadEvent::Finished);
        } else if !self.finished {
            let fraction = self.fraction();
            if fraction != self.last_fraction {
                self.last_fraction = fraction;
                let label = if self.waiting > 0 { "Decoding and analysing audio" } else { "Computing spectrograms" };
                events.push(LoadEvent::Progress { fraction, label: label.into() });
            }
        }
        events
    }

    fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        let ready = (self.total - self.waiting - self.streams.len() - self.finished_tracks) as f32;
        let streaming: f32 = self.streams.iter().map(|s| READY_SHARE + (1.0 - READY_SHARE) * s.done as f32 / s.frames as f32).sum();
        (ready + self.finished_tracks as f32 + streaming) / self.total as f32
    }

    fn apply_chunks(&mut self, store: &mut AudioStore, events: &mut Vec<LoadEvent>) {
        let start = Instant::now();
        let mut changed = false;
        for stream in &mut self.streams {
            let mut touched = false;
            let mut ended = false;
            while start.elapsed() < POLL_BUDGET || !touched {
                match stream.chunks.try_recv() {
                    Ok(chunk) => {
                        if let Some(t) = store.tracks.get_mut(&stream.id) {
                            t.spectrogram.apply_chunk(&chunk);
                        }
                        stream.done = stream.done.max(chunk.start_frame + chunk.n_frames);
                        touched = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        ended = true;
                        break;
                    }
                }
            }
            if touched {
                changed = true;
                events.push(LoadEvent::SpectrogramUpdated { id: stream.id });
            }
            if ended {
                stream.frames = 0;
                if let Some(t) = store.tracks.get_mut(&stream.id) {
                    t.spectrogram_ready = true;
                }
                events.push(LoadEvent::SpectrogramDone { id: stream.id });
            }
        }
        if changed {
            store.revision += 1;
        }
        let before = self.streams.len();
        self.streams.retain(|s| s.frames != 0);
        self.finished_tracks += before - self.streams.len();
    }
}

impl Drop for LoadJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// `Ok(None)` when cancelled. The returned data has a placeholder spectrogram, replaced by the poll.
fn load_track(path: &PathBuf, rate: u32, cancel: &AtomicBool) -> Result<Option<(TrackData, SpectrogramStream)>, String> {
    let decoded = decode_file(path).map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let mono: Arc<[f32]> = Arc::from(decoded.mono.as_slice());
    let stream = compute_spectrogram_streaming(mono.clone(), decoded.sample_rate, &SpectrogramParams::default());
    let (prepared, (pyramid, envelope)) = thread::scope(|s| {
        let prepared = s.spawn(|| PreparedAudio::new(&decoded, rate));
        let analysis = (WaveformPyramid::build(&mono, decoded.sample_rate), onset_envelope(&mono, decoded.sample_rate));
        (prepared.join(), analysis)
    });
    let prepared = prepared.map_err(|_| "audio preparation panicked".to_owned())?.map_err(|e| e.to_string())?;
    let onsets = detect_onsets(&envelope);
    let data = TrackData {
        prepared,
        sample_rate: decoded.sample_rate,
        duration_sec: decoded.duration_sec,
        spectrogram: Spectrogram::empty(&stream.info),
        spectrogram_ready: false,
        pyramid: Arc::new(pyramid),
        mono,
        envelope,
        onsets,
    };
    Ok(Some((data, stream)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use fretscribe_ui::LaneSource;
    use std::path::Path;

    pub fn write_wav(path: &Path, rate: u32, samples: &[f32]) {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s * 32767.0) as i16).to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 0x0001_0001, rate, rate * 2, 0x0010_0002] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        std::fs::write(path, out).unwrap();
    }

    /// Decaying noise-free clicks (short sine bursts) every `period` seconds.
    pub fn clicks(rate: u32, seconds: f64, period: f64) -> Vec<f32> {
        (0..(seconds * f64::from(rate)) as usize)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                let since = t % period;
                ((-since * 60.0).exp() * (std::f64::consts::TAU * 1000.0 * t).sin() * 0.8) as f32
            })
            .collect()
    }

    fn run(job: &mut LoadJob, store: &mut AudioStore) -> Vec<LoadEvent> {
        let mut all = Vec::new();
        let start = Instant::now();
        while !job.is_finished() && start.elapsed() < Duration::from_secs(60) {
            all.extend(job.poll(store));
            thread::sleep(Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn loads_two_tracks_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
        write_wav(&a, 22_050, &clicks(22_050, 3.0, 0.5));
        write_wav(&b, 16_000, &clicks(16_000, 2.0, 0.5));
        let mut store = AudioStore::default();
        let mut job = LoadJob::spawn(vec![(1, a), (2, b), (3, dir.path().join("missing.wav"))], 48_000);
        let events = run(&mut job, &mut store);
        assert!(job.is_finished() && events.last() == Some(&LoadEvent::Finished));
        assert!(events.iter().any(|e| matches!(e, LoadEvent::TrackFailed { id: 3, .. })));
        assert!(events.iter().any(|e| matches!(e, LoadEvent::TrackReady { id: 2, .. })));
        assert!(events.iter().any(|e| matches!(e, LoadEvent::SpectrogramDone { id: 1 })));
        assert_eq!(store.tracks.len(), 2);
        assert!(store.tracks.values().all(|t| t.spectrogram_ready));
        assert!(store.revision > 0 && store.peaks(1, (0.0, 3.0), 77).len() == 77);
        assert!((store.duration_sec(1) - 3.0).abs() < 0.01);
        assert!(store.tracks[&1].spectrogram.data.iter().any(|&v| v > 0.0));
        assert!(store.onsets(1).len() >= 4, "{:?}", store.onsets(1));
        assert_eq!(store.tracks[&1].prepared.sample_rate(), 48_000);
        let fractions: Vec<f32> = events.iter().filter_map(|e| if let LoadEvent::Progress { fraction, .. } = e { Some(*fraction) } else { None }).collect();
        assert!(fractions.windows(2).all(|w| w[1] >= w[0]) && fractions.last() == Some(&1.0), "{fractions:?}");
        assert!(job.poll(&mut store).is_empty());
    }

    #[test]
    fn empty_job_finishes() {
        let mut job = LoadJob::spawn(vec![], 48_000);
        let events = job.poll(&mut AudioStore::default());
        assert_eq!(events.last(), Some(&LoadEvent::Finished));
    }
}
