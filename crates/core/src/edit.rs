//! Note-aware grid edits.
//!
//! Every edit runs as a transaction (a failure leaves the project untouched) and returns a
//! `ProjectEdit` that can be undone and redone. After the grid changes, each note time is resolved
//! according to the `NotePolicy`: `FollowGrid` keeps its `TimePos`, `KeepAudioPosition` keeps its
//! seconds. A time whose measure no longer exists is always re-resolved by seconds.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::tempo::EPS;
use crate::delta::{Delta, Snapshot};
use crate::error::{CoreError, EditError, UndoError};
use crate::measures::{self, recompute_starts, valid_tempo, Propagation};
use crate::model::{Measure, Project, Technique, TimePos, TimeSignature, MAX_BPM, MIN_BPM};
use crate::pitch::PitchRef;
use crate::tempo::TempoMap;

const MIN_MEASURE_SEC: f64 = 1e-3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotePolicy {
    #[default]
    FollowGrid,
    KeepAudioPosition,
}

impl NotePolicy {
    pub fn inverted(self) -> Self {
        match self {
            Self::FollowGrid => Self::KeepAudioPosition,
            Self::KeepAudioPosition => Self::FollowGrid,
        }
    }
}

/// An undoable change, stored as a delta (see `delta`). `weight` is the number of user-visible steps
/// it covers (more than 1 once old history has been thinned).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectEdit {
    label: String,
    weight: usize,
    delta: Delta,
}

impl ProjectEdit {
    pub(crate) fn new(label: &str, delta: Delta) -> Self {
        Self {
            label: label.to_owned(),
            weight: 1,
            delta,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn weight(&self) -> usize {
        self.weight
    }

    /// One edit that undoes `self` and `later` together; the weight stays that of `self`.
    pub fn merge(self, later: ProjectEdit) -> ProjectEdit {
        let weight = self.weight;
        self.merge_weighted(later, weight)
    }

    pub(crate) fn merge_weighted(self, later: ProjectEdit, weight: usize) -> ProjectEdit {
        ProjectEdit {
            label: later.label,
            weight,
            delta: self.delta.compose(later.delta),
        }
    }

    pub fn try_undo(&self, project: &mut Project) -> Result<(), UndoError> {
        self.delta.apply(project, true)
    }

    pub fn try_redo(&self, project: &mut Project) -> Result<(), UndoError> {
        self.delta.apply(project, false)
    }

    /// Applies nothing if the project diverged from the history; use `try_undo` to learn about it.
    pub fn undo(&self, project: &mut Project) {
        let _ = self.try_undo(project);
    }

    pub fn redo(&self, project: &mut Project) {
        let _ = self.try_redo(project);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FittedMeasure {
    pub start_sec: f64,
    pub tempo_bpm: f64,
    pub time_sig: TimeSignature,
}

fn resolve(map: &TempoMap, pos: TimePos, old_sec: f64, policy: NotePolicy) -> TimePos {
    let unchanged = match policy {
        NotePolicy::FollowGrid => map.index_of(pos.measure).is_some(),
        NotePolicy::KeepAudioPosition => map
            .try_pos_to_sec(pos)
            .is_some_and(|sec| (sec - old_sec).abs() < EPS),
    };
    if unchanged {
        map.normalise(pos)
    } else {
        map.sec_to_pos(old_sec)
    }
}

impl Project {
    /// Runs `f` as one undoable edit; an error leaves the project untouched.
    /// Like `edit_notes` for operations with their own error and result types.
    pub fn try_edit<T, E>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project) -> Result<T, E>,
    ) -> Result<(ProjectEdit, T), E> {
        let before = Snapshot::capture(self);
        match f(self) {
            Ok(value) => Ok((self.finish(label, &before), value)),
            Err(e) => {
                before.restore(self);
                Err(e)
            }
        }
    }

    /// Runs `f` as one undoable edit; an error leaves the project untouched.
    pub fn edit_notes<F>(&mut self, label: &str, f: F) -> Result<ProjectEdit, EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        self.try_edit(label, f).map(|(edit, ())| edit)
    }

    pub(crate) fn transact_infallible(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project),
    ) -> ProjectEdit {
        let before = Snapshot::capture(self);
        f(self);
        self.finish(label, &before)
    }

    fn finish(&self, label: &str, before: &Snapshot) -> ProjectEdit {
        let edit = ProjectEdit::new(label, Delta::diff(before, self));
        #[cfg(all(test, dev_tests))]
        tests::check_undo_redo(self, &edit, before, label);
        edit
    }

    /// Changes the reference pitch (A4); notes keep their pitch numbers.
    pub fn set_reference(&mut self, hz: f32) -> Result<ProjectEdit, CoreError> {
        PitchRef::new(hz)?;
        Ok(self.transact_infallible("Set reference pitch", |p| p.reference_hz = hz))
    }

    fn rebase_notes<F>(&mut self, policy: NotePolicy, f: F) -> Result<(), EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        let old_times = self.note_times();
        f(self)?;
        let map = TempoMap::new(&self.measures);
        for note in &mut self.notes {
            if let (Some(start), Some(end)) =
                (old_times.start_sec(note.id), old_times.end_sec(note.id))
            {
                note.start = resolve(&map, note.start, start, policy);
                note.end = resolve(&map, note.end, end, policy);
            }
        }
        Ok(())
    }

    fn grid_edit<F>(
        &mut self,
        label: &str,
        policy: NotePolicy,
        f: F,
    ) -> Result<ProjectEdit, EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        self.edit_notes(label, |p| p.rebase_notes(policy, f))
    }

    fn check_measure(&self, index: usize) -> Result<(), EditError> {
        if index < self.measures.len() {
            Ok(())
        } else {
            Err(EditError::MeasureOutOfRange {
                index,
                len: self.measures.len(),
            })
        }
    }

    /// Sets let ring on measure `index`; `propagate` extends it to the contiguous following measures that had
    /// the same old value (`measures::set_let_ring`). Not a grid edit: no note moves.
    pub fn set_let_ring(&mut self, index: usize, on: bool, propagate: bool) -> Result<ProjectEdit, EditError> {
        self.edit_notes("Let ring", |p| {
            let range = measures::set_let_ring(&mut p.measures, index, on)?;
            if !propagate {
                (index + 1..range.end).for_each(|i| p.measures[i].let_ring = !on);
            }
            Ok(())
        })
    }

    /// Sets let ring on exactly the measures of `range`.
    pub fn set_let_ring_range(&mut self, range: std::ops::Range<usize>, on: bool) -> Result<ProjectEdit, EditError> {
        self.edit_notes("Let ring", |p| {
            p.check_measure(range.start)?;
            p.check_measure(range.end.saturating_sub(1))?;
            p.measures[range].iter_mut().for_each(|m| m.let_ring = on);
            Ok(())
        })
    }

    pub fn set_tempo_scoped(
        &mut self,
        index: usize,
        new_bpm: f64,
        propagation: Propagation,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Set tempo", policy, |p| {
            measures::set_tempo_scoped(&mut p.measures, index, new_bpm, propagation).map(drop)
        })
    }

    pub fn set_time_sig_scoped(
        &mut self,
        index: usize,
        new_sig: TimeSignature,
        propagation: Propagation,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Set time signature", policy, |p| {
            measures::set_time_sig_scoped(&mut p.measures, index, new_sig, propagation).map(drop)
        })
    }

    /// Anchors measure `index` at `new_start_sec`. The previous measure is stretched to end there
    /// (its tempo is refitted); later un-anchored measures keep their lengths and follow.
    pub fn move_measure_start(
        &mut self,
        index: usize,
        new_start_sec: f64,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Move measure start", policy, |p| {
            p.check_measure(index)?;
            if !new_start_sec.is_finite() {
                return Err(EditError::InvalidTime(new_start_sec));
            }
            if index > 0 {
                let prev = &p.measures[index - 1];
                let span = new_start_sec - prev.start_sec;
                if span < MIN_MEASURE_SEC {
                    return Err(EditError::InvalidTime(new_start_sec));
                }
                let refit = prev.time_sig.beats() * 60.0 / span;
                if !valid_tempo(refit) {
                    return Err(EditError::InvalidTempo(refit));
                }
                p.measures[index - 1].tempo_bpm = refit;
            }
            if let Some(next) = p.measures.get(index + 1) {
                if next.anchored && new_start_sec > next.start_sec - MIN_MEASURE_SEC {
                    return Err(EditError::InvalidTime(new_start_sec));
                }
            }
            p.measures[index].start_sec = new_start_sec;
            p.measures[index].anchored = true;
            recompute_starts(&mut p.measures, index + 1);
            Ok(())
        })
    }

    /// Replaces the grid with the fitted one (every measure anchored). Measures keep their id by
    /// position; extra ones get fresh ids. Callers normally pass `KeepAudioPosition`.
    pub fn auto_fit(
        &mut self,
        fitted: &[FittedMeasure],
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Auto-fit grid", policy, |p| {
            if fitted.is_empty() {
                return Err(EditError::LastMeasure);
            }
            let mut previous_start = f64::NEG_INFINITY;
            for f in fitted {
                if !f.start_sec.is_finite() || f.start_sec <= previous_start {
                    return Err(EditError::InvalidTime(f.start_sec));
                }
                if !valid_tempo(f.tempo_bpm) {
                    return Err(EditError::InvalidTempo(f.tempo_bpm));
                }
                if !f.time_sig.is_valid() {
                    return Err(EditError::InvalidTimeSignature(f.time_sig));
                }
                previous_start = f.start_sec;
            }
            let old_ids: Vec<u64> = p.measures.iter().map(|m| m.id).collect();
            let mut measures = Vec::with_capacity(fitted.len());
            for (k, f) in fitted.iter().enumerate() {
                let id = match old_ids.get(k) {
                    Some(&id) => id,
                    None => p.fresh_measure_id(),
                };
                measures.push(Measure {
                    id,
                    start_sec: f.start_sec,
                    tempo_bpm: f.tempo_bpm,
                    time_sig: f.time_sig,
                    anchored: true,
                    let_ring: p.measures.get(k).is_some_and(|m| m.let_ring),
                });
            }
            p.measures = measures;
            Ok(())
        })
    }

    /// Adds or removes measures at the end so the grid covers the audio; `None` when it already does.
    pub fn cover_audio(&mut self, audio_len_sec: f64) -> Option<ProjectEdit> {
        let used: HashSet<u64> = self.notes.iter().flat_map(|n| [n.start.measure, n.end.measure]).collect();
        let (mut measures, mut next) = (self.measures.clone(), self.next_measure_id);
        if !measures::cover_audio(&mut measures, audio_len_sec, &mut next, |m| used.contains(&m.id)) {
            return None;
        }
        Some(self.transact_infallible("Cover audio with measures", |p| {
            p.measures = measures;
            p.next_measure_id = next;
        }))
    }

    /// Inserts a measure (tempo and signature of the previous one) at `index`; every later measure,
    /// anchored or not, moves later by its duration.
    pub fn insert_measure(
        &mut self,
        index: usize,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Insert measure", policy, |p| {
            let len = p.measures.len();
            if len == 0 {
                return Err(EditError::NoGrid);
            }
            if index > len {
                return Err(EditError::MeasureOutOfRange { index, len });
            }
            let template = &p.measures[index.saturating_sub(1)];
            let (tempo_bpm, time_sig) = (template.tempo_bpm, template.time_sig);
            let start_sec = match p.measures.get(index) {
                Some(next) => next.start_sec,
                None => p.measures[len - 1].start_sec + p.measures[len - 1].nominal_duration_sec(),
            };
            let id = p.fresh_measure_id();
            let inserted = Measure::new(id, start_sec, tempo_bpm, time_sig);
            let shift = inserted.nominal_duration_sec();
            for m in &mut p.measures[index..] {
                m.start_sec += shift;
            }
            p.measures.insert(index, inserted);
            Ok(())
        })
    }

    /// Notes starting in the measure are deleted with their grace notes, which requires
    /// `confirmed`. Notes only ending in it are cut at its position; later measures move earlier.
    pub fn delete_measure(
        &mut self,
        index: usize,
        policy: NotePolicy,
        confirmed: bool,
    ) -> Result<ProjectEdit, EditError> {
        self.edit_notes("Delete measure", |p| {
            p.check_measure(index)?;
            if p.measures.len() == 1 {
                return Err(EditError::LastMeasure);
            }
            let map = TempoMap::new(&p.measures);
            let id = p.measures[index].id;
            let starting_here: Vec<u64> = p
                .notes
                .iter()
                .filter(|n| map.normalise(n.start).measure == id)
                .map(|n| n.id)
                .collect();
            let doomed = p.with_grace_notes(&starting_here);
            if !doomed.is_empty() && !confirmed {
                return Err(EditError::NeedsConfirmation {
                    notes: doomed.len(),
                });
            }

            let cut = match p.measures.get(index + 1) {
                Some(next) => TimePos::at(next.id, 0.0),
                None => TimePos::at(p.measures[index - 1].id, map.beats_in_measure(index - 1)),
            };
            for n in p.notes.iter_mut().filter(|n| !doomed.contains(&n.id)) {
                if map.normalise(n.end).measure == id {
                    n.end = cut;
                }
            }
            p.remove_note_set(&doomed);

            p.rebase_notes(policy, |p| {
                let removed = &p.measures[index];
                let shift = match p.measures.get(index + 1) {
                    Some(next) => next.start_sec - removed.start_sec,
                    None => removed.nominal_duration_sec(),
                };
                for m in &mut p.measures[index + 1..] {
                    m.start_sec -= shift;
                }
                p.measures.remove(index);
                Ok(())
            })
        })
    }

    /// Splits at a quarter-note beat that is a whole number of the signature's units. Both halves
    /// keep the tempo; notes stay at the same time.
    pub fn split_measure(&mut self, index: usize, at_beat: f64) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Split measure", NotePolicy::KeepAudioPosition, |p| {
            p.check_measure(index)?;
            let map = TempoMap::new(&p.measures);
            let sig = p.measures[index].time_sig;
            let units = at_beat * f64::from(sig.denominator) / 4.0;
            let whole = units.round();
            let split_ok = at_beat.is_finite()
                && (units - whole).abs() < EPS
                && whole >= 1.0
                && whole < f64::from(sig.numerator);
            if !split_ok {
                return Err(EditError::InvalidSplit);
            }
            let first_numerator = whole as u8;

            let nominal = p.measures[index].tempo_bpm;
            let effective = map.effective_bpm(index);
            let tempo_bpm = if (effective - nominal).abs() < EPS {
                nominal
            } else {
                effective
            };
            let start_sec = p.measures[index].start_sec + at_beat * 60.0 / effective;

            p.measures[index].time_sig = TimeSignature::new(first_numerator, sig.denominator);
            p.measures[index].tempo_bpm = tempo_bpm;
            let id = p.fresh_measure_id();
            let second = Measure::new(
                id,
                start_sec,
                tempo_bpm,
                TimeSignature::new(sig.numerator - first_numerator, sig.denominator),
            );
            p.measures.insert(index + 1, Measure { let_ring: p.measures[index].let_ring, ..second });
            Ok(())
        })
    }

    /// Merges `index` and `index + 1` into the first one (same denominator required); notes stay at
    /// the same time.
    pub fn merge_measures(&mut self, index: usize) -> Result<ProjectEdit, EditError> {
        self.grid_edit("Merge measures", NotePolicy::KeepAudioPosition, |p| {
            p.check_measure(index + 1)?;
            let map = TempoMap::new(&p.measures);
            let (a, b) = (p.measures[index].time_sig, p.measures[index + 1].time_sig);
            if a.denominator != b.denominator {
                return Err(EditError::IncompatibleTimeSignatures);
            }
            let merged_sig = a
                .numerator
                .checked_add(b.numerator)
                .map(|n| TimeSignature::new(n, a.denominator))
                .filter(|s| s.is_valid())
                .ok_or(EditError::IncompatibleTimeSignatures)?;

            let total_sec = map.beats_in_measure(index) * 60.0 / map.effective_bpm(index)
                + map.beats_in_measure(index + 1) * 60.0 / map.effective_bpm(index + 1);
            let realised = merged_sig.beats() * 60.0 / total_sec;
            let nominal = p.measures[index].tempo_bpm;
            let tempo_bpm = if (realised - nominal).abs() < EPS {
                nominal
            } else {
                realised.clamp(MIN_BPM, MAX_BPM)
            };
            p.measures[index].time_sig = merged_sig;
            p.measures[index].tempo_bpm = tempo_bpm;
            p.measures.remove(index + 1);
            Ok(())
        })
    }

    /// Grace notes of a removed main note go with it; links and legato sources pointing at a
    /// removed note are cleared.
    pub fn remove_notes(&mut self, ids: &[u64]) -> ProjectEdit {
        self.transact_infallible("Delete notes", |p| {
            let doomed = p.with_grace_notes(ids);
            p.remove_note_set(&doomed);
        })
    }

    pub(crate) fn with_grace_notes(&self, seeds: &[u64]) -> HashSet<u64> {
        let mut doomed: HashSet<u64> = seeds.iter().copied().collect();
        loop {
            let before = doomed.len();
            let graces: Vec<u64> = self
                .notes
                .iter()
                .filter(|n| n.grace_of.is_some_and(|g| doomed.contains(&g.main)))
                .map(|n| n.id)
                .collect();
            doomed.extend(graces);
            if doomed.len() == before {
                return doomed;
            }
        }
    }

    pub(crate) fn remove_note_set(&mut self, doomed: &HashSet<u64>) {
        self.notes.retain(|n| !doomed.contains(&n.id));
        for n in &mut self.notes {
            if n.link_to.is_some_and(|id| doomed.contains(&id)) {
                n.link_to = None;
                if n.technique.is_some_and(Technique::is_link) {
                    n.technique = None;
                }
            }
            if n.legato.is_some_and(|l| doomed.contains(&l.from)) {
                n.legato = None;
            }
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/edit.rs"]
mod tests;
