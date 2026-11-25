//! Measure-level grid operations (no notes involved; see `edit` for the note-aware versions).
//!
//! Grid invariant: an un-anchored measure starts where the previous one ends by tempo
//! (`start + beats * 60 / bpm`); an anchored measure keeps its start whatever happens before it.

use std::ops::Range;

use crate::tempo::EPS;
use crate::error::EditError;
use crate::model::{Measure, TimeSignature, MAX_BPM, MIN_BPM};

const MAX_GENERATED_MEASURES: usize = 100_000;

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
        ((span - EPS).ceil().max(1.0) as usize).min(MAX_GENERATED_MEASURES)
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
    while measures.len() > 1 && measures.last().is_some_and(|m| m.start_sec >= audio_len_sec - EPS && !keep(m)) {
        measures.pop();
    }
    while measures.len() < MAX_GENERATED_MEASURES {
        let Some(last) = measures.last() else { break };
        let end = last.start_sec + last.nominal_duration_sec();
        if end >= audio_len_sec - EPS {
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
        if !measures[j].is_anchored() {
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
    /// The edit also applies to every following measure (locked ones are skipped).
    #[default]
    Following,
    /// The following measures that had the same old value, stopping before a locked one.
    Identical,
    Single,
}

/// `index` and the contiguous following measures with the same `key` as `index`. Include `locked` in the
/// key to stop before a locked measure.
pub fn contiguous_same<K: PartialEq>(measures: &[Measure], index: usize, key: impl Fn(&Measure) -> K) -> Range<usize> {
    let Some(first) = measures.get(index).map(&key) else { return index..index };
    let end = measures[index..].iter().position(|m| key(m) != first).map_or(measures.len(), |p| index + p);
    index..end
}

pub(crate) fn check_unlocked(measures: &[Measure], index: usize) -> Result<(), EditError> {
    match measures.get(index) {
        Some(m) if m.locked => Err(EditError::MeasureLocked(index)),
        _ => Ok(()),
    }
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
    check_unlocked(measures, index)?;
    if !valid_tempo(new_bpm) {
        return Err(EditError::InvalidTempo(new_bpm));
    }
    Ok(set_scoped(measures, index, propagation, |m| m.tempo_bpm, new_bpm, |m| &mut m.tempo_bpm))
}

/// Sets the field on the unlocked measures in scope that differ, dropping their anchor, then refits the starts.
fn set_scoped<T: PartialEq + Copy>(
    measures: &mut [Measure],
    index: usize,
    propagation: Propagation,
    get: impl Fn(&Measure) -> T,
    value: T,
    field: impl Fn(&mut Measure) -> &mut T,
) -> Range<usize> {
    let range = match propagation {
        Propagation::Following => index..measures.len(),
        Propagation::Identical => contiguous_same(measures, index, |m| (get(m), m.locked)),
        Propagation::Single => index..index + 1,
    };
    for m in measures[range.clone()].iter_mut().filter(|m| !m.locked) {
        if get(m) != value {
            *field(m) = value;
            m.anchored = false;
        }
    }
    recompute_starts(measures, index);
    range
}

/// Sets a per-measure flag on `index` and, with `propagate`, on the run `contiguous_same` gives for `key`.
pub(crate) fn set_flag<K: PartialEq>(
    measures: &mut [Measure],
    index: usize,
    propagate: bool,
    key: impl Fn(&Measure) -> K,
    set: impl Fn(&mut Measure),
) -> Result<Range<usize>, EditError> {
    check_index(measures, index)?;
    let range = if propagate { contiguous_same(measures, index, key) } else { index..index + 1 };
    measures[range.clone()].iter_mut().for_each(set);
    Ok(range)
}

/// Sets let ring on `index` and on every contiguous following measure that had the same old value as `index`
/// (the run stops at the first measure that differs). Returns the edited range.
pub fn set_let_ring(measures: &mut [Measure], index: usize, on: bool) -> Result<Range<usize>, EditError> {
    set_flag(measures, index, true, |m| m.let_ring, |m| m.let_ring = on)
}

/// Same propagation rule as `set_tempo`, applied to the time signature. A manual change of either drops the fit (anchor) of every measure it changes.
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
    check_unlocked(measures, index)?;
    if !new_sig.is_valid() {
        return Err(EditError::InvalidTimeSignature(new_sig));
    }
    Ok(set_scoped(measures, index, propagation, |m| m.time_sig, new_sig, |m| &mut m.time_sig))
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/measures.rs"]
pub(crate) mod tests;
