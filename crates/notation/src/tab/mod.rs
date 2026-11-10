//! Tab inference: label helpers, tuning presets and the string/fret assignment (see `solve`).

mod label;
mod solve;
#[cfg(test)]
mod tests;

use fretscribe_core::Tuning;

pub use label::{candidate_label, candidate_label_styled, string_label, string_label_styled, LabelStyle};
pub use solve::{assign_tabs, assign_tabs_range};

pub const MAX_FRET: u8 = 24;

#[derive(Clone, Debug, PartialEq)]
pub struct TabOptions {
    pub max_fret: u8,
    pub max_hand_span: u8,
    pub position_weight: f32,
    pub movement_weight: f32,
    pub string_jump_weight: f32,
    pub open_string_bonus: f32,
    pub grace_bonus: f32,
}

impl Default for TabOptions {
    fn default() -> Self {
        Self {
            max_fret: 22,
            max_hand_span: 4,
            position_weight: 0.1,
            movement_weight: 1.0,
            string_jump_weight: 0.3,
            open_string_bonus: 0.3,
            grace_bonus: 1.0,
        }
    }
}

/// Every `(string, fret)` that plays `midi`, lowest string first; string 0 is the lowest in `Tuning::open_strings`.
pub fn candidates(midi: u8, tuning: &Tuning, capo: u8, max_fret: u8) -> Vec<(u8, u8)> {
    tuning
        .open_strings
        .iter()
        .enumerate()
        .filter_map(|(string, &open)| {
            let fret = midi.checked_sub(open)?;
            (capo..=max_fret).contains(&fret).then_some((string as u8, fret))
        })
        .collect()
}

/// Fret with the lowest number (not below the capo) on any string; ties go to the lower string.
pub fn lowest_fret_candidate(tuning: &Tuning, capo: u8, midi: u8) -> Option<(u8, u8)> {
    candidates(midi, tuning, capo, MAX_FRET).into_iter().min_by_key(|&(_, fret)| fret)
}

pub fn presets() -> Vec<Tuning> {
    let tuning = |name: &str, open: &[u8]| Tuning { name: name.to_owned(), open_strings: open.to_vec() };
    vec![
        Tuning::standard(),
        tuning("Drop D", &[38, 45, 50, 55, 59, 64]),
        tuning("Double Drop D", &[38, 45, 50, 55, 59, 62]),
        tuning("DADGAD", &[38, 45, 50, 55, 57, 62]),
        tuning("Open G", &[38, 43, 50, 55, 59, 62]),
        tuning("Open D", &[38, 45, 50, 54, 57, 62]),
        tuning("Half-step down", &[39, 44, 49, 54, 58, 63]),
        tuning("Full-step down", &[38, 43, 48, 53, 57, 62]),
        tuning("7-string standard", &[35, 40, 45, 50, 55, 59, 64]),
        tuning("Bass 4-string", &[28, 33, 38, 43]),
    ]
}
