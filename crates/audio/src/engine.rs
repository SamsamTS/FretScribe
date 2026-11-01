//! The real-time mixing engine, driven one block at a time from the audio thread.
//!
//! All transport positions are project frames at the output rate. A `Voice` is a read head over
//! every track: it advances `speed` project frames per output frame, either by interpolating the
//! track directly or, for pitch-preserving slow-down, through one stretcher per track. Anything
//! that would make the output discontinuous (seek, loop wrap, speed or mode change) starts a new
//! voice that is crossfaded in over `XFADE_SEC` while the old one fades out.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use fretscribe_core::PitchRef;

use crate::focus::{FocusBand, FocusFilter};
use crate::ramp::Ramp;
use crate::resample::Prepared;
use crate::stretch::Stretcher;
use crate::synth::NoteSynth;

pub const BLOCK_FRAMES: usize = 64;
pub const MIN_SPEED: f32 = 0.25;
pub const MAX_SPEED: f32 = 2.0;
pub const XFADE_SEC: f64 = 0.005;

const MIN_SUB_CHUNK: usize = 4;
const GAIN_RAMP_SEC: f64 = 0.010;
const MIN_REGION_SEC: f64 = 0.02;
const PEAK_RELEASE_SEC: f64 = 0.3;
const UNITY_EPS: f64 = 1e-6;
const LIMITER_KNEE: f32 = 0.8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeedMode {
    /// Real-time time-stretch: the pitch stays put whatever the speed.
    #[default]
    PreservePitch,
    /// Plain resampling, like a tape: the pitch follows the speed.
    ChangePitch,
}

pub(crate) struct Shared {
    position_bits: AtomicU64,
    pub playing: AtomicBool,
    pub shutdown: AtomicBool,
    pub peaks: Mutex<Vec<(u64, f32)>>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            position_bits: AtomicU64::new(0.0f64.to_bits()),
            playing: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            peaks: Mutex::new(Vec::new()),
        })
    }

    pub fn position_sec(&self) -> f64 {
        f64::from_bits(self.position_bits.load(Ordering::Relaxed))
    }

    fn set_position(&self, sec: f64) {
        self.position_bits.store(sec.to_bits(), Ordering::Relaxed);
    }
}

pub(crate) enum Command {
    SetTracks(Vec<TrackState>),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetRegion(Option<(f64, f64)>),
    SetLoop(bool),
    SetVolume(f32),
    SetSpeed(f32),
    SetSpeedMode(SpeedMode),
    TrackVolume(u64, f32),
    TrackMute(u64, bool),
    TrackSolo(u64, bool),
    TrackOffset(u64, f64),
    SetOverlay(Vec<(u8, f64, f64)>, f32),
    SetReference(PitchRef),
    SetFocus(bool),
    SetFocusStrength(f32),
    SetFocusBand(FocusBand),
}

pub(crate) fn soft_limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= LIMITER_KNEE {
        x
    } else {
        let room = 1.0 - LIMITER_KNEE;
        x.signum() * (LIMITER_KNEE + room * ((magnitude - LIMITER_KNEE) / room).tanh())
    }
}

pub(crate) struct TrackState {
    id: u64,
    audio: Arc<Prepared>,
    offset_frames: f64,
    pending_offset: Option<f64>,
    volume: f32,
    muted: bool,
    solo: bool,
    gain: Ramp,
    peak: f32,
    stretch: [Stretcher; 2],
    primed: [bool; 2],
}

impl TrackState {
    pub fn new(
        id: u64,
        audio: Arc<Prepared>,
        offset_sec: f64,
        volume: f32,
        muted: bool,
        solo: bool,
        rate: u32,
    ) -> Self {
        Self {
            id,
            audio,
            offset_frames: offset_sec * f64::from(rate),
            pending_offset: None,
            volume,
            muted,
            solo,
            gain: Ramp::new(0.0),
            peak: 0.0,
            stretch: [Stretcher::new(rate), Stretcher::new(rate)],
            primed: [false; 2],
        }
    }

    fn end_frames(&self) -> f64 {
        self.offset_frames + self.audio.frames() as f64
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    /// Project frame being heard (stretched voices compensate for the stretcher latency).
    pos: f64,
    speed: f64,
    stretching: bool,
    slot: usize,
    latency: f64,
    /// Stretched voices only: next input frame to feed, and the fractional frame carried over.
    in_pos: f64,
    carry: f64,
}

fn stretches(mode: SpeedMode, speed: f64) -> bool {
    mode == SpeedMode::PreservePitch && (speed - 1.0).abs() > UNITY_EPS
}

impl Voice {
    fn new(pos: f64, speed: f64, mode: SpeedMode, slot: usize, latency: (f64, f64)) -> Self {
        let stretching = stretches(mode, speed);
        let lead = if stretching { latency.0 + latency.1 * speed } else { 0.0 };
        let in_pos = (pos + lead).round();
        Self {
            pos: if stretching { in_pos - lead } else { pos },
            speed,
            stretching,
            slot,
            latency: lead,
            in_pos,
            carry: 0.0,
        }
    }

    fn idle(pos: f64) -> Self {
        Self::new(pos, 1.0, SpeedMode::ChangePitch, 0, (0.0, 0.0))
    }

    /// Input frames a stretched voice consumes for `n` output frames.
    fn plan(&mut self, n: usize) -> usize {
        if !self.stretching {
            return 0;
        }
        self.carry += n as f64 * self.speed;
        let whole = self.carry.floor();
        self.carry -= whole;
        whole as usize
    }

    fn advance(&mut self, n: usize, consumed: usize) {
        if self.stretching {
            self.in_pos += consumed as f64;
            self.pos = self.in_pos - self.latency;
        } else {
            self.pos += n as f64 * self.speed;
        }
    }
}

struct Fade {
    old: Voice,
    left: u32,
    total: u32,
    /// Loop wraps report the region start while the incoming voice is still in its pre-roll.
    floor: f64,
}

struct Scratch {
    input: Vec<f32>,
    preroll: Vec<f32>,
    preroll_frames: usize,
}

fn read_voice(voice: &Voice, track: &mut TrackState, n: usize, consumed: usize, scratch: &mut Scratch, out: &mut [f32]) {
    if !voice.stretching {
        let first = voice.pos - track.offset_frames;
        let last = first + n as f64 * voice.speed;
        if last < -2.0 || first > track.audio.frames() as f64 + 2.0 {
            out.fill(0.0);
            return;
        }
        for (k, frame) in out.chunks_exact_mut(2).enumerate() {
            let s = track.audio.cubic(first + k as f64 * voice.speed);
            frame.copy_from_slice(&s);
        }
        return;
    }
    if consumed == 0 {
        out.fill(0.0);
        return;
    }
    let head = voice.in_pos as i64 - track.offset_frames.round() as i64;
    if !track.primed[voice.slot] {
        let pre = scratch.preroll_frames;
        track.audio.fill(head - pre as i64, &mut scratch.preroll[..2 * pre]);
        track.stretch[voice.slot].prime(&scratch.preroll[..2 * pre], voice.speed);
        track.primed[voice.slot] = true;
    }
    track.audio.fill(head, &mut scratch.input[..2 * consumed]);
    track.stretch[voice.slot].process(&scratch.input[..2 * consumed], out);
}

pub(crate) struct Engine {
    rate: f64,
    xfade: u32,
    ramp_frames: f64,
    peak_block_decay: f32,
    rx: Receiver<Command>,
    shared: Arc<Shared>,
    tracks: Vec<TrackState>,
    cur: Voice,
    fade: Option<Fade>,
    pending_seek: Option<f64>,
    speed: f64,
    mode: SpeedMode,
    region: Option<(f64, f64)>,
    looping: bool,
    running: bool,
    stopping: bool,
    park_at: Option<f64>,
    master: Ramp,
    master_volume: f32,
    latency: (f64, f64),
    scratch: Scratch,
    synth: NoteSynth,
    synth_mix: Ramp,
    focus: FocusFilter,
    jumped: bool,
    mix: Vec<f32>,
    track_buf: Vec<f32>,
    old_buf: Vec<f32>,
    synth_buf: Vec<f32>,
}

impl Engine {
    pub fn new(sample_rate: u32, rx: Receiver<Command>, shared: Arc<Shared>) -> Self {
        let rate = f64::from(sample_rate);
        let probe = Stretcher::new(sample_rate);
        let latency = (probe.input_latency() as f64, probe.output_latency() as f64);
        let preroll_frames = ((2.0 * latency.0) as usize).max(256);
        let peak_decay = (-1.0 / (PEAK_RELEASE_SEC * rate)).exp() as f32;
        Self {
            rate,
            xfade: (XFADE_SEC * rate).round().max(1.0) as u32,
            ramp_frames: GAIN_RAMP_SEC * rate,
            peak_block_decay: peak_decay.powi(BLOCK_FRAMES as i32),
            rx,
            shared,
            tracks: Vec::new(),
            cur: Voice::idle(0.0),
            fade: None,
            pending_seek: None,
            speed: 1.0,
            mode: SpeedMode::default(),
            region: None,
            looping: false,
            running: false,
            stopping: false,
            park_at: None,
            master: Ramp::new(0.0),
            master_volume: 1.0,
            latency,
            scratch: Scratch {
                input: vec![0.0; 2 * (2 * BLOCK_FRAMES + 8)],
                preroll: vec![0.0; 2 * preroll_frames],
                preroll_frames,
            },
            synth: NoteSynth::new(sample_rate),
            synth_mix: Ramp::new(0.0),
            focus: FocusFilter::new(sample_rate, FocusBand::default()),
            jumped: false,
            mix: vec![0.0; 2 * BLOCK_FRAMES],
            track_buf: vec![0.0; 2 * BLOCK_FRAMES],
            old_buf: vec![0.0; 2 * BLOCK_FRAMES],
            synth_buf: vec![0.0; BLOCK_FRAMES],
        }
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shared.shutdown.load(Ordering::Acquire)
    }

    pub fn render_block(&mut self, out: &mut [f32]) {
        debug_assert_eq!(out.len(), 2 * BLOCK_FRAMES);
        while let Ok(command) = self.rx.try_recv() {
            self.apply(command);
        }
        out.fill(0.0);
        for track in &mut self.tracks {
            track.peak *= self.peak_block_decay;
        }

        let mut done = 0;
        while self.running && done < BLOCK_FRAMES {
            self.reconcile();
            if !self.running {
                break;
            }
            let n = self.plan_chunk(BLOCK_FRAMES - done);
            self.render_chunk(n, &mut out[2 * done..2 * (done + n)]);
            done += n;
        }
        self.publish();
    }

    fn publish(&self) {
        self.shared.set_position(self.reported_pos().max(0.0) / self.rate);
        if let Ok(mut peaks) = self.shared.peaks.try_lock() {
            peaks.clear();
            peaks.extend(self.tracks.iter().map(|t| (t.id, t.peak)));
        }
    }

    fn reported_pos(&self) -> f64 {
        match &self.fade {
            Some(fade) => self.cur.pos.max(fade.floor),
            None => self.cur.pos,
        }
    }

    fn duration_frames(&self) -> f64 {
        self.tracks.iter().map(TrackState::end_frames).fold(0.0, f64::max)
    }

    fn bounds(&self) -> (f64, f64) {
        self.region.unwrap_or((0.0, self.duration_frames()))
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::SetTracks(mut tracks) => {
                for track in &mut tracks {
                    if let Some(old) = self.tracks.iter().find(|o| o.id == track.id) {
                        track.gain.cur = old.gain.cur;
                        track.peak = old.peak;
                    }
                }
                self.tracks = tracks;
                self.refresh_gains();
                if !self.running {
                    self.cur.pos = self.cur.pos.min(self.duration_frames());
                }
            }
            Command::Play => self.play(),
            Command::Pause => {
                self.shared.playing.store(false, Ordering::Release);
                if self.running && !self.stopping {
                    self.begin_stop(f64::from(self.xfade), None);
                }
            }
            Command::Stop => {
                self.shared.playing.store(false, Ordering::Release);
                let target = self.bounds().0;
                if self.running {
                    self.begin_stop(f64::from(self.xfade), Some(target));
                } else {
                    self.cur = Voice::idle(target);
                }
            }
            Command::Seek(sec) => {
                let frames = (sec * self.rate).clamp(0.0, self.duration_frames());
                if !self.running {
                    self.cur = Voice::idle(frames);
                } else if self.stopping {
                    self.park_at = Some(frames);
                } else {
                    self.pending_seek = Some(frames);
                }
            }
            Command::SetRegion(region) => self.region = region.and_then(|r| self.sanitize_region(r)),
            Command::SetLoop(looping) => self.looping = looping,
            Command::SetVolume(volume) => {
                self.master_volume = volume;
                if !self.stopping && self.running {
                    self.master.set(volume, f64::from(self.xfade));
                }
            }
            Command::SetSpeed(speed) => {
                self.speed = f64::from(speed);
                self.retune_focus();
            }
            Command::SetSpeedMode(mode) => {
                self.mode = mode;
                self.retune_focus();
            }
            Command::TrackVolume(id, volume) => self.edit_track(id, |t| t.volume = volume),
            Command::TrackMute(id, muted) => self.edit_track(id, |t| t.muted = muted),
            Command::TrackSolo(id, solo) => self.edit_track(id, |t| t.solo = solo),
            Command::TrackOffset(id, sec) => {
                let frames = sec * self.rate;
                let running = self.running;
                self.edit_track(id, |t| {
                    if (frames - t.offset_frames).abs() < UNITY_EPS && t.pending_offset.is_none() {
                        return;
                    }
                    if running {
                        t.pending_offset = Some(frames);
                    } else {
                        t.offset_frames = frames;
                        t.pending_offset = None;
                        t.primed = [false; 2];
                    }
                });
            }
            Command::SetOverlay(notes, mix) => {
                self.synth.set_notes(notes);
                self.synth.seek(self.reported_pos() / self.rate);
                self.synth_mix.set(mix, self.ramp_frames);
            }
            Command::SetReference(pitch) => self.synth.set_pitch_ref(pitch),
            Command::SetFocus(enabled) => self.focus.set_enabled(enabled),
            Command::SetFocusStrength(strength) => self.focus.set_strength(strength),
            Command::SetFocusBand(band) => self.focus.set_band(band),
        }
    }

    /// With ChangePitch the whole spectrum moves with the speed, and so must the band.
    fn retune_focus(&mut self) {
        let scale = if self.mode == SpeedMode::ChangePitch { self.speed } else { 1.0 };
        self.focus.set_speed_scale(scale);
    }

    fn sanitize_region(&self, (a, b): (f64, f64)) -> Option<(f64, f64)> {
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        let (start, end) = (a.min(b).max(0.0) * self.rate, a.max(b).max(0.0) * self.rate);
        Some((start, end.max(start + MIN_REGION_SEC * self.rate)))
    }

    fn edit_track(&mut self, id: u64, edit: impl FnOnce(&mut TrackState)) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            edit(track);
            self.refresh_gains();
        }
    }

    /// The contract's audibility rule: any solo wins over mute, otherwise everything unmuted.
    fn refresh_gains(&mut self) {
        let any_solo = self.tracks.iter().any(|t| t.solo);
        for track in &mut self.tracks {
            let audible = if any_solo { track.solo } else { !track.muted };
            let target = if audible && track.pending_offset.is_none() {
                track.volume
            } else {
                0.0
            };
            track.gain.set(target, self.ramp_frames);
        }
    }

    fn make_voice(&mut self, pos: f64, slot: usize) -> Voice {
        for track in &mut self.tracks {
            track.primed[slot] = false;
        }
        Voice::new(pos, self.speed, self.mode, slot, self.latency)
    }

    fn play(&mut self) {
        if self.running {
            if self.stopping {
                self.stopping = false;
                self.park_at = None;
                self.master.set(self.master_volume, f64::from(self.xfade));
            }
            self.shared.playing.store(true, Ordering::Release);
            return;
        }
        let duration = self.duration_frames();
        if duration <= 0.0 {
            return;
        }
        let (start, end) = self.bounds();
        let start = if self.cur.pos >= start && self.cur.pos < end { self.cur.pos } else { start };
        self.cur = self.make_voice(start, 0);
        self.focus.reset_state();
        self.fade = None;
        self.pending_seek = None;
        self.running = true;
        self.stopping = false;
        self.park_at = None;
        self.jumped = true;
        self.master.snap(0.0);
        self.master.set(self.master_volume, f64::from(self.xfade));
        self.shared.playing.store(true, Ordering::Release);
    }

    fn begin_stop(&mut self, frames: f64, park: Option<f64>) {
        self.stopping = true;
        self.park_at = park;
        self.master.set(0.0, frames);
    }

    fn finish_stop(&mut self) {
        self.running = false;
        self.stopping = false;
        self.fade = None;
        self.pending_seek = None;
        if let Some(pos) = self.park_at.take() {
            self.cur = Voice::idle(pos);
        }
        self.synth.release_all();
        self.focus.reset_state();
        self.shared.playing.store(false, Ordering::Release);
    }

    fn start_fade(&mut self, pos: f64, floor: f64, jumped: bool) {
        let slot = 1 - self.cur.slot;
        let incoming = self.make_voice(pos, slot);
        let old = std::mem::replace(&mut self.cur, incoming);
        self.fade = Some(Fade {
            old,
            left: self.xfade,
            total: self.xfade,
            floor,
        });
        self.jumped |= jumped;
    }

    /// Chunk-boundary housekeeping: pending offsets, stop completion, region end, loop wrap and
    /// any change of voice parameters.
    fn reconcile(&mut self) {
        self.apply_pending_offsets();
        if self.stopping && self.master.cur == 0.0 {
            self.finish_stop();
            return;
        }
        if self.stopping {
            return;
        }

        if self.fade.is_none() {
            let wants_stretch = stretches(self.mode, self.speed);
            let changed = (self.speed - self.cur.speed).abs() > f64::EPSILON || wants_stretch != self.cur.stretching;
            if let Some(target) = self.pending_seek.take() {
                self.start_fade(target, f64::NEG_INFINITY, true);
                return;
            }
            if changed {
                self.start_fade(self.cur.pos, f64::NEG_INFINITY, false);
                return;
            }
        }

        let (start, end) = self.bounds();
        let to_end = (end - self.cur.pos) / self.cur.speed;
        let xfade = f64::from(self.xfade);
        if to_end > xfade {
            return;
        }
        if self.looping {
            if self.fade.is_none() {
                self.start_fade(start - to_end.max(0.0) * self.speed, start, true);
            }
        } else {
            let frames = if to_end >= 8.0 { to_end } else { xfade };
            self.begin_stop(frames, Some(end));
        }
    }

    fn apply_pending_offsets(&mut self) {
        let mut applied = false;
        for track in &mut self.tracks {
            if let Some(offset) = track.pending_offset {
                if track.gain.cur == 0.0 {
                    track.offset_frames = offset;
                    track.pending_offset = None;
                    track.primed = [false; 2];
                    applied = true;
                }
            }
        }
        if applied {
            self.refresh_gains();
        }
    }

    fn plan_chunk(&self, remaining: usize) -> usize {
        let mut n = remaining;
        if let Some(fade) = &self.fade {
            n = n.min(fade.left as usize);
        }
        if !self.stopping {
            let (_, end) = self.bounds();
            let until_trigger = (end - self.cur.pos) / self.cur.speed - f64::from(self.xfade);
            if until_trigger > 0.0 {
                n = n.min(until_trigger.ceil() as usize);
            }
        }
        n = n.max(1);
        if self.cur.stretching {
            n = n.max(MIN_SUB_CHUNK);
        }
        n = n.min(remaining);
        if remaining - n < MIN_SUB_CHUNK {
            n = remaining;
        }
        n
    }

    fn mix_tracks(&mut self, n: usize, consumed_cur: usize, consumed_old: usize) {
        let Engine {
            tracks,
            cur,
            fade,
            scratch,
            mix,
            track_buf,
            old_buf,
            ..
        } = self;
        mix[..2 * n].fill(0.0);
        for track in tracks.iter_mut() {
            if track.gain.cur == 0.0 && track.gain.target == 0.0 {
                track.primed = [false; 2];
                continue;
            }
            read_voice(cur, track, n, consumed_cur, scratch, &mut track_buf[..2 * n]);
            if let Some(fade) = fade.as_ref() {
                read_voice(&fade.old, track, n, consumed_old, scratch, &mut old_buf[..2 * n]);
                let elapsed = (fade.total - fade.left) as usize;
                for k in 0..n {
                    let w = ((elapsed + k + 1) as f32 / fade.total as f32).min(1.0);
                    for c in 0..2 {
                        track_buf[2 * k + c] = old_buf[2 * k + c] * (1.0 - w) + track_buf[2 * k + c] * w;
                    }
                }
            }
            let mut peak = track.peak;
            for k in 0..n {
                let gain = track.gain.next();
                for c in 0..2 {
                    let v = track_buf[2 * k + c] * gain;
                    mix[2 * k + c] += v;
                    peak = peak.max(v.abs());
                }
            }
            track.peak = peak;
        }
    }

    fn render_chunk(&mut self, n: usize, out: &mut [f32]) {
        let from = self.reported_pos() / self.rate;
        if std::mem::take(&mut self.jumped) {
            self.synth.release_all();
            self.synth.seek(from);
        }
        let consumed_cur = self.cur.plan(n);
        let consumed_old = self.fade.as_mut().map_or(0, |f| f.old.plan(n));

        self.mix_tracks(n, consumed_cur, consumed_old);
        self.focus.process(&mut self.mix[..2 * n]);

        self.cur.advance(n, consumed_cur);
        let mut finished = false;
        if let Some(fade) = &mut self.fade {
            fade.old.advance(n, consumed_old);
            fade.left = fade.left.saturating_sub(n as u32);
            finished = fade.left == 0;
        }
        if finished {
            self.fade = None;
        }

        let to = self.reported_pos() / self.rate;
        self.synth_buf[..n].fill(0.0);
        self.synth.render(from, to.max(from), &mut self.synth_buf[..n]);

        for k in 0..n {
            let master = self.master.next();
            let overlay = self.synth_mix.next() * self.synth_buf[k];
            for c in 0..2 {
                out[2 * k + c] = soft_limit((self.mix[2 * k + c] + overlay) * master);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_target_in_the_requested_frames() {
        let mut ramp = Ramp::new(0.0);
        ramp.set(1.0, 100.0);
        let values: Vec<f32> = (0..120).map(|_| ramp.next()).collect();
        assert!((values[49] - 0.5).abs() < 1e-5);
        assert_eq!(values[99], 1.0);
        assert_eq!(values[119], 1.0);
        ramp.set(0.25, 50.0);
        let first = ramp.next();
        assert!((first - (1.0 - 0.75 / 50.0)).abs() < 1e-6);
        let steps = std::iter::repeat_with(|| ramp.next()).take(60).collect::<Vec<_>>();
        assert!(steps.windows(2).all(|w| w[1] <= w[0]));
        assert_eq!(steps[48], 0.25);
        assert_eq!(*steps.last().unwrap(), 0.25);
    }

    #[test]
    fn soft_limiter_is_transparent_bounded_and_monotonic() {
        for x in [-0.8f32, -0.3, 0.0, 0.5, 0.8] {
            assert_eq!(soft_limit(x), x);
        }
        let mut previous = soft_limit(0.8);
        for i in 0..400 {
            let x = 0.8 + i as f32 * 0.05;
            let y = soft_limit(x);
            assert!(y >= previous && y <= 1.0, "{x} {y}");
            assert_eq!(soft_limit(-x), -y);
            previous = y;
        }
    }

    #[test]
    fn direct_voice_counts_samples_exactly() {
        let mut voice = Voice::new(1_000.0, 0.5, SpeedMode::ChangePitch, 0, (2_880.0, 2_000.0));
        assert!(!voice.stretching);
        for _ in 0..1_000 {
            assert_eq!(voice.plan(64), 0);
            voice.advance(64, 0);
        }
        assert_eq!(voice.pos, 1_000.0 + 64_000.0 * 0.5);
    }

    #[test]
    fn stretched_voice_consumes_the_right_number_of_input_frames() {
        for speed in [0.25, 0.5, 0.8, 1.5, 2.0] {
            let mut voice = Voice::new(5_000.0, speed, SpeedMode::PreservePitch, 1, (2_880.0, 2_000.0));
            assert!(voice.stretching);
            let start = voice.pos;
            assert!((start - 5_000.0).abs() <= 0.5);
            let mut total = 0usize;
            let mut frames = 0usize;
            for n in [64, 7, 64, 13, 4, 64, 64] {
                let m = voice.plan(n);
                assert!(m >= 1, "speed {speed} chunk {n}");
                voice.advance(n, m);
                total += m;
                frames += n;
            }
            assert!((total as f64 - frames as f64 * speed).abs() < 1.0);
            assert!((voice.pos - start - frames as f64 * speed).abs() < 1.0);
        }
    }

    #[test]
    fn unity_speed_bypasses_the_stretcher_in_both_modes() {
        assert!(!stretches(SpeedMode::PreservePitch, 1.0));
        assert!(!stretches(SpeedMode::ChangePitch, 0.5));
        assert!(stretches(SpeedMode::PreservePitch, 0.5));
    }
}
