//! Provisional string/fret helpers used by the spectrogram readout, until the tab inference lands.

use fretscribe_core::{midi_to_name, Tuning};

pub const MAX_FRET: u8 = 24;

/// Fret with the lowest number (not below the capo) on any string; ties go to the lower string.
/// Returns `(string_index, fret)`, string 0 being the lowest in `Tuning::open_strings`.
pub fn lowest_fret_candidate(tuning: &Tuning, capo: u8, midi: u8) -> Option<(u8, u8)> {
    tuning
        .open_strings
        .iter()
        .enumerate()
        .filter_map(|(string, &open)| {
            let fret = midi.checked_sub(open)?;
            (capo..=MAX_FRET).contains(&fret).then_some((string as u8, fret))
        })
        .min_by_key(|&(_, fret)| fret)
}

/// The "E 10" label of the tab menus: open-string name without octave, a space, the fret.
pub fn candidate_label(tuning: &Tuning, string: u8, fret: u8) -> Option<String> {
    let open = *tuning.open_strings.get(usize::from(string))?;
    let name = midi_to_name(open);
    Some(format!("{} {fret}", name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowest_fret_wins_and_respects_capo() {
        let t = Tuning::standard();
        assert_eq!(lowest_fret_candidate(&t, 0, 64), Some((5, 0)));
        assert_eq!(lowest_fret_candidate(&t, 0, 50), Some((2, 0)));
        assert_eq!(lowest_fret_candidate(&t, 2, 64), Some((4, 5)));
        assert_eq!(lowest_fret_candidate(&t, 0, 30), None);
        assert_eq!(lowest_fret_candidate(&t, 0, 120), None);
    }

    #[test]
    fn label_format() {
        let t = Tuning::standard();
        assert_eq!(candidate_label(&t, 0, 10).as_deref(), Some("E 10"));
        assert_eq!(candidate_label(&t, 5, 0).as_deref(), Some("E 0"));
        assert_eq!(candidate_label(&t, 9, 0), None);
    }
}
