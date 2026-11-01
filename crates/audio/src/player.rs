use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use fretscribe_core::{PitchRef, Track};
use rodio::cpal::traits::HostTrait;
use rodio::{DeviceTrait, OutputStream, OutputStreamHandle, Source};

use crate::decode::DecodedAudio;
use crate::engine::{Command, Engine, Shared, SpeedMode, BLOCK_FRAMES, MAX_SPEED, MIN_SPEED};
use crate::error::{AudioError, Result};
use crate::resample::Prepared;
use crate::engine::TrackState;

/// One audio track of the mix. `project_time = audio_time + offset_sec`, and the offset may be
/// negative (the first seconds of the file are then never heard).
#[derive(Clone, Debug)]
pub struct TrackAudio {
    pub id: u64,
    pub audio: Arc<DecodedAudio>,
    pub offset_sec: f64,
    pub volume: f32,
    pub muted: bool,
    pub solo: bool,
}

struct TrackInfo {
    id: u64,
    duration_sec: f64,
    offset_sec: f64,
}

#[derive(Default)]
struct State {
    /// Keyed by the address of the decoded audio, which the entry keeps alive.
    cache: HashMap<usize, (Arc<DecodedAudio>, Arc<Prepared>)>,
    tracks: Vec<TrackInfo>,
}

struct AudioThread {
    stop: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

/// Transport and mixer front end. Every method takes `&self` and is cheap enough to call from a
/// UI frame: changes are queued to the audio thread and take effect at the next audio block.
pub struct Player {
    rate: u32,
    tx: Sender<Command>,
    shared: Arc<Shared>,
    state: Mutex<State>,
    audio: Option<AudioThread>,
}

fn clamp_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, Track::MAX_VOLUME)
    } else {
        1.0
    }
}

fn clean_offset(offset: f64) -> f64 {
    if offset.is_finite() {
        offset
    } else {
        0.0
    }
}

impl Player {
    /// Opens the default output device. Fails with `NoOutputDevice` when there is none.
    pub fn new() -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (engine_tx, engine_rx) = mpsc::channel::<Engine>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let handle = thread::Builder::new()
            .name("fretscribe-audio".to_owned())
            .spawn(move || audio_thread(ready_tx, engine_rx, stop_rx))?;

        let rate = match ready_rx.recv() {
            Ok(Ok(rate)) => rate,
            Ok(Err(e)) => {
                let _ = handle.join();
                return Err(e);
            }
            Err(_) => return Err(AudioError::Output("the audio thread terminated".to_owned())),
        };
        let (mut player, engine) = Self::build(rate);
        if engine_tx.send(engine).is_err() {
            return Err(AudioError::Output("the audio thread terminated".to_owned()));
        }
        player.audio = Some(AudioThread {
            stop: Some(stop_tx),
            handle: Some(handle),
        });
        Ok(player)
    }

    fn build(rate: u32) -> (Self, Engine) {
        let (tx, rx) = mpsc::channel();
        let shared = Shared::new();
        let engine = Engine::new(rate, rx, Arc::clone(&shared));
        let player = Self {
            rate,
            tx,
            shared,
            state: Mutex::new(State::default()),
            audio: None,
        };
        (player, engine)
    }

    #[cfg(test)]
    pub(crate) fn headless(rate: u32) -> (Self, Engine) {
        Self::build(rate)
    }

    fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Replaces the whole mix. Tracks are resampled to the output rate here (once per decoded
    /// file), so call it from a worker thread for long files. Tracks that disappear are cut off
    /// without a fade; surviving ones keep their gain state.
    pub fn set_tracks(&self, tracks: Vec<TrackAudio>) -> Result<()> {
        for (i, track) in tracks.iter().enumerate() {
            if tracks[..i].iter().any(|t| t.id == track.id) {
                return Err(AudioError::DuplicateTrack(track.id));
            }
        }
        let mut state = self.state();
        let mut states = Vec::with_capacity(tracks.len());
        let mut infos = Vec::with_capacity(tracks.len());
        let mut keep = Vec::with_capacity(tracks.len());
        for track in &tracks {
            let key = Arc::as_ptr(&track.audio) as usize;
            let prepared = match state.cache.get(&key) {
                Some((_, prepared)) => Arc::clone(prepared),
                None => {
                    let prepared = Arc::new(Prepared::from_decoded(&track.audio, self.rate)?);
                    state
                        .cache
                        .insert(key, (Arc::clone(&track.audio), Arc::clone(&prepared)));
                    prepared
                }
            };
            keep.push(key);
            let offset = clean_offset(track.offset_sec);
            infos.push(TrackInfo {
                id: track.id,
                duration_sec: prepared.frames() as f64 / f64::from(self.rate),
                offset_sec: offset,
            });
            states.push(TrackState::new(
                track.id,
                prepared,
                offset,
                clamp_volume(track.volume),
                track.muted,
                track.solo,
                self.rate,
            ));
        }
        state.cache.retain(|key, _| keep.contains(key));
        state.tracks = infos;
        drop(state);
        self.send(Command::SetTracks(states));
        Ok(())
    }

    /// One-track convenience: replaces the mix with `audio` (track id 1) and rewinds.
    pub fn load(&self, audio: Arc<DecodedAudio>) -> Result<()> {
        self.set_tracks(vec![TrackAudio {
            id: 1,
            audio,
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
        }])?;
        self.stop();
        Ok(())
    }

    /// End of the latest track, in project time.
    pub fn duration_sec(&self) -> f64 {
        self.state()
            .tracks
            .iter()
            .map(|t| t.offset_sec + t.duration_sec)
            .fold(0.0, f64::max)
    }

    pub fn play(&self) {
        if self.duration_sec() > 0.0 {
            self.shared.playing.store(true, Ordering::Release);
        }
        self.send(Command::Play);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Release);
        self.send(Command::Pause);
    }

    /// Pauses and returns to the region start (or the beginning without a region).
    pub fn stop(&self) {
        self.shared.playing.store(false, Ordering::Release);
        self.send(Command::Stop);
    }

    pub fn seek(&self, sec: f64) {
        if sec.is_finite() {
            self.send(Command::Seek(sec));
        }
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    /// Project time of the audio block most recently produced; it runs ahead of what the speakers
    /// emit by the device buffer, which the output API does not report.
    pub fn position_sec(&self) -> f64 {
        self.shared.position_sec()
    }

    pub fn set_volume(&self, volume: f32) {
        self.send(Command::SetVolume(clamp_volume(volume)));
    }

    /// Project-time region `(start, end)`. Regions shorter than 20 ms are stretched to that.
    pub fn set_region(&self, region: Option<(f64, f64)>) {
        self.send(Command::SetRegion(region));
    }

    pub fn set_loop(&self, looping: bool) {
        self.send(Command::SetLoop(looping));
    }

    /// Clamped to `[MIN_SPEED, MAX_SPEED]`; non-finite values are ignored.
    pub fn set_playback_speed(&self, speed: f32) {
        if speed.is_finite() {
            self.send(Command::SetSpeed(speed.clamp(MIN_SPEED, MAX_SPEED)));
        }
    }

    pub fn set_speed_mode(&self, mode: SpeedMode) {
        self.send(Command::SetSpeedMode(mode));
    }

    pub fn set_track_volume(&self, id: u64, volume: f32) {
        self.send(Command::TrackVolume(id, clamp_volume(volume)));
    }

    pub fn set_track_mute(&self, id: u64, muted: bool) {
        self.send(Command::TrackMute(id, muted));
    }

    pub fn set_track_solo(&self, id: u64, solo: bool) {
        self.send(Command::TrackSolo(id, solo));
    }

    /// While playing, the track dips to silence for about 20 ms around the change.
    pub fn set_track_offset(&self, id: u64, offset_sec: f64) {
        let offset = clean_offset(offset_sec);
        if let Some(info) = self.state().tracks.iter_mut().find(|t| t.id == id) {
            info.offset_sec = offset;
        }
        self.send(Command::TrackOffset(id, offset));
    }

    /// Smoothed output level (peak, 300 ms release) of every track after mute, solo and volume.
    pub fn track_peak_levels(&self) -> Vec<(u64, f32)> {
        self.shared
            .peaks
            .lock()
            .map(|peaks| peaks.clone())
            .unwrap_or_default()
    }

    /// The transcription as one extra synth channel, independent of the tracks' mute and solo.
    /// Notes are `(midi, start_sec, end_sec)` in project time; `mix` is its level.
    pub fn set_note_overlay(&self, notes: Vec<(u8, f64, f64)>, mix: f32) {
        let mix = if mix.is_finite() { mix.clamp(0.0, 2.0) } else { 0.0 };
        self.send(Command::SetOverlay(notes, mix));
    }

    /// Tuning used by the note overlay, normally the project's reference pitch.
    pub fn set_reference_pitch(&self, pitch: PitchRef) {
        self.send(Command::SetReference(pitch));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        if let Some(mut audio) = self.audio.take() {
            drop(audio.stop.take());
            if let Some(handle) = audio.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn open_output() -> Result<(OutputStream, OutputStreamHandle, u32)> {
    let device = rodio::cpal::default_host()
        .default_output_device()
        .ok_or(AudioError::NoOutputDevice)?;
    let rate = device
        .default_output_config()
        .map_err(|e| AudioError::Output(e.to_string()))?
        .sample_rate()
        .0;
    let (stream, handle) =
        OutputStream::try_from_device(&device).map_err(|e| AudioError::Output(e.to_string()))?;
    Ok((stream, handle, rate))
}

/// The output stream is not `Send` on every platform, so one thread owns it for its whole life.
fn audio_thread(ready: Sender<Result<u32>>, engine: Receiver<Engine>, stop: Receiver<()>) {
    let (stream, handle, rate) = match open_output() {
        Ok(opened) => opened,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if ready.send(Ok(rate)).is_err() {
        return;
    }
    let Ok(engine) = engine.recv() else {
        return;
    };
    if let Err(e) = handle.play_raw(EngineSource::new(engine, rate)) {
        log::error!("cannot start playback: {e}");
        return;
    }
    let _ = stop.recv();
    drop(stream);
}

struct EngineSource {
    engine: Engine,
    block: Vec<f32>,
    next: usize,
    rate: u32,
}

impl EngineSource {
    fn new(engine: Engine, rate: u32) -> Self {
        Self {
            engine,
            block: vec![0.0; 2 * BLOCK_FRAMES],
            next: 2 * BLOCK_FRAMES,
            rate,
        }
    }
}

impl Iterator for EngineSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.next >= self.block.len() {
            if self.engine.shutdown_requested() {
                return None;
            }
            self.engine.render_block(&mut self.block);
            self.next = 0;
        }
        let sample = self.block[self.next];
        self.next += 1;
        Some(sample)
    }
}

impl Source for EngineSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
