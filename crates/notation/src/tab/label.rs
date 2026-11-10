//! The one place that names strings: spectrogram readout, menus and the tab ruler all go through here.

use fretscribe_core::{midi_to_name, Tuning};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelStyle {
    #[default]
    Convention,
    PitchWithOctave,
    StringNumber,
}

fn pitch_class(midi: u8) -> String {
    midi_to_name(midi).trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').to_owned()
}

fn octave(midi: u8) -> i32 {
    i32::from(midi / 12) - 1
}

fn convention(tuning: &Tuning, string: usize) -> Option<String> {
    let strings = &tuning.open_strings;
    if string >= strings.len() {
        return None;
    }
    let top = strings.iter().rposition(|&m| Some(&m) == strings.iter().max())?;
    let base = |i: usize| {
        let name = pitch_class(strings[i]);
        if i == top { name.to_lowercase() } else { name }
    };
    let label = base(string);
    let shared = (0..strings.len()).filter(|&i| base(i) == label).count() > 1;
    Some(if shared { format!("{label}{}", octave(strings[string])) } else { label })
}

pub fn string_label_styled(tuning: &Tuning, string: u8, style: LabelStyle) -> Option<String> {
    let index = usize::from(string);
    let open = *tuning.open_strings.get(index)?;
    match style {
        LabelStyle::Convention => convention(tuning, index),
        LabelStyle::PitchWithOctave => Some(midi_to_name(open)),
        LabelStyle::StringNumber => Some((tuning.open_strings.len() - index).to_string()),
    }
}

pub fn string_label(tuning: &Tuning, string: u8) -> Option<String> {
    string_label_styled(tuning, string, LabelStyle::Convention)
}

pub fn candidate_label_styled(tuning: &Tuning, string: u8, fret: u8, style: LabelStyle) -> Option<String> {
    Some(format!("{} {fret}", string_label_styled(tuning, string, style)?))
}

pub fn candidate_label(tuning: &Tuning, string: u8, fret: u8) -> Option<String> {
    candidate_label_styled(tuning, string, fret, LabelStyle::Convention)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(open: &[u8]) -> Vec<String> {
        let t = Tuning { name: String::new(), open_strings: open.to_vec() };
        (0..open.len() as u8).filter_map(|s| string_label(&t, s)).collect()
    }

    #[test]
    fn conventional_labels() {
        assert_eq!(labels(&[40, 45, 50, 55, 59, 64]), ["E", "A", "D", "G", "B", "e"]);
        assert_eq!(labels(&[38, 45, 50, 55, 59, 64]), ["D2", "A", "D3", "G", "B", "e"]);
        assert_eq!(labels(&[38, 45, 50, 55, 57, 62]), ["D2", "A2", "D3", "G", "A3", "d"]);
        assert_eq!(labels(&[35, 40, 45, 50, 55, 59, 64]), ["B1", "E", "A", "D", "G", "B3", "e"]);
        assert_eq!(labels(&[38, 43, 50, 55, 59, 62]), ["D2", "G2", "D3", "G3", "B", "d"]);
        assert_eq!(labels(&[28, 33, 38, 43]), ["E", "A", "D", "g"]);
    }

    #[test]
    fn styles_and_fret_suffix() {
        let t = Tuning::standard();
        assert_eq!(candidate_label(&t, 0, 10).as_deref(), Some("E 10"));
        assert_eq!(candidate_label(&t, 5, 3).as_deref(), Some("e 3"));
        assert_eq!(candidate_label_styled(&t, 0, 10, LabelStyle::PitchWithOctave).as_deref(), Some("E2 10"));
        assert_eq!(candidate_label_styled(&t, 0, 10, LabelStyle::StringNumber).as_deref(), Some("6 10"));
        assert_eq!(candidate_label(&t, 9, 0), None);
    }
}
