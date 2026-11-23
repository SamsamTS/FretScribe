//! Every frequency <-> note-number conversion in the application goes through `PitchRef`.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

pub const A4_MIDI: f64 = 69.0;
const STANDARD_A4_HZ: f64 = 440.0;
/// Accepted reference pitch for A4.
pub const A4_RANGE_HZ: std::ops::RangeInclusive<f32> = 400.0..=480.0;
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct PitchRef {
    a4_hz: f32,
}

impl PitchRef {
    pub fn new(a4_hz: f32) -> Result<Self, CoreError> {
        if A4_RANGE_HZ.contains(&a4_hz) {
            Ok(Self { a4_hz })
        } else {
            Err(CoreError::PitchRefOutOfRange(a4_hz))
        }
    }

    pub fn a4_hz(&self) -> f32 {
        self.a4_hz
    }
}

impl Default for PitchRef {
    fn default() -> Self {
        Self { a4_hz: 440.0 }
    }
}

impl TryFrom<f32> for PitchRef {
    type Error = CoreError;

    fn try_from(hz: f32) -> Result<Self, Self::Error> {
        Self::new(hz)
    }
}

impl From<PitchRef> for f32 {
    fn from(r: PitchRef) -> Self {
        r.a4_hz
    }
}

fn midi_float(hz: f64, a4_hz: f64) -> f64 {
    A4_MIDI + 12.0 * (hz / a4_hz).log2()
}

pub fn freq_to_midi_float(hz: f32, pitch_ref: &PitchRef) -> f32 {
    midi_float(f64::from(hz), f64::from(pitch_ref.a4_hz)) as f32
}

pub fn midi_to_freq(midi: f32, pitch_ref: &PitchRef) -> f32 {
    (f64::from(pitch_ref.a4_hz) * ((f64::from(midi) - A4_MIDI) / 12.0).exp2()) as f32
}

/// Non-positive or non-finite frequencies have no pitch and map to `(0, 0.0)`.
pub fn cents_off(hz: f32, pitch_ref: &PitchRef) -> (u8, f32) {
    if !(hz.is_finite() && hz > 0.0) {
        return (0, 0.0);
    }
    let exact = midi_float(f64::from(hz), f64::from(pitch_ref.a4_hz));
    let nearest = exact.round().clamp(0.0, 127.0);
    (nearest as u8, ((exact - nearest) * 100.0) as f32)
}

/// Clamped to the valid reference range (about +-151 cents around 440 Hz).
pub fn ref_from_cents(cents: f32) -> PitchRef {
    let hz = STANDARD_A4_HZ * (f64::from(cents) / 1200.0).exp2();
    let hz = (hz as f32).clamp(*A4_RANGE_HZ.start(), *A4_RANGE_HZ.end());
    PitchRef { a4_hz: hz }
}

pub fn cents_of(pitch_ref: &PitchRef) -> f32 {
    (1200.0 * (f64::from(pitch_ref.a4_hz) / STANDARD_A4_HZ).log2()) as f32
}

/// Pitch class of `midi` ("C", "C#", ...).
pub fn pitch_class_name(midi: u8) -> &'static str {
    NOTE_NAMES[usize::from(midi % 12)]
}

pub fn midi_to_name(midi: u8) -> String {
    let octave = i32::from(midi / 12) - 1;
    format!("{}{octave}", pitch_class_name(midi))
}

/// A MIDI number or a name such as `E2`, `F#3`, `Bb4`.
pub fn parse_pitch(text: &str) -> Option<u8> {
    let text = text.trim();
    if let Ok(n) = text.parse::<u8>() {
        return (n <= 127).then_some(n);
    }
    let mut chars = text.chars();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = match rest.chars().next()? {
        '#' => (1, &rest[1..]),
        'b' => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let midi = (octave.parse::<i32>().ok()? + 1) * 12 + base + shift;
    u8::try_from(midi).ok().filter(|m| *m <= 127)
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/pitch.rs"]
mod tests;
