//! Pitch refinement of hand-placed notes.

use fretscribe_core::PitchRef;

use crate::Spectrogram;

const SEARCH_SEMITONES: f32 = 0.5;
const WINDOW_SEC: f64 = 0.03;
const MIN_ENERGY: f32 = 1e-4;

/// Local energy peak within half a semitone of `approx_midi`, averaged over about +-30 ms around
/// `time_sec` (audio time of the spectrogram). Returns `approx_midi` when there is nothing to find.
pub fn refine_pitch(spec: &Spectrogram, pitch_ref: &PitchRef, time_sec: f64, approx_midi: f32) -> f32 {
    if spec.n_frames == 0 || spec.n_rows == 0 || !approx_midi.is_finite() || !time_sec.is_finite() {
        return approx_midi;
    }
    let frame = spec.sec_to_frame(time_sec).round();
    if frame < 0.0 || frame >= spec.n_frames as f64 {
        return approx_midi;
    }
    let half = (WINDOW_SEC / spec.hop_sec).ceil().max(1.0) as usize;
    let frames = (frame as usize).saturating_sub(half)..(frame as usize + half + 1).min(spec.n_frames);
    let energy = |row: usize| frames.clone().map(|f| spec.value(row, f)).sum::<f32>() / frames.len() as f32;

    let last = (spec.n_rows - 1) as f32;
    let lo = spec.midi_to_row(approx_midi - SEARCH_SEMITONES, pitch_ref).ceil().clamp(0.0, last) as usize;
    let hi = spec.midi_to_row(approx_midi + SEARCH_SEMITONES, pitch_ref).floor().clamp(0.0, last) as usize;
    if lo > hi {
        return approx_midi;
    }
    let (peak, best) = (lo..=hi)
        .map(|row| (row, energy(row)))
        .fold((lo, f32::MIN), |acc, cur| if cur.1 > acc.1 { cur } else { acc });
    if best < MIN_ENERGY {
        return approx_midi;
    }

    let mut offset = 0.0;
    if peak > 0 && peak < spec.n_rows - 1 {
        let (a, c) = (energy(peak - 1), energy(peak + 1));
        let curvature = a - 2.0 * best + c;
        if curvature < 0.0 {
            offset = (0.5 * (a - c) / curvature).clamp(-0.5, 0.5);
        }
    }
    let midi = spec.row_to_midi(peak as f32 + offset, pitch_ref);
    midi.clamp(approx_midi - SEARCH_SEMITONES, approx_midi + SEARCH_SEMITONES)
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/analysis/notes.rs"]
mod tests;
