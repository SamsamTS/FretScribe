//! Plucked-string (Karplus-Strong) voices driven by project time.
//!
//! Each voice is a delay line closed by a two-point averager and a first-order allpass, so that
//! the loop delay equals `rate / freq` exactly and the pitch stays in tune with the recording.

use fretscribe_core::{midi_to_freq, PitchRef};

const MAX_VOICES: usize = 24;
const RELEASE_SEC: f32 = 0.03;
const MIN_NOTE_SEC: f64 = 0.03;
const T60_SEC: f32 = 2.5;
const AMPLITUDE: f32 = 0.35;
const MIN_FREQ_HZ: f32 = 20.0;
const NOISE_SEED: u32 = 0x9E37_79B9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynthNote {
    pub midi: u8,
    pub start: f64,
    pub end: f64,
}

struct Voice {
    active: bool,
    delay: Vec<f32>,
    len: usize,
    idx: usize,
    prev: f32,
    coeff: f32,
    ap_in: f32,
    ap_out: f32,
    loss: f32,
    env: f32,
    releasing: bool,
    end: f64,
    started: u64,
    wait: usize,
}

impl Voice {
    fn new(capacity: usize) -> Self {
        Self {
            active: false,
            delay: vec![0.0; capacity],
            len: 2,
            idx: 0,
            prev: 0.0,
            coeff: 0.0,
            ap_in: 0.0,
            ap_out: 0.0,
            loss: 0.0,
            env: 0.0,
            releasing: false,
            end: 0.0,
            started: 0,
            wait: 0,
        }
    }

    fn render(&mut self, out: &mut [f32], release_at: usize, release_step: f32) {
        let wait = std::mem::take(&mut self.wait);
        for (k, sample) in out.iter_mut().enumerate().skip(wait) {
            if k >= release_at {
                self.releasing = true;
            }
            if self.releasing {
                self.env -= release_step;
                if self.env <= 0.0 {
                    self.active = false;
                    return;
                }
            }
            let x = self.delay[self.idx];
            let averaged = self.loss * 0.5 * (x + self.prev);
            self.prev = x;
            let y = self.coeff * averaged + self.ap_in - self.coeff * self.ap_out;
            self.ap_in = averaged;
            self.ap_out = y;
            self.delay[self.idx] = y;
            self.idx += 1;
            if self.idx == self.len {
                self.idx = 0;
            }
            *sample += y * self.env;
        }
    }
}

pub struct NoteSynth {
    rate: f32,
    pitch: PitchRef,
    notes: Vec<SynthNote>,
    next: usize,
    voices: Vec<Voice>,
    noise: u32,
    clock: u64,
}

impl NoteSynth {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let capacity = (rate / MIN_FREQ_HZ) as usize + 8;
        Self {
            rate,
            pitch: PitchRef::default(),
            notes: Vec::new(),
            next: 0,
            voices: (0..MAX_VOICES).map(|_| Voice::new(capacity)).collect(),
            noise: NOISE_SEED,
            clock: 0,
        }
    }

    /// Invalid entries are dropped; the rest is sorted by start time.
    pub fn set_notes(&mut self, notes: impl IntoIterator<Item = (u8, f64, f64)>) {
        self.notes = notes
            .into_iter()
            .filter(|&(midi, start, end)| midi < 128 && start.is_finite() && end.is_finite())
            .map(|(midi, start, end)| SynthNote {
                midi,
                start,
                end: end.max(start + MIN_NOTE_SEC),
            })
            .collect();
        self.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
    }

    pub fn set_pitch_ref(&mut self, pitch: PitchRef) {
        self.pitch = pitch;
    }

    pub fn release_all(&mut self) {
        for voice in &mut self.voices {
            voice.releasing = true;
        }
    }

    /// Notes starting before `time_sec` are considered already played.
    pub fn seek(&mut self, time_sec: f64) {
        self.next = self.notes.partition_point(|n| n.start < time_sec);
    }

    /// Adds the voices for the project-time window `[from, to)` to `out`, which spans the window.
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

        while let Some(note) = self.notes.get(self.next).copied() {
            if note.start >= to {
                break;
            }
            self.next += 1;
            if note.start >= from {
                self.trigger(note, offset_of(note.start));
            }
        }

        let release_step = 1.0 / (RELEASE_SEC * self.rate);
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            let release_at = if voice.releasing {
                0
            } else if voice.end < to {
                offset_of(voice.end).max(voice.wait)
            } else {
                usize::MAX
            };
            voice.render(out, release_at, release_step);
        }
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    fn trigger(&mut self, note: SynthNote, wait: usize) {
        self.clock += 1;
        let slot = match self.voices.iter().position(|v| !v.active) {
            Some(i) => i,
            None => self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.started)
                .map_or(0, |(i, _)| i),
        };

        let freq = midi_to_freq(f32::from(note.midi), &self.pitch).clamp(MIN_FREQ_HZ, self.rate / 6.0);
        let loop_len = self.rate / freq;
        let capacity = self.voices[slot].delay.len();
        let whole = ((loop_len - 1.0).floor() as usize).clamp(2, capacity);
        let fraction = loop_len - 0.5 - whole as f32;
        let loss = 10f32.powf(-3.0 / (freq * T60_SEC)).min(0.9999);

        let mut low = 0.0;
        let mut mean = 0.0;
        for k in 0..whole {
            low = 0.6 * low + 0.4 * self.next_noise();
            self.voices[slot].delay[k] = low;
            mean += low;
        }
        mean /= whole as f32;
        let voice = &mut self.voices[slot];
        for sample in &mut voice.delay[..whole] {
            *sample = (*sample - mean) * AMPLITUDE * 2.0;
        }

        voice.active = true;
        voice.len = whole;
        voice.idx = 0;
        voice.prev = 0.0;
        voice.coeff = (1.0 - fraction) / (1.0 + fraction);
        voice.ap_in = 0.0;
        voice.ap_out = 0.0;
        voice.loss = loss;
        voice.env = 1.0;
        voice.releasing = false;
        voice.end = note.end;
        voice.started = self.clock;
        voice.wait = wait;
    }

    /// Offline rendering of a whole note list as a mono signal.
    pub fn render_offline(
        notes: &[(u8, f64, f64)],
        sample_rate: u32,
        pitch: PitchRef,
        seconds: f64,
    ) -> Vec<f32> {
        const BLOCK: usize = 64;
        let mut synth = Self::new(sample_rate);
        synth.set_pitch_ref(pitch);
        synth.set_notes(notes.iter().copied());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pitch_by_autocorrelation(x: &[f32], rate: u32, low: f64, high: f64) -> f64 {
        let lag_min = (f64::from(rate) / high) as usize;
        let lag_max = (f64::from(rate) / low) as usize;
        let corr = |lag: usize| -> f64 {
            x.iter().zip(&x[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum()
        };
        let best = (lag_min..=lag_max)
            .max_by(|&a, &b| corr(a).total_cmp(&corr(b)))
            .unwrap();
        let (y0, y1, y2) = (corr(best - 1), corr(best), corr(best + 1));
        let shift = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
        f64::from(rate) / (best as f64 + shift)
    }

    fn cents(measured: f64, expected: f64) -> f64 {
        1200.0 * (measured / expected).log2()
    }

    #[test]
    fn notes_are_in_tune_for_every_reference_pitch() {
        for (a4, reference) in [(440.0, PitchRef::default()), (432.0, PitchRef::new(432.0).unwrap())] {
            for (midi, ratio) in [(45u8, 0.25), (57, 0.5), (69, 1.0), (81, 2.0)] {
                let audio = NoteSynth::render_offline(&[(midi, 0.0, 1.0)], 48_000, reference, 1.0);
                let expected = a4 * ratio;
                let measured = pitch_by_autocorrelation(&audio[2_000..8_000], 48_000, expected * 0.8, expected * 1.25);
                assert!(cents(measured, expected).abs() < 3.0, "{midi} {measured} {expected}");
            }
        }
    }

    #[test]
    fn plucks_start_on_time_and_stop_after_release() {
        let audio = NoteSynth::render_offline(&[(60, 0.5, 0.75)], 48_000, PitchRef::default(), 1.5);
        let first = audio.iter().position(|s| s.abs() > 1e-4).unwrap();
        assert!((first as i64 - 24_000).abs() <= 64, "{first}");
        let tail = &audio[(0.75 * 48_000.0) as usize + 2_000..];
        assert!(tail.iter().all(|s| s.abs() < 1e-6));
        assert!(audio.iter().all(|s| s.abs() < 1.0));
    }

    #[test]
    fn invalid_notes_are_dropped_and_overlaps_are_polyphonic() {
        let mut synth = NoteSynth::new(48_000);
        synth.set_notes([(200, 0.0, 1.0), (60, f64::NAN, 1.0), (60, 0.2, 0.1), (64, 0.0, 1.0)]);
        assert_eq!(synth.notes.len(), 2);
        let chord = NoteSynth::render_offline(&[(60, 0.0, 1.0), (64, 0.0, 1.0), (67, 0.0, 1.0)], 48_000, PitchRef::default(), 0.5);
        let single = NoteSynth::render_offline(&[(60, 0.0, 1.0)], 48_000, PitchRef::default(), 0.5);
        let energy = |x: &[f32]| x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
        assert!(energy(&chord) > 2.0 * energy(&single));
    }

    #[test]
    fn many_simultaneous_notes_steal_the_oldest_voice() {
        let notes: Vec<(u8, f64, f64)> = (0..40).map(|i| (40 + i as u8, 0.0, 1.0)).collect();
        let audio = NoteSynth::render_offline(&notes, 48_000, PitchRef::default(), 0.2);
        assert!(audio.iter().any(|s| s.abs() > 0.01));
        assert!(audio.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn seek_skips_earlier_notes() {
        let mut synth = NoteSynth::new(48_000);
        synth.set_notes([(60, 0.1, 1.0), (62, 0.5, 1.0)]);
        synth.seek(0.3);
        let mut out = vec![0.0; 64];
        synth.render(0.0, 0.0013, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        synth.render(0.5, 0.5013, &mut out);
        assert!(out.iter().any(|s| *s != 0.0));
    }
}
