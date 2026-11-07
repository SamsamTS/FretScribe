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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn readout_names_cents_frequency_and_candidate() {
        let std = Tuning::standard();
        let r = PitchRef::default();
        let midi = 58.0 + 0.12;
        let out = readout(midi, &r, &std, 0);
        assert!(out.pitch.starts_with("A#3 +12c, 23"), "{}", out.pitch);
        assert!(out.pitch.ends_with(" Hz"));
        assert_eq!(out.candidate.as_deref(), Some("G 3"));
        assert_eq!(readout(64.0, &r, &std, 0).candidate.as_deref(), Some("E 0"));
        assert_eq!(readout(30.0, &r, &std, 0).candidate, None);
    }

    #[test]
    fn readout_is_relative_to_the_reference_pitch() {
        let tuned = PitchRef::new(432.0).unwrap();
        let out = readout(69.0, &tuned, &Tuning::standard(), 0);
        assert!(out.pitch.starts_with("A4 +0c, 432.0"), "{}", out.pitch);
    }

    #[test]
    fn natural_notes_and_open_strings() {
        let naturals: Vec<i32> = (60..72).filter(|&m| is_natural(m)).collect();
        assert_eq!(naturals, [60, 62, 64, 65, 67, 69, 71]);
        assert!(is_natural(-12));
        assert_eq!(open_pitches(&Tuning::standard(), 2), [42, 47, 52, 57, 61, 66]);
    }

    #[test]
    fn readout_never_leaves_the_bounds() {
        let b = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
        let size = vec2(110.0, 34.0);
        for cursor in [pos2(10.0, 10.0), pos2(295.0, 10.0), pos2(10.0, 195.0), pos2(299.0, 199.0), pos2(150.0, 100.0)] {
            let p = place_readout(b, cursor, size);
            assert!(b.contains_rect(Rect::from_min_size(p, size)), "{cursor:?} -> {p:?}");
        }
        assert_eq!(place_readout(b, pos2(10.0, 10.0), size), pos2(24.0, 24.0));
        assert!(place_readout(b, pos2(295.0, 10.0), size).x < 295.0);
    }
}
