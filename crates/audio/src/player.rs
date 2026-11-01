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
use crate::engine::TrackState;
use crate::error::{AudioError, Result};
use crate::parallel::parallel_map;
use crate::resample::PreparedAudio;

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

/// A track whose audio is already prepared for this player's output rate (see
/// `PreparedAudio::new`), so installing it is cheap.
#[derive(Clone, Debug)]
pub struct PreparedTrack {
    pub id: u64,
    pub audio: PreparedAudio,
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
    cache: HashMap<usize, (Arc<DecodedAudio>, PreparedAudio)>,
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

fn ensure_unique(ids: impl Iterator<Item = u64> + Clone) -> Result<()> {
    for (i, id) in ids.clone().enumerate() {
        if ids.clone().take(i).any(|earlier| earlier == id) {
            return Err(AudioError::DuplicateTrack(id));
        }
    }
    Ok(())
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

    /// The rate `PreparedAudio::new` must be given, so a worker thread can prepare without
    /// holding the player.
    pub fn output_rate(&self) -> u32 {
        self.rate
    }

    /// Replaces the whole mix. Tracks are resampled to the output rate here (once per decoded
    /// file, in parallel), which can take seconds for long files: from a UI thread prefer
    /// `PreparedAudio::new` on a worker followed by `set_prepared_tracks`. Tracks that disappear
    /// are cut off without a fade; surviving ones keep their gain state.
    pub fn set_tracks(&self, tracks: Vec<TrackAudio>) -> Result<()> {
        ensure_unique(tracks.iter().map(|t| t.id))?;
        let keys: Vec<usize> = tracks.iter().map(|t| Arc::as_ptr(&t.audio) as usize).collect();

        let mut missing: Vec<usize> = Vec::new();
        {
            let state = self.state();
            for (i, key) in keys.iter().enumerate() {
                if !state.cache.contains_key(key) && !missing.iter().any(|&j| keys[j] == *key) {
                    missing.push(i);
                }
            }
        }
        let rate = self.rate;
        let prepared = parallel_map(&missing, |&i| PreparedAudio::new(&tracks[i].audio, rate));

        let mut state = self.state();
        for (&i, result) in missing.iter().zip(prepared) {
            state.cache.insert(keys[i], (Arc::clone(&tracks[i].audio), result?));
        }
        let mut ready = Vec::with_capacity(tracks.len());
        for (track, key) in tracks.iter().zip(&keys) {
            let Some((_, audio)) = state.cache.get(key) else {
                return Err(AudioError::Resample("prepared audio went missing".to_owned()));
            };
            ready.push(PreparedTrack {
                id: track.id,
                audio: audio.clone(),
                offset_sec: track.offset_sec,
                volume: track.volume,
                muted: track.muted,
                solo: track.solo,
            });
        }
        state.cache.retain(|key, _| keys.contains(key));
        self.install(state, ready);
        Ok(())
    }

    /// Replaces the whole mix with already prepared tracks. Cheap enough for any thread. The
    /// cache `set_tracks` keeps is dropped, since these tracks are owned by the caller.
    pub fn set_prepared_tracks(&self, tracks: Vec<PreparedTrack>) -> Result<()> {
        ensure_unique(tracks.iter().map(|t| t.id))?;
        if let Some(track) = tracks.iter().find(|t| t.audio.sample_rate() != self.rate) {
            return Err(AudioError::RateMismatch {
                expected: self.rate,
                found: track.audio.sample_rate(),
            });
        }
        let mut state = self.state();
        state.cache.clear();
        self.install(state, tracks);
        Ok(())
    }

    fn install(&self, mut state: MutexGuard<'_, State>, tracks: Vec<PreparedTrack>) {
        let mut states = Vec::with_capacity(tracks.len());
        let mut infos = Vec::with_capacity(tracks.len());
        for track in tracks {
            let offset = clean_offset(track.offset_sec);
            infos.push(TrackInfo {
                id: track.id,
                duration_sec: track.audio.duration_sec(),
                offset_sec: offset,
            });
            states.push(TrackState::new(
                track.id,
                Arc::clone(track.audio.shared()),
                offset,
                clamp_volume(track.volume),
                track.muted,
                track.solo,
                self.rate,
            ));
        }
        state.tracks = infos;
        drop(state);
        self.send(Command::SetTracks(states));
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::testutil::*;

    fn prepared(id: u64, audio: &Arc<DecodedAudio>) -> PreparedTrack {
        PreparedTrack {
            id,
            audio: PreparedAudio::new(audio, RATE).unwrap(),
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
        }
    }

    #[test]
    fn installing_prepared_tracks_is_fast_for_long_audio() {
        let (player, _engine) = Player::headless(RATE);
        let audio = mono_audio(RATE, dc(380.0, 0.1));
        let tracks = vec![prepared(1, &audio), prepared(2, &audio)];
        let best = (0..3)
            .map(|_| {
                let started = Instant::now();
                player.set_prepared_tracks(tracks.clone()).unwrap();
                started.elapsed()
            })
            .min()
            .unwrap();
        assert!(best < Duration::from_millis(50), "{best:?}");
        assert!((player.duration_sec() - 380.0).abs() < 1e-6);
    }

    #[test]
    fn preparing_on_another_thread_sounds_identical_to_set_tracks() {
        let source: Vec<f32> = sine(330.0, 44_100, 2.0, 0.4);
        let audio = mono_audio(44_100, source);

        let (blocking, mut blocking_engine) = Player::headless(RATE);
        blocking
            .set_tracks(vec![TrackAudio {
                id: 4,
                audio: Arc::clone(&audio),
                offset_sec: 0.25,
                volume: 0.8,
                muted: false,
                solo: false,
            }])
            .unwrap();

        let (swapped, mut swapped_engine) = Player::headless(RATE);
        let rate = swapped.output_rate();
        let worker_audio = Arc::clone(&audio);
        let handle = std::thread::spawn(move || PreparedAudio::new(&worker_audio, rate));
        let ready = handle.join().unwrap().unwrap();
        swapped
            .set_prepared_tracks(vec![PreparedTrack {
                id: 4,
                audio: ready,
                offset_sec: 0.25,
                volume: 0.8,
                muted: false,
                solo: false,
            }])
            .unwrap();

        assert_eq!(blocking.duration_sec(), swapped.duration_sec());
        blocking.play();
        swapped.play();
        let a = render_sec(&blocking, &mut blocking_engine, 1.0);
        let b = render_sec(&swapped, &mut swapped_engine, 1.0);
        assert_eq!(a.left, b.left);
        assert!(rms(&a.left[SKIP..]) > 0.1);
    }

    const SKIP: usize = 24_000;

    #[test]
    fn unchanged_tracks_are_never_prepared_again() {
        let (player, _engine) = Player::headless(RATE);
        let first = mono_audio(44_100, sine(220.0, 44_100, 0.5, 0.3));
        let second = mono_audio(44_100, sine(330.0, 44_100, 0.5, 0.3));
        let mix = |audio: &Arc<DecodedAudio>, id| TrackAudio {
            id,
            audio: Arc::clone(audio),
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
        };
        let cached_ptr = |player: &Player, audio: &Arc<DecodedAudio>| {
            let state = player.state();
            let (_, prepared) = state.cache.get(&(Arc::as_ptr(audio) as usize)).unwrap();
            Arc::as_ptr(prepared.shared())
        };

        player.set_tracks(vec![mix(&first, 1)]).unwrap();
        let before = cached_ptr(&player, &first);
        player.set_tracks(vec![mix(&first, 1), mix(&second, 2), mix(&second, 3)]).unwrap();
        assert_eq!(cached_ptr(&player, &first), before);
        assert_eq!(player.state().cache.len(), 2);

        player.set_tracks(vec![mix(&second, 2)]).unwrap();
        assert_eq!(player.state().cache.len(), 1);
    }

    #[test]
    fn prepared_tracks_are_validated() {
        let (player, _engine) = Player::headless(RATE);
        let audio = mono_audio(RATE, dc(1.0, 0.1));
        let duplicate = player.set_prepared_tracks(vec![prepared(3, &audio), prepared(3, &audio)]);
        assert!(matches!(duplicate, Err(AudioError::DuplicateTrack(3))));

        let other_rate = mono_audio(44_100, dc(1.0, 0.1));
        let wrong = PreparedTrack {
            audio: PreparedAudio::new(&other_rate, 44_100).unwrap(),
            ..prepared(1, &audio)
        };
        assert!(matches!(
            player.set_prepared_tracks(vec![wrong]),
            Err(AudioError::RateMismatch { expected: RATE, found: 44_100 })
        ));
        assert_eq!(player.duration_sec(), 0.0);
    }

    #[test]
    fn the_state_lock_is_not_held_while_preparing() {
        let (player, _engine) = Player::headless(RATE);
        let long = mono_audio(44_100, vec![0.1; 120 * 44_100]);
        let player = Arc::new(player);
        let loader = {
            let player = Arc::clone(&player);
            std::thread::spawn(move || {
                player
                    .set_tracks(vec![TrackAudio {
                        id: 1,
                        audio: long,
                        offset_sec: 0.0,
                        volume: 1.0,
                        muted: false,
                        solo: false,
                    }])
                    .unwrap();
            })
        };
        let mut slowest = Duration::ZERO;
        while !loader.is_finished() {
            let started = Instant::now();
            let _ = player.duration_sec();
            slowest = slowest.max(started.elapsed());
            std::thread::sleep(Duration::from_millis(1));
        }
        loader.join().unwrap();
        assert!(slowest < Duration::from_millis(100), "{slowest:?}");
        assert!((player.duration_sec() - 120.0).abs() < 1e-3);
    }
}
