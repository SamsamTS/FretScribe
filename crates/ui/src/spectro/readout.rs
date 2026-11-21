//! Pitch guide and hover readout logic (pure, no drawing).

use egui::{vec2, Pos2, Rect, Vec2};
use fretscribe_core::{cents_off, midi_to_freq, midi_to_name, PitchRef, Tuning};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

const CURSOR_GAP: f32 = 14.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    pub pitch: String,
    pub candidate: Option<String>,
}

/// `midi_float` is under `pitch_ref`, so the cents and the frequency follow the chosen reference.
pub fn readout(midi_float: f32, pitch_ref: &PitchRef, tuning: &Tuning, capo: u8) -> Readout {
    let hz = midi_to_freq(midi_float, pitch_ref);
    let (nearest, cents) = cents_off(hz, pitch_ref);
    let candidate = lowest_fret_candidate(tuning, capo, nearest).and_then(|(string, fret)| candidate_label(tuning, string, fret));
    Readout { pitch: format!("{} {:+}c, {hz:.1} Hz", midi_to_name(nearest), cents.round() as i32), candidate }
}

pub fn is_natural(midi: i32) -> bool {
    matches!(midi.rem_euclid(12), 0 | 2 | 4 | 5 | 7 | 9 | 11)
}

/// Sounding pitches of the open strings (the capo raises them).
pub fn open_pitches(tuning: &Tuning, capo: u8) -> Vec<i32> {
    tuning.open_strings.iter().map(|&m| i32::from(m) + i32::from(capo)).collect()
}

/// Down-right of the cursor, flipped to the other side where it would overflow, then clamped.
pub fn place_readout(bounds: Rect, cursor: Pos2, size: Vec2) -> Pos2 {
    let mut p = cursor + vec2(CURSOR_GAP, CURSOR_GAP);
    if p.x + size.x > bounds.right() {
        p.x = cursor.x - CURSOR_GAP - size.x;
    }
    if p.y + size.y > bounds.bottom() {
        p.y = cursor.y - CURSOR_GAP - size.y;
    }
    Pos2::new(p.x.min(bounds.right() - size.x).max(bounds.left()), p.y.min(bounds.bottom() - size.y).max(bounds.top()))
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/spectro/readout.rs"]
mod tests;
