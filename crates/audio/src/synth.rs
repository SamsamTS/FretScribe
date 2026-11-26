//! Plucked-string (Karplus-Strong) voices driven by project time, into a simple amp.
//!
//! Each voice is a ring buffer read at a fractional delay (cubic Lagrange, so the delay can move
//! while the string rings) and closed by a weighted two-point averager. The interpolated delay plus
//! the averager's phase delay equals `rate / freq`, which keeps the pitch in tune with the recording.
//! The amp (soft clipping, then a two-pole tone filter) acts on the sum of the voices.

use std::f32::consts::TAU;
use std::sync::Arc;

use fretscribe_core::perform::{value_at, Articulation, Attack, PlayedNote};
use fretscribe_core::{midi_to_freq, PitchRef};

const MAX_VOICES: usize = 24;
const RELEASE_SEC: f32 = 0.03;
const MIN_NOTE_SEC: f64 = 0.03;
const MIN_FREQ_HZ: f32 = 20.0;
const NOISE_SEED: u32 = 0x9E37_79B9;
/// Samples between two evaluations of a pitch curve; the delay is linear in between.
const CONTROL_SAMPLES: usize = 16;
const EXCITATION_RMS: f32 = 0.18;
const FULL_VELOCITY: f32 = 100.0;
const SOFT_BRIGHTNESS: f32 = 0.5;
const SOFT_GAIN: f32 = 0.65;
const MUTED_T60: f32 = 0.15;
const MUTED_BRIGHTNESS: f32 = 0.6;
const MUTED_DAMPING: f32 = 0.5;
const HARMONIC_T60: f32 = 0.6;
const HARMONIC_BRIGHTNESS: f32 = 2.2;
const HARMONIC_DAMPING: f32 = 0.5;
const MAX_DAMPING: f32 = 0.5;
const DEAD_T60: f32 = 0.04;
const DEAD_BRIGHTNESS: f32 = 0.35;
const DEAD_GAIN: f32 = 1.6;
/// Time constant of the amp's parameter changes, so a preset switch does not click.
const AMP_SMOOTH_SEC: f32 = 0.005;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SynthPreset {
    #[default]
    Acoustic,
    ElectricClean,
    Overdrive,
    Distortion,
}

struct Params {
    name: &'static str,
    t60_sec: f32,
    /// Weight of the older sample in the string's averager (0.5 damps most, 0 not at all).
    damping: f32,
    /// Pole of the excitation's low-pass: 1 is white noise.
    brightness: f32,
    drive: f32,
    cutoff_hz: f32,
    level: f32,
}

impl SynthPreset {
    pub fn all() -> [Self; 4] {
        [Self::Acoustic, Self::ElectricClean, Self::Overdrive, Self::Distortion]
    }

    pub fn name(self) -> &'static str {
        self.params().name
    }

    /// Unknown names are Acoustic.
    pub fn from_name(name: &str) -> Self {
        Self::all().into_iter().find(|p| p.name() == name).unwrap_or_default()
    }

    #[rustfmt::skip]
    fn params(self) -> &'static Params {
        const TABLE: [Params; 4] = [
            Params { name: "Acoustic",       t60_sec: 2.5,  damping: 0.5,  brightness: 0.4,  drive: 0.0,  cutoff_hz: f32::INFINITY, level: 1.0 },
            Params { name: "Electric clean", t60_sec: 8.0,  damping: 0.12, brightness: 0.55, drive: 0.0,  cutoff_hz: 5000.0,        level: 0.85 },
            Params { name: "Overdrive",      t60_sec: 9.0,  damping: 0.15, brightness: 0.55, drive: 5.0,  cutoff_hz: 4000.0,        level: 0.25 },
            Params { name: "Distortion",     t60_sec: 10.0, damping: 0.18, brightness: 0.6,  drive: 18.0, cutoff_hz: 3200.0,        level: 0.16 },
        ];
        &TABLE[self as usize]
    }
}

struct Amp {
    preset: SynthPreset,
    smooth: f32,
    drive: f32,
    wet: f32,
    tone: f32,
    level: f32,
    z: [f32; 2],
}

impl Amp {
    /// `(wet, tone coefficient, level)`.
    fn targets(&self, rate: f32) -> (f32, f32, f32) {
        let p = self.preset.params();
        let tone = if p.cutoff_hz < rate / 2.0 { 1.0 - (-TAU * p.cutoff_hz / rate).exp() } else { 1.0 };
        (if p.drive > 0.0 { 1.0 } else { 0.0 }, tone, p.level)
    }

    fn snap(&mut self, rate: f32) {
        (self.wet, self.tone, self.level) = self.targets(rate);
        self.drive = self.preset.params().drive;
        self.z = [0.0; 2];
    }

    fn process(&mut self, rate: f32, out: &mut [f32]) {
        let (wet, tone, level) = self.targets(rate);
        let drive = self.preset.params().drive;
        for sample in out {
            let k = self.smooth;
            self.wet += k * (wet - self.wet);
            self.tone += k * (tone - self.tone);
            if (tone - self.tone).abs() < 1e-4 {
                self.tone = tone;
            }
            self.level += k * (level - self.level);
            // Fading to clean keeps the old drive while the wet share goes down.
            if drive > 0.0 {
                self.drive += k * (drive - self.drive);
            }
            let mut x = *sample;
            if self.wet > 1e-4 {
                x += self.wet * ((self.drive * x).tanh() - x);
            }
            for z in &mut self.z {
                *z += self.tone * (x - *z);
                // Fully open, the filter is exactly the input.
                if self.tone >= 0.9999 {
                    *z = x;
                }
                x = *z;
            }
            *sample = x * self.level;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct StringLoop {
    damping: f32,
    loss: f32,
}

struct Voice {
    note: Option<Arc<PlayedNote>>,
    delay: Vec<f32>,
    write: usize,
    /// Interpolated part of the loop delay, in samples.
    length: f32,
    string: StringLoop,
    prev: f32,
    /// A dead hit: decaying low-passed noise instead of a string.
    burst: Option<(u32, f32)>,
    gain: f32,
    env: f32,
    releasing: bool,
    started: u64,
    wait: usize,
}

fn next_noise(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn delay_length(rate: f32, pitch: &PitchRef, midi: f32, damping: f32) -> f32 {
    let freq = midi_to_freq(midi, pitch).clamp(MIN_FREQ_HZ, rate / 6.0);
    let w = TAU * freq / rate;
    rate / freq - (damping * w.sin()).atan2(1.0 - damping + damping * w.cos()) / w
}

impl Voice {
    fn new(capacity: usize) -> Self {
        Self {
            note: None,
            delay: vec![0.0; capacity],
            write: 0,
            length: 2.0,
            string: StringLoop::default(),
            prev: 0.0,
            burst: None,
            gain: 0.0,
            env: 0.0,
            releasing: false,
            started: 0,
            wait: 0,
        }
    }

    fn string_sample(&mut self) -> f32 {
        let mask = self.delay.len() - 1;
        let whole = self.length as usize - 1;
        let f = self.length - whole as f32;
        let at = |k: usize| self.delay[(self.write + self.delay.len() - whole - k) & mask];
        let (a, b, c) = (f - 1.0, f - 2.0, f - 3.0);
        let x = at(0) * (-a * b * c / 6.0) + at(1) * (f * b * c / 2.0) - at(2) * (f * a * c / 2.0) + at(3) * (f * a * b / 6.0);
        let s = self.string;
        let y = s.loss * ((1.0 - s.damping) * x + s.damping * self.prev);
        self.prev = x;
        self.delay[self.write] = y;
        self.write = (self.write + 1) & mask;
        y
    }

    /// `time_at` maps a sample index of `out` to project time.
    fn render(&mut self, out: &mut [f32], rate: f32, pitch: &PitchRef, time_at: impl Fn(usize) -> f64, release_at: usize) {
        let release_step = 1.0 / (RELEASE_SEC * rate);
        let Some(note) = self.note.clone() else { return };
        let mut slope = 0.0;
        let wait = std::mem::take(&mut self.wait);
        for k in wait..out.len() {
            if self.burst.is_none() && !note.pitch_curve.is_empty() && (k - wait).is_multiple_of(CONTROL_SAMPLES) {
                let ahead = (k + CONTROL_SAMPLES).min(out.len());
                let semis = value_at(&note.pitch_curve, time_at(ahead));
                let target = delay_length(rate, pitch, f32::from(note.midi) + semis, self.string.damping);
                slope = (target - self.length) / (ahead - k) as f32;
            }
            if k >= release_at {
                self.releasing = true;
            }
            if self.releasing {
                self.env -= release_step;
                if self.env <= 0.0 {
                    self.note = None;
                    return;
                }
            }
            self.length += slope;
            let y = match &mut self.burst {
                Some((noise, decay)) => {
                    self.prev += DEAD_BRIGHTNESS * (next_noise(noise) - self.prev);
                    self.gain *= *decay;
                    self.prev
                }
                None => self.string_sample(),
            };
            out[k] += y * self.env * self.gain;
        }
    }
}

pub struct NoteSynth {
    rate: f32,
    pitch: PitchRef,
    notes: Vec<Arc<PlayedNote>>,
    next: usize,
    voices: Vec<Voice>,
    amp: Amp,
    noise: u32,
    clock: u64,
}

impl NoteSynth {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let capacity = ((rate / MIN_FREQ_HZ) as usize + 8).next_power_of_two();
        let mut amp = Amp {
            preset: SynthPreset::default(),
            smooth: 1.0 - (-1.0 / (AMP_SMOOTH_SEC * rate)).exp(),
            drive: 0.0,
            wet: 0.0,
            tone: 1.0,
            level: 1.0,
            z: [0.0; 2],
        };
        amp.snap(rate);
        Self {
            rate,
            pitch: PitchRef::default(),
            notes: Vec::new(),
            next: 0,
            voices: (0..MAX_VOICES).map(|_| Voice::new(capacity)).collect(),
            amp,
            noise: NOISE_SEED,
            clock: 0,
        }
    }

    /// Invalid entries are dropped; the rest is sorted by start time.
    pub fn set_performance(&mut self, notes: impl IntoIterator<Item = PlayedNote>) {
        self.notes = notes
            .into_iter()
            .filter(|n| n.midi < 128 && n.start_sec.is_finite() && n.end_sec.is_finite())
            .map(|mut n| {
                n.end_sec = n.end_sec.max(n.start_sec + MIN_NOTE_SEC);
                Arc::new(n)
            })
            .collect();
        self.notes.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec));
    }

    /// String parameters apply from the next note, the amp glides over a few milliseconds.
    pub fn set_preset(&mut self, preset: SynthPreset) {
        self.amp.preset = preset;
    }

    pub fn set_pitch_ref(&mut self, pitch: PitchRef) {
        self.pitch = pitch;
    }

    pub fn release_all(&mut self) {
        for voice in &mut self.voices {
            voice.releasing = true;
        }
    }

    /// Immediate silence, leaving no tail; for when the output is already faded out.
    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.note = None;
        }
        self.amp.snap(self.rate);
    }

    /// Notes starting before `time_sec` are considered already played.
    pub fn seek(&mut self, time_sec: f64) {
        self.next = self.notes.partition_point(|n| n.start_sec < time_sec);
    }

    /// Writes the project-time window `[from, to)` to `out`, which spans the window and is zeroed.
    pub fn render(&mut self, from: f64, to: f64, out: &mut [f32]) {
        let n = out.len();
        if n == 0 {
            return;
        }
        let span = (to - from).max(0.0);
        let offset_of = |t: f64| -> usize {
            if span <= 0.0 {
                0
            } else {
                (((t - from) / span) * n as f64).clamp(0.0, n as f64) as usize
            }
        };

        while let Some(note) = self.notes.get(self.next).cloned() {
            if note.start_sec >= to {
                break;
            }
            self.next += 1;
            if note.start_sec >= from {
                let wait = offset_of(note.start_sec);
                self.trigger(note, wait);
            }
        }

        for voice in &mut self.voices {
            let Some(end) = voice.note.as_ref().map(|n| n.end_sec) else { continue };
            let release_at = if voice.releasing {
                0
            } else if end < to {
                offset_of(end).max(voice.wait)
            } else {
                usize::MAX
            };
            let time_at = |k: usize| from + span * k as f64 / n as f64;
            voice.render(out, self.rate, &self.pitch, time_at, release_at);
        }
        self.amp.process(self.rate, out);
    }

    fn trigger(&mut self, note: Arc<PlayedNote>, wait: usize) {
        self.clock += 1;
        let slot = match self.voices.iter().position(|v| v.note.is_none()) {
            Some(i) => i,
            None => self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.started)
                .map_or(0, |(i, _)| i),
        };
        let p = self.amp.preset.params();
        let (mut t60, mut damping, mut brightness) = (p.t60_sec, p.damping, p.brightness);
        let mut gain = f32::from(note.velocity) / FULL_VELOCITY;
        match note.kind {
            Articulation::Muted => {
                t60 *= MUTED_T60;
                damping += (MAX_DAMPING - damping) * MUTED_DAMPING;
                brightness *= MUTED_BRIGHTNESS;
            }
            Articulation::Harmonic => {
                t60 *= HARMONIC_T60;
                damping *= HARMONIC_DAMPING;
                brightness = (brightness * HARMONIC_BRIGHTNESS).min(1.0);
            }
            Articulation::Normal | Articulation::Dead => {}
        }
        if note.attack == Attack::Soft {
            brightness *= SOFT_BRIGHTNESS;
            gain *= SOFT_GAIN;
        }

        let midi = f32::from(note.midi) + value_at(&note.pitch_curve, note.start_sec);
        let freq = midi_to_freq(midi, &self.pitch).clamp(MIN_FREQ_HZ, self.rate / 6.0);
        let voice = &mut self.voices[slot];
        voice.length = delay_length(self.rate, &self.pitch, midi, damping);
        voice.string = StringLoop { damping, loss: 10f32.powf(-3.0 / (freq * t60)).min(0.9999) };

        // One period of excitation, repeated over the whole ring so a downward bend always has history.
        let period = ((self.rate / freq).round() as usize).clamp(2, voice.delay.len());
        let mut low = 0.0;
        for k in 0..period {
            low += brightness * (next_noise(&mut self.noise) - low);
            voice.delay[k] = low;
        }
        let mean = voice.delay[..period].iter().sum::<f32>() / period as f32;
        let power = voice.delay[..period].iter().map(|s| (s - mean).powi(2)).sum::<f32>() / period as f32;
        let scale = EXCITATION_RMS / power.sqrt().max(1e-6);
        for sample in &mut voice.delay[..period] {
            *sample = (*sample - mean) * scale;
        }
        for k in period..voice.delay.len() {
            voice.delay[k] = voice.delay[k - period];
        }
        voice.write = 0;
        voice.prev = 0.0;
        voice.burst = (note.kind == Articulation::Dead).then(|| {
            gain *= DEAD_GAIN;
            (self.noise ^ NOISE_SEED.rotate_left(7), 10f32.powf(-3.0 / (self.rate * DEAD_T60)))
        });
        voice.gain = gain;
        voice.env = 1.0;
        voice.releasing = false;
        voice.started = self.clock;
        voice.wait = wait;
        voice.note = Some(note);
    }

    /// Offline rendering of a whole performance as a mono signal.
    pub fn render_offline(
        notes: &[PlayedNote],
        preset: SynthPreset,
        sample_rate: u32,
        pitch: PitchRef,
        seconds: f64,
    ) -> Vec<f32> {
        const BLOCK: usize = 64;
        let mut synth = Self::new(sample_rate);
        synth.set_pitch_ref(pitch);
        synth.set_preset(preset);
        synth.reset();
        synth.set_performance(notes.iter().cloned());
        let total = (seconds * f64::from(sample_rate)).round() as usize;
        let mut out = vec![0.0; total.div_ceil(BLOCK) * BLOCK];
        for (b, block) in out.chunks_mut(BLOCK).enumerate() {
            let from = (b * BLOCK) as f64 / f64::from(sample_rate);
            let to = ((b + 1) * BLOCK) as f64 / f64::from(sample_rate);
            synth.render(from, to, block);
        }
        out.truncate(total);
        out
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/synth.rs"]
mod tests;
