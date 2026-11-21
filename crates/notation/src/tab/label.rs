//! The one place that names strings: spectrogram readout, menus and the tab ruler all go through here.

use fretscribe_core::{midi_to_name, pitch_class_name, Tuning};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelStyle {
    #[default]
    Convention,
    PitchWithOctave,
    StringNumber,
}

fn convention(tuning: &Tuning, string: usize) -> Option<String> {
    let strings = &tuning.open_strings;
    if string >= strings.len() {
        return None;
    }
    let top = strings.iter().rposition(|&m| Some(&m) == strings.iter().max())?;
    let base = |i: usize| {
        let name = pitch_class_name(strings[i]).to_owned();
        if i == top { name.to_lowercase() } else { name }
    };
    let label = base(string);
    let shared = (0..strings.len()).filter(|&i| base(i) == label).count() > 1;
    Some(if shared { format!("{label}{}", i32::from(strings[string] / 12) - 1) } else { label })
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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/notation/tab/label.rs"]
mod tests;
