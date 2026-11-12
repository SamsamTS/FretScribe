//! Every frequency <-> note-number conversion in the application goes through `PitchRef`.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

pub const A4_MIDI: f64 = 69.0;
const STANDARD_A4_HZ: f64 = 440.0;
const A4_RANGE_HZ: std::ops::RangeInclusive<f32> = 400.0..=480.0;
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

pub fn midi_to_name(midi: u8) -> String {
    let octave = i32::from(midi / 12) - 1;
    format!("{}{octave}", NOTE_NAMES[usize::from(midi % 12)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(hz: f32) -> PitchRef {
        PitchRef::new(hz).expect("valid reference")
    }

    #[test]
    fn default_is_440() {
        assert_eq!(PitchRef::default().a4_hz(), 440.0);
    }

    #[test]
    fn validation_range() {
        assert!(PitchRef::new(400.0).is_ok());
        assert!(PitchRef::new(480.0).is_ok());
        assert!(PitchRef::new(399.9).is_err());
        assert!(PitchRef::new(480.1).is_err());
        assert!(PitchRef::new(f32::NAN).is_err());
    }

    #[test]
    fn serde_validates() {
        assert_eq!(serde_json::to_string(&r(432.0)).unwrap(), "432.0");
        assert_eq!(serde_json::from_str::<PitchRef>("442.0").unwrap(), r(442.0));
        assert!(serde_json::from_str::<PitchRef>("100.0").is_err());
    }

    #[test]
    fn a4_is_midi_69_in_every_reference() {
        for hz in [440.0, 432.0, 442.0] {
            let p = r(hz);
            assert!((freq_to_midi_float(hz, &p) - 69.0).abs() < 1e-4);
            assert!((midi_to_freq(69.0, &p) - hz).abs() < 1e-3);
            assert_eq!(cents_off(hz, &p).0, 69);
            assert!(cents_off(hz, &p).1.abs() < 1e-2);
        }
    }

    #[test]
    fn middle_c_across_references() {
        assert!((midi_to_freq(60.0, &r(440.0)) - 261.6256).abs() < 1e-3);
        assert!((midi_to_freq(60.0, &r(432.0)) - 256.8687).abs() < 1e-3);
        assert!((midi_to_freq(60.0, &r(442.0)) - 262.8138).abs() < 1e-3);
    }

    #[test]
    fn round_trip_frequency_midi() {
        for hz in [400.0, 432.0, 440.0, 442.0, 480.0] {
            let p = r(hz);
            for midi in [28.0_f32, 40.0, 59.5, 64.0, 88.25] {
                let back = freq_to_midi_float(midi_to_freq(midi, &p), &p);
                assert!((back - midi).abs() < 1e-3, "{hz} {midi} {back}");
            }
        }
    }

    #[test]
    fn cents_against_other_reference() {
        let (midi, cents) = cents_off(440.0, &r(432.0));
        assert_eq!(midi, 69);
        assert!((cents - 31.767).abs() < 0.01, "{cents}");
        let (midi, cents) = cents_off(440.0, &r(442.0));
        assert_eq!(midi, 69);
        assert!((cents + 7.85).abs() < 0.01, "{cents}");
    }

    #[test]
    fn cents_arithmetic() {
        let p = r(440.0);
        let quarter_tone_up = midi_to_freq(60.5, &p);
        let (midi, cents) = cents_off(quarter_tone_up, &p);
        assert!(midi == 60 || midi == 61);
        assert!((cents.abs() - 50.0).abs() < 0.05);
        let (midi, cents) = cents_off(midi_to_freq(60.0, &p) * 2.0_f32.powf(10.0 / 1200.0), &p);
        assert_eq!(midi, 60);
        assert!((cents - 10.0).abs() < 0.02);
    }

    #[test]
    fn cents_off_degenerate_input() {
        let p = PitchRef::default();
        assert_eq!(cents_off(0.0, &p), (0, 0.0));
        assert_eq!(cents_off(-5.0, &p), (0, 0.0));
        assert_eq!(cents_off(f32::NAN, &p), (0, 0.0));
    }

    #[test]
    fn reference_cents_round_trip() {
        assert!((ref_from_cents(0.0).a4_hz() - 440.0).abs() < 1e-4);
        assert!((ref_from_cents(-31.767).a4_hz() - 432.0).abs() < 1e-2);
        assert!((ref_from_cents(7.8).a4_hz() - 442.0).abs() < 1e-1);
        for cents in [-100.0_f32, -31.0, 0.0, 12.5, 100.0] {
            assert!((cents_of(&ref_from_cents(cents)) - cents).abs() < 1e-2);
        }
        assert!(cents_of(&r(432.0)) < 0.0 && cents_of(&r(442.0)) > 0.0);
    }

    #[test]
    fn reference_from_cents_is_clamped() {
        assert_eq!(ref_from_cents(1000.0).a4_hz(), 480.0);
        assert_eq!(ref_from_cents(-1000.0).a4_hz(), 400.0);
    }

    #[test]
    fn names() {
        assert_eq!(midi_to_name(60), "C4");
        assert_eq!(midi_to_name(61), "C#4");
        assert_eq!(midi_to_name(69), "A4");
        assert_eq!(midi_to_name(40), "E2");
        assert_eq!(midi_to_name(0), "C-1");
        assert_eq!(midi_to_name(127), "G9");
    }
}
