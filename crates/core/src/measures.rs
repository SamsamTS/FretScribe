//! Measure-level grid operations (no notes involved; see `edit` for the note-aware versions).
//!
//! Grid invariant: an un-anchored measure starts where the previous one ends by tempo
//! (`start + beats * 60 / bpm`); an anchored measure keeps its start whatever happens before it.

use std::ops::Range;

use crate::error::EditError;
use crate::model::{Measure, TimeSignature, MAX_BPM, MIN_BPM};

const MAX_GENERATED_MEASURES: usize = 100_000;
const SAME_VALUE_EPS: f64 = 1e-9;

pub fn valid_tempo(bpm: f64) -> bool {
    bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&bpm)
}

/// Out-of-range tempos and invalid signatures fall back to the nearest valid value / 4/4,
/// so the result always has at least one measure and covers `audio_len_sec`.
pub fn generate_measures(
    start_sec: f64,
    audio_len_sec: f64,
    bpm: f64,
    time_sig: TimeSignature,
    next_measure_id: &mut u64,
) -> Vec<Measure> {
    let bpm = if bpm.is_finite() {
        bpm.clamp(MIN_BPM, MAX_BPM)
    } else {
        120.0
    };
    let time_sig = if time_sig.is_valid() {
        time_sig
    } else {
        TimeSignature::COMMON
    };
    let duration = time_sig.beats() * 60.0 / bpm;
    let span = (audio_len_sec - start_sec) / duration;
    let count = if span.is_finite() {
        ((span - SAME_VALUE_EPS).ceil().max(1.0) as usize).min(MAX_GENERATED_MEASURES)
    } else {
        1
    };
    (0..count)
        .map(|k| {
            let id = *next_measure_id;
            *next_measure_id += 1;
            Measure::new(id, start_sec + k as f64 * duration, bpm, time_sig)
        })
        .collect()
}

/// Appends unanchored copies of the last measure until the grid covers `audio_len_sec`, then drops
/// trailing measures that start at or after it, as long as `keep` accepts them (e.g. no note on them).
/// Returns whether anything changed.
pub fn cover_audio(measures: &mut Vec<Measure>, audio_len_sec: f64, next_id: &mut u64, keep: impl Fn(&Measure) -> bool) -> bool {
    if !audio_len_sec.is_finite() || measures.is_empty() {
        return false;
    }
    let before = measures.len();
    while measures.len() > 1 && measures.last().is_some_and(|m| m.start_sec >= audio_len_sec - SAME_VALUE_EPS && !keep(m)) {
        measures.pop();
    }
    while measures.len() < MAX_GENERATED_MEASURES {
        let Some(last) = measures.last() else { break };
        let end = last.start_sec + last.nominal_duration_sec();
        if end >= audio_len_sec - SAME_VALUE_EPS {
            break;
        }
        let mut next = Measure::new(*next_id, end, last.tempo_bpm, last.time_sig);
        next.anchored = false;
        *next_id += 1;
        measures.push(next);
    }
    measures.len() != before
}

pub(crate) fn recompute_starts(measures: &mut [Measure], from: usize) {
    for j in from.max(1)..measures.len() {
        if !measures[j].anchored {
            let prev = &measures[j - 1];
            measures[j].start_sec = prev.start_sec + prev.nominal_duration_sec();
        }
    }
}

fn check_index(measures: &[Measure], index: usize) -> Result<(), EditError> {
    if index < measures.len() {
        Ok(())
    } else {
        Err(EditError::MeasureOutOfRange {
            index,
            len: measures.len(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Propagation {
    /// The edit also applies to every following measure.
    #[default]
    Following,
    Single,
}

/// Edits `index` and every following measure. Returns the edited range.
pub fn set_tempo(
    measures: &mut [Measure],
    index: usize,
    new_bpm: f64,
) -> Result<Range<usize>, EditError> {
    set_tempo_scoped(measures, index, new_bpm, Propagation::Following)
}

pub fn set_tempo_scoped(
    measures: &mut [Measure],
    index: usize,
    new_bpm: f64,
    propagation: Propagation,
) -> Result<Range<usize>, EditError> {
    check_index(measures, index)?;
    if !valid_tempo(new_bpm) {
        return Err(EditError::InvalidTempo(new_bpm));
    }
    let end = if propagation == Propagation::Following { measures.len() } else { index + 1 };
    for m in &mut measures[index..end] {
        m.tempo_bpm = new_bpm;
    }
    recompute_starts(measures, index + 1);
    Ok(index..end)
}

/// Same propagation rule as `set_tempo`, applied to the time signature.
pub fn set_time_sig(
    measures: &mut [Measure],
    index: usize,
    new_sig: TimeSignature,
) -> Result<Range<usize>, EditError> {
    set_time_sig_scoped(measures, index, new_sig, Propagation::Following)
}

pub fn set_time_sig_scoped(
    measures: &mut [Measure],
    index: usize,
    new_sig: TimeSignature,
    propagation: Propagation,
) -> Result<Range<usize>, EditError> {
    check_index(measures, index)?;
    if !new_sig.is_valid() {
        return Err(EditError::InvalidTimeSignature(new_sig));
    }
    let end = if propagation == Propagation::Following { measures.len() } else { index + 1 };
    for m in &mut measures[index..end] {
        m.time_sig = new_sig;
    }
    recompute_starts(measures, index + 1);
    Ok(index..end)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    pub fn grid(tempos: &[f64]) -> Vec<Measure> {
        let mut ms: Vec<Measure> = tempos
            .iter()
            .enumerate()
            .map(|(i, &bpm)| Measure::new(i as u64 + 1, 0.0, bpm, TimeSignature::COMMON))
            .collect();
        recompute_starts(&mut ms, 1);
        ms
    }

    #[test]
    fn cover_adds_and_removes_trailing_measures() {
        let mut ms = grid(&[120.0, 120.0]);
        let mut id = 10;
        assert!(cover_audio(&mut ms, 7.0, &mut id, |_| false));
        assert_eq!((ms.len(), id), (4, 12));
        close(ms[3].start_sec, 6.0);
        assert!(!cover_audio(&mut ms, 7.0, &mut id, |_| false));
        assert!(cover_audio(&mut ms, 3.0, &mut id, |m| m.id == 10));
        assert_eq!(ms.len(), 3);
        assert!(cover_audio(&mut ms, 0.5, &mut id, |_| false));
        assert_eq!(ms.len(), 1);
    }

    #[test]
    fn generate_covers_audio_with_fresh_ids() {
        let mut next = 10;
        let ms = generate_measures(0.5, 10.0, 120.0, TimeSignature::COMMON, &mut next);
        assert_eq!(ms.len(), 5);
        assert_eq!(next, 15);
        assert_eq!(
            ms.iter().map(|m| m.id).collect::<Vec<_>>(),
            [10, 11, 12, 13, 14]
        );
        close(ms[0].start_sec, 0.5);
        close(ms[4].start_sec, 8.5);
        assert!(ms.iter().all(|m| !m.anchored));
        let last_end = ms[4].start_sec + ms[4].nominal_duration_sec();
        assert!(last_end >= 10.0);
    }

    #[test]
    fn generate_exact_fit_has_no_extra_measure() {
        let mut next = 1;
        let ms = generate_measures(0.0, 8.0, 120.0, TimeSignature::COMMON, &mut next);
        assert_eq!(ms.len(), 4);
    }

    #[test]
    fn generate_degenerate_inputs() {
        let mut next = 1;
        assert_eq!(
            generate_measures(0.0, 0.0, 120.0, TimeSignature::COMMON, &mut next).len(),
            1
        );
        assert_eq!(
            generate_measures(5.0, 1.0, 120.0, TimeSignature::COMMON, &mut next).len(),
            1
        );
        let ms = generate_measures(0.0, 4.0, 0.0, TimeSignature::new(0, 3), &mut next);
        assert_eq!(ms[0].time_sig, TimeSignature::COMMON);
        assert_eq!(ms[0].tempo_bpm, MIN_BPM);
        let ms = generate_measures(0.0, 4.0, f64::NAN, TimeSignature::COMMON, &mut next);
        assert_eq!(ms[0].tempo_bpm, 120.0);
    }

    #[test]
    fn generate_other_signatures() {
        let mut next = 1;
        let ms = generate_measures(0.0, 6.0, 120.0, TimeSignature::new(6, 8), &mut next);
        assert_eq!(ms.len(), 4);
        close(ms[1].start_sec, 1.5);
    }

    #[test]
    fn tempo_edit_propagates_to_all_following_measures() {
        let mut ms = grid(&[120.0, 120.0, 120.0, 90.0, 120.0]);
        let range = set_tempo(&mut ms, 0, 60.0).unwrap();
        assert_eq!(range, 0..5);
        let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [60.0; 5]);
    }

    #[test]
    fn single_tempo_edit_leaves_equal_followers_alone() {
        let mut ms = grid(&[120.0, 120.0, 120.0]);
        let range = set_tempo_scoped(&mut ms, 0, 60.0, Propagation::Single).unwrap();
        assert_eq!(range, 0..1);
        let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [60.0, 120.0, 120.0]);
        close(ms[1].start_sec, 4.0);
        close(ms[2].start_sec, 6.0);
        assert_eq!(
            set_tempo_scoped(&mut ms, 0, 60.0, Propagation::Following).unwrap(),
            0..3
        );
    }

    #[test]
    fn single_time_sig_edit_leaves_equal_followers_alone() {
        let mut ms = grid(&[120.0; 3]);
        let range =
            set_time_sig_scoped(&mut ms, 1, TimeSignature::new(3, 4), Propagation::Single).unwrap();
        assert_eq!(range, 1..2);
        assert_eq!(ms[1].time_sig, TimeSignature::new(3, 4));
        assert_eq!(ms[2].time_sig, TimeSignature::COMMON);
        close(ms[2].start_sec, 3.5);
        assert!(
            set_time_sig_scoped(&mut ms, 0, TimeSignature::new(4, 3), Propagation::Single).is_err()
        );
    }

    #[test]
    fn tempo_edit_overrides_different_followers() {
        let mut ms = grid(&[120.0, 120.0, 90.0, 120.0]);
        set_tempo(&mut ms, 0, 100.0).unwrap();
        let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [100.0; 4]);
    }

    #[test]
    fn tempo_edit_in_the_middle_leaves_earlier_measures_alone() {
        let mut ms = grid(&[120.0, 120.0, 120.0, 120.0]);
        let range = set_tempo(&mut ms, 2, 240.0).unwrap();
        assert_eq!(range, 2..4);
        let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [120.0, 120.0, 240.0, 240.0]);
        close(ms[2].start_sec, 4.0);
        close(ms[3].start_sec, 5.0);
    }

    #[test]
    fn single_tempo_edit_between_different_neighbours() {
        let mut ms = grid(&[90.0, 120.0, 100.0]);
        assert_eq!(set_tempo_scoped(&mut ms, 1, 60.0, Propagation::Single).unwrap(), 1..2);
        let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [90.0, 60.0, 100.0]);
    }

    #[test]
    fn tempo_edit_shifts_later_starts() {
        let mut ms = grid(&[120.0, 120.0, 90.0, 120.0]);
        set_tempo(&mut ms, 0, 60.0).unwrap();
        close(ms[1].start_sec, 4.0);
        close(ms[2].start_sec, 8.0);
        close(ms[3].start_sec, 12.0);
    }

    #[test]
    fn anchored_measures_keep_their_start() {
        let mut ms = grid(&[120.0, 120.0, 120.0, 120.0]);
        ms[2].anchored = true;
        set_tempo(&mut ms, 0, 60.0).unwrap();
        close(ms[1].start_sec, 4.0);
        close(ms[2].start_sec, 4.0);
        close(ms[3].start_sec, 4.0 + 4.0 * 60.0 / 60.0);
    }

    #[test]
    fn anchor_on_the_edited_measure_does_not_block_its_own_edit() {
        let mut ms = grid(&[120.0, 120.0]);
        ms[0].anchored = true;
        set_tempo(&mut ms, 0, 60.0).unwrap();
        close(ms[0].start_sec, 0.0);
        close(ms[1].start_sec, 4.0);
    }

    #[test]
    fn tempo_validation() {
        let mut ms = grid(&[120.0]);
        assert_eq!(
            set_tempo(&mut ms, 0, 0.0),
            Err(EditError::InvalidTempo(0.0))
        );
        assert!(set_tempo(&mut ms, 0, f64::NAN).is_err());
        assert!(set_tempo(&mut ms, 0, 9.0).is_err());
        assert!(set_tempo(&mut ms, 0, 501.0).is_err());
        assert_eq!(
            set_tempo(&mut ms, 3, 100.0),
            Err(EditError::MeasureOutOfRange { index: 3, len: 1 })
        );
        assert_eq!(ms[0].tempo_bpm, 120.0);
    }

    #[test]
    fn time_sig_edit_propagates_to_all_following_measures() {
        let mut ms = grid(&[120.0; 5]);
        ms[3].time_sig = TimeSignature::new(3, 4);
        ms[4].time_sig = TimeSignature::new(4, 4);
        let range = set_time_sig(&mut ms, 1, TimeSignature::new(6, 8)).unwrap();
        assert_eq!(range, 1..5);
        let sigs: Vec<(u8, u8)> = ms
            .iter()
            .map(|m| (m.time_sig.numerator, m.time_sig.denominator))
            .collect();
        assert_eq!(sigs, [(4, 4), (6, 8), (6, 8), (6, 8), (6, 8)]);
    }

    #[test]
    fn time_sig_edit_recomputes_starts() {
        let mut ms = grid(&[120.0; 4]);
        set_time_sig(&mut ms, 1, TimeSignature::new(3, 4)).unwrap();
        close(ms[1].start_sec, 2.0);
        close(ms[2].start_sec, 3.5);
        close(ms[3].start_sec, 5.0);
    }

    #[test]
    fn time_sig_edit_respects_anchors() {
        let mut ms = grid(&[120.0; 4]);
        ms[3].anchored = true;
        set_time_sig(&mut ms, 0, TimeSignature::new(3, 4)).unwrap();
        close(ms[2].start_sec, 3.0);
        close(ms[3].start_sec, 6.0);
    }

    #[test]
    fn time_sig_validation() {
        let mut ms = grid(&[120.0]);
        assert!(set_time_sig(&mut ms, 0, TimeSignature::new(0, 4)).is_err());
        assert!(set_time_sig(&mut ms, 0, TimeSignature::new(4, 3)).is_err());
        assert!(set_time_sig(&mut ms, 0, TimeSignature::new(4, 0)).is_err());
        assert_eq!(ms[0].time_sig, TimeSignature::COMMON);
    }

    #[test]
    fn signature_beats() {
        assert_eq!(TimeSignature::new(4, 4).beats(), 4.0);
        assert_eq!(TimeSignature::new(6, 8).beats(), 3.0);
        assert_eq!(TimeSignature::new(3, 4).beats(), 3.0);
        assert_eq!(TimeSignature::new(7, 8).beats(), 3.5);
        assert_eq!(TimeSignature::new(2, 2).beats(), 4.0);
        assert_eq!(TimeSignature::new(5, 16).beats(), 1.25);
    }
}
