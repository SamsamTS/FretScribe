//! Note-aware grid edits.
//!
//! Every edit runs as a transaction (a failure leaves the project untouched) and returns a
//! `ProjectEdit` that can be undone and redone. After the grid changes, each note time is resolved
//! according to the `NotePolicy`: `FollowGrid` keeps its `TimePos`, `KeepAudioPosition` keeps its
//! seconds. A time whose measure no longer exists is always re-resolved by seconds.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::EditError;
use crate::measures::{self, recompute_starts, valid_tempo, Propagation};
use crate::model::{Measure, Note, Project, TimePos, TimeSignature, Track, Technique, MAX_BPM, MIN_BPM};
use crate::tempo::{NoteTimes, TempoMap};

const MIN_MEASURE_SEC: f64 = 1e-3;
const SAME_TIME_EPS: f64 = 1e-9;
const INTEGER_EPS: f64 = 1e-9;

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

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    tracks: Vec<Track>,
    source_track: u64,
    measures: Vec<Measure>,
    next_measure_id: u64,
    notes: Vec<Note>,
}

impl Snapshot {
    fn capture(p: &Project) -> Self {
        Self {
            tracks: p.tracks.clone(),
            source_track: p.source_track,
            measures: p.measures.clone(),
            next_measure_id: p.next_measure_id,
            notes: p.notes.clone(),
        }
    }

    fn restore(&self, p: &mut Project) {
        p.tracks = self.tracks.clone();
        p.source_track = self.source_track;
        p.measures = self.measures.clone();
        p.next_measure_id = self.next_measure_id;
        p.notes = self.notes.clone();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEdit {
    label: String,
    before: Snapshot,
    after: Snapshot,
}

impl ProjectEdit {
    pub fn label(&self) -> &str {
        &self.label
    }

    /// One edit that undoes `self` and `later` together.
    pub fn merge(self, later: ProjectEdit) -> ProjectEdit {
        ProjectEdit {
            label: later.label,
            before: self.before,
            after: later.after,
        }
    }

    pub fn undo(&self, project: &mut Project) {
        self.before.restore(project);
    }

    pub fn redo(&self, project: &mut Project) {
        self.after.restore(project);
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
            .is_some_and(|sec| (sec - old_sec).abs() < SAME_TIME_EPS),
    };
    if unchanged {
        map.normalise(pos)
    } else {
        map.sec_to_pos(old_sec)
    }
}

impl Project {
    /// Runs `f` as one undoable edit; an error leaves the project untouched.
    pub fn edit_notes<F>(&mut self, label: &str, f: F) -> Result<ProjectEdit, EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        self.transact(label, f)
    }

    /// Like `edit_notes` for operations with their own error and result types.
    pub fn try_edit<T, E>(&mut self, label: &str, f: impl FnOnce(&mut Project) -> Result<T, E>) -> Result<(ProjectEdit, T), E> {
        let before = Snapshot::capture(self);
        match f(self) {
            Ok(value) => Ok((ProjectEdit { label: label.to_owned(), before, after: Snapshot::capture(self) }, value)),
            Err(e) => {
                before.restore(self);
                Err(e)
            }
        }
    }

    pub(crate) fn transact<F>(&mut self, label: &str, f: F) -> Result<ProjectEdit, EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        let before = Snapshot::capture(self);
        if let Err(e) = f(self) {
            before.restore(self);
            return Err(e);
        }
        Ok(ProjectEdit {
            label: label.to_owned(),
            before,
            after: Snapshot::capture(self),
        })
    }

    pub(crate) fn transact_infallible(&mut self, label: &str, f: impl FnOnce(&mut Project)) -> ProjectEdit {
        let before = Snapshot::capture(self);
        f(self);
        ProjectEdit {
            label: label.to_owned(),
            before,
            after: Snapshot::capture(self),
        }
    }

    fn rebase_notes<F>(&mut self, policy: NotePolicy, f: F) -> Result<(), EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        let old_times = NoteTimes::build(&self.notes, &TempoMap::new(&self.measures));
        f(self)?;
        let map = TempoMap::new(&self.measures);
        for note in &mut self.notes {
            if let (Some(start), Some(end)) = (old_times.start_sec(note.id), old_times.end_sec(note.id)) {
                note.start = resolve(&map, note.start, start, policy);
                note.end = resolve(&map, note.end, end, policy);
            }
        }
        Ok(())
    }

    fn grid_edit<F>(&mut self, label: &str, policy: NotePolicy, f: F) -> Result<ProjectEdit, EditError>
    where
        F: FnOnce(&mut Project) -> Result<(), EditError>,
    {
        self.transact(label, |p| p.rebase_notes(policy, f))
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

    pub fn set_tempo(
        &mut self,
        index: usize,
        new_bpm: f64,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.set_tempo_scoped(index, new_bpm, Propagation::Following, policy)
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

    pub fn set_time_sig(
        &mut self,
        index: usize,
        new_sig: TimeSignature,
        policy: NotePolicy,
    ) -> Result<ProjectEdit, EditError> {
        self.set_time_sig_scoped(index, new_sig, Propagation::Following, policy)
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
                });
            }
            p.measures = measures;
            Ok(())
        })
    }

    /// Inserts a measure (tempo and signature of the previous one) at `index`; every later measure,
    /// anchored or not, moves later by its duration.
    pub fn insert_measure(&mut self, index: usize, policy: NotePolicy) -> Result<ProjectEdit, EditError> {
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
        self.transact("Delete measure", |p| {
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
                return Err(EditError::NeedsConfirmation { notes: doomed.len() });
            }

            let cut = match p.measures.get(index + 1) {
                Some(next) => TimePos::new(next.id, 0.0),
                None => TimePos::new(p.measures[index - 1].id, map.beats_in_measure(index - 1)),
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
                && (units - whole).abs() < INTEGER_EPS
                && whole >= 1.0
                && whole < f64::from(sig.numerator);
            if !split_ok {
                return Err(EditError::InvalidSplit);
            }
            let first_numerator = whole as u8;

            let nominal = p.measures[index].tempo_bpm;
            let effective = map.effective_bpm(index);
            let tempo_bpm = if (effective - nominal).abs() < INTEGER_EPS { nominal } else { effective };
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
            p.measures.insert(index + 1, second);
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
            let tempo_bpm = if (realised - nominal).abs() < INTEGER_EPS {
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
        let before = Snapshot::capture(self);
        let doomed = self.with_grace_notes(ids);
        self.remove_note_set(&doomed);
        ProjectEdit {
            label: "Delete notes".to_owned(),
            before,
            after: Snapshot::capture(self),
        }
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
                if matches!(n.technique, Some(Technique::Slide | Technique::Bend)) {
                    n.technique = None;
                }
            }
            if n.legato.is_some_and(|l| doomed.contains(&l.from)) {
                n.legato = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measures::tests::close;
    use crate::model::{GraceKind, GraceNote, Legato, LegatoLink, TrackRole};
    use crate::tempo::NoteTimes;
    use NotePolicy::{FollowGrid as Follow, KeepAudioPosition as Keep};

    fn project(measures: usize) -> Project {
        Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            measures as f64 * 2.0,
            120.0,
            TimeSignature::COMMON,
        )
    }

    fn note(p: &mut Project, midi: u8, start: (u64, f64), end: (u64, f64)) -> u64 {
        let id = crate::tracks::next_note_id(p);
        p.notes.push(Note::new(
            id,
            midi,
            TimePos::new(start.0, start.1),
            TimePos::new(end.0, end.1),
        ));
        id
    }

    fn secs(p: &Project, id: u64) -> (f64, f64) {
        let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
        (times.start_sec(id).unwrap(), times.end_sec(id).unwrap())
    }

    fn pos(p: &Project, id: u64) -> (TimePos, TimePos) {
        let n = p.note(id).unwrap();
        (n.start, n.end)
    }

    fn ids(p: &Project) -> Vec<u64> {
        p.measures.iter().map(|m| m.id).collect()
    }

    #[test]
    fn follow_grid_keeps_beats_and_moves_seconds() {
        let mut p = project(4);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        assert_eq!(secs(&p, n), (2.5, 3.0));
        p.set_tempo(0, 60.0, Follow).unwrap();
        assert_eq!(pos(&p, n), (TimePos::new(2, 1.0), TimePos::new(2, 2.0)));
        assert_eq!(secs(&p, n), (5.0, 6.0));
    }

    #[test]
    fn keep_audio_position_keeps_seconds_and_changes_beats() {
        let mut p = project(4);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        p.set_tempo(0, 60.0, Keep).unwrap();
        let (s, e) = secs(&p, n);
        close(s, 2.5);
        close(e, 3.0);
        let (start, end) = pos(&p, n);
        assert_eq!(start.measure, 1);
        close(start.beat, 2.5);
        close(end.beat, 3.0);
    }

    #[test]
    fn keep_audio_position_leaves_untouched_notes_bit_exact() {
        let mut p = project(4);
        let third = 1.0 / 3.0;
        let n = note(&mut p, 60, (1, third), (1, 2.0 * third));
        p.set_tempo(3, 90.0, Keep).unwrap();
        assert_eq!(pos(&p, n), (TimePos::new(1, third), TimePos::new(1, 2.0 * third)));
    }

    #[test]
    fn note_on_beat_two_stays_on_beat_two_of_the_same_measure() {
        let mut p = project(6);
        let n = note(&mut p, 60, (5, 1.0), (5, 2.0));
        p.set_tempo(2, 90.0, Follow).unwrap();
        p.set_time_sig(0, TimeSignature::new(4, 4), Follow).unwrap();
        assert_eq!(p.note(n).unwrap().start, TimePos::new(5, 1.0));
    }

    #[test]
    fn tempo_change_with_propagation_moves_all_equal_measures() {
        let mut p = project(4);
        let n = note(&mut p, 60, (3, 0.0), (3, 4.0));
        p.set_tempo(1, 60.0, Follow).unwrap();
        assert_eq!(p.measures.iter().map(|m| m.tempo_bpm).collect::<Vec<_>>(), [120.0, 60.0, 60.0, 60.0]);
        let (s, e) = secs(&p, n);
        close(s, 6.0);
        close(e, 10.0);
        assert_eq!(pos(&p, n).1, TimePos::new(4, 0.0));
    }

    #[test]
    fn note_spanning_a_barline_follows_both_ends() {
        let mut p = project(3);
        let n = note(&mut p, 60, (1, 3.0), (2, 1.0));
        assert_eq!(secs(&p, n), (1.5, 2.5));
        p.set_tempo(0, 60.0, Follow).unwrap();
        assert_eq!(secs(&p, n), (3.0, 5.0));
        assert_eq!(pos(&p, n), (TimePos::new(1, 3.0), TimePos::new(2, 1.0)));
    }

    #[test]
    fn note_spanning_a_barline_with_keep_audio() {
        let mut p = project(3);
        let n = note(&mut p, 60, (1, 3.0), (2, 1.0));
        p.set_tempo(0, 60.0, Keep).unwrap();
        let (s, e) = secs(&p, n);
        close(s, 1.5);
        close(e, 2.5);
    }

    #[test]
    fn unnormalised_end_beat_is_normalised_after_a_time_signature_edit() {
        let mut p = project(3);
        let n = note(&mut p, 60, (1, 1.0), (1, 3.5));
        p.set_time_sig(0, TimeSignature::new(3, 4), Follow).unwrap();
        assert_eq!(p.note(n).unwrap().start, TimePos::new(1, 1.0));
        let end = p.note(n).unwrap().end;
        assert_eq!(end.measure, 2);
        close(end.beat, 0.5);
    }

    #[test]
    fn time_signature_edit_follow_and_keep() {
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        p.set_time_sig(0, TimeSignature::new(3, 4), Keep).unwrap();
        let (s, e) = secs(&p, n);
        close(s, 2.5);
        close(e, 3.0);
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        p.set_time_sig(0, TimeSignature::new(3, 4), Follow).unwrap();
        let (s, e) = secs(&p, n);
        close(s, 2.0);
        close(e, 2.5);
    }

    #[test]
    fn failed_edit_changes_nothing() {
        let mut p = project(3);
        note(&mut p, 60, (1, 0.0), (1, 1.0));
        let before = p.clone();
        assert!(p.set_tempo(9, 100.0, Follow).is_err());
        assert!(p.set_tempo(0, 0.0, Follow).is_err());
        assert!(p.delete_measure(0, Follow, false).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn undo_and_redo_restore_the_project() {
        let mut p = project(4);
        note(&mut p, 60, (2, 1.0), (2, 2.0));
        let before = p.clone();
        let edit = p.set_tempo(0, 60.0, Follow).unwrap();
        let after = p.clone();
        assert_ne!(after, before);
        edit.undo(&mut p);
        assert_eq!(p, before);
        edit.redo(&mut p);
        assert_eq!(p, after);
        assert_eq!(edit.label(), "Set tempo");
    }

    #[test]
    fn single_propagation_edits_only_the_chosen_measure() {
        let mut p = project(4);
        let n = note(&mut p, 60, (3, 0.0), (3, 1.0));
        let edit = p.set_tempo_scoped(0, 60.0, Propagation::Single, Follow).unwrap();
        let tempos: Vec<f64> = p.measures.iter().map(|m| m.tempo_bpm).collect();
        assert_eq!(tempos, [60.0, 120.0, 120.0, 120.0]);
        close(p.measures[1].start_sec, 4.0);
        close(secs(&p, n).0, 6.0);
        p.set_time_sig_scoped(1, TimeSignature::new(3, 4), Propagation::Single, Follow).unwrap();
        assert_eq!(p.measures[1].time_sig, TimeSignature::new(3, 4));
        assert_eq!(p.measures[2].time_sig, TimeSignature::COMMON);
        p.validate().unwrap();
        assert_eq!(edit.label(), "Set tempo");
    }

    #[test]
    fn inverted_policy() {
        assert_eq!(Follow.inverted(), Keep);
        assert_eq!(Keep.inverted(), Follow);
        assert_eq!(NotePolicy::default(), Follow);
    }

    #[test]
    fn move_measure_start_refits_previous_tempo_and_shifts_followers() {
        let mut p = project(4);
        let n = note(&mut p, 60, (3, 0.0), (3, 1.0));
        p.move_measure_start(2, 4.5, Follow).unwrap();
        close(p.measures[2].start_sec, 4.5);
        assert!(p.measures[2].anchored);
        close(p.measures[1].tempo_bpm, 4.0 * 60.0 / 2.5);
        close(p.measures[3].start_sec, 6.5);
        close(p.measures[0].start_sec, 0.0);
        close(secs(&p, n).0, 4.5);
    }

    #[test]
    fn move_measure_start_with_keep_audio() {
        let mut p = project(4);
        let n = note(&mut p, 60, (3, 0.0), (3, 1.0));
        p.move_measure_start(2, 4.5, Keep).unwrap();
        close(secs(&p, n).0, 4.0);
        close(secs(&p, n).1, 4.5);
    }

    #[test]
    fn move_measure_start_validation() {
        let mut p = project(3);
        assert!(p.move_measure_start(1, 0.0, Follow).is_err());
        assert!(p.move_measure_start(1, f64::NAN, Follow).is_err());
        assert!(p.move_measure_start(5, 1.0, Follow).is_err());
        assert!(p.move_measure_start(1, 100.0, Follow).is_err());
        p.move_measure_start(2, 4.5, Follow).unwrap();
        assert!(p.move_measure_start(1, 4.5, Follow).is_err());
    }

    #[test]
    fn auto_fit_replaces_grid_and_keeps_notes_on_the_audio() {
        let mut p = project(2);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        let (s0, e0) = secs(&p, n);
        let fitted = [
            FittedMeasure { start_sec: 0.2, tempo_bpm: 118.0, time_sig: TimeSignature::COMMON },
            FittedMeasure { start_sec: 0.2 + 4.0 * 60.0 / 118.0, tempo_bpm: 118.0, time_sig: TimeSignature::COMMON },
            FittedMeasure { start_sec: 0.2 + 8.0 * 60.0 / 118.0, tempo_bpm: 118.0, time_sig: TimeSignature::COMMON },
        ];
        p.auto_fit(&fitted, Keep).unwrap();
        assert_eq!(p.measures.len(), 3);
        assert_eq!(&ids(&p)[..2], [1, 2]);
        assert_eq!(p.measures[2].id, 3);
        assert!(p.measures.iter().all(|m| m.anchored));
        let (s1, e1) = secs(&p, n);
        close(s0, s1);
        close(e0, e1);
        assert_eq!(p.next_measure_id, 4);
    }

    #[test]
    fn auto_fit_dropping_measures_re_anchors_notes_by_seconds() {
        let mut p = project(4);
        let n = note(&mut p, 60, (4, 1.0), (4, 2.0));
        let (s0, _) = secs(&p, n);
        let fitted = [FittedMeasure { start_sec: 0.0, tempo_bpm: 60.0, time_sig: TimeSignature::COMMON }];
        p.auto_fit(&fitted, Follow).unwrap();
        assert_eq!(p.measures.len(), 1);
        close(secs(&p, n).0, s0);
        p.validate().unwrap();
    }

    #[test]
    fn auto_fit_validation() {
        let mut p = project(2);
        let ok = FittedMeasure { start_sec: 0.0, tempo_bpm: 120.0, time_sig: TimeSignature::COMMON };
        assert!(p.auto_fit(&[], Keep).is_err());
        assert!(p.auto_fit(&[ok, ok], Keep).is_err());
        assert!(p.auto_fit(&[FittedMeasure { tempo_bpm: 1.0, ..ok }], Keep).is_err());
        assert!(p.auto_fit(&[FittedMeasure { time_sig: TimeSignature::new(4, 5), ..ok }], Keep).is_err());
    }

    #[test]
    fn insert_measure_keeps_ids_stable_and_notes_in_their_measures() {
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        p.measures[2].tempo_bpm = 90.0;
        let edit = p.insert_measure(1, Follow).unwrap();
        assert_eq!(ids(&p), [1, 4, 2, 3]);
        assert_eq!(p.next_measure_id, 5);
        assert_eq!(pos(&p, n), (TimePos::new(2, 1.0), TimePos::new(2, 2.0)));
        close(secs(&p, n).0, 4.5);
        close(p.measures[1].start_sec, 2.0);
        close(p.measures[2].start_sec, 4.0);
        p.validate().unwrap();
        edit.undo(&mut p);
        assert_eq!(ids(&p), [1, 2, 3]);
        assert_eq!(p.next_measure_id, 4);
    }

    #[test]
    fn insert_measure_at_the_ends() {
        let mut p = project(2);
        p.insert_measure(0, Follow).unwrap();
        p.insert_measure(3, Follow).unwrap();
        assert_eq!(ids(&p), [3, 1, 2, 4]);
        close(p.measures[0].start_sec, 0.0);
        close(p.measures[1].start_sec, 2.0);
        close(p.measures[3].start_sec, 6.0);
        assert!(p.insert_measure(9, Follow).is_err());
    }

    #[test]
    fn insert_measure_with_keep_audio_holds_seconds() {
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        p.insert_measure(1, Keep).unwrap();
        close(secs(&p, n).0, 2.5);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut p = project(3);
        p.delete_measure(2, Follow, false).unwrap();
        p.insert_measure(2, Follow).unwrap();
        assert_eq!(ids(&p), [1, 2, 4]);
        p.delete_measure(0, Follow, false).unwrap();
        p.insert_measure(0, Follow).unwrap();
        assert_eq!(ids(&p), [5, 2, 4]);
    }

    #[test]
    fn delete_measure_without_notes_closes_the_gap() {
        let mut p = project(4);
        let n = note(&mut p, 60, (4, 0.0), (4, 1.0));
        p.delete_measure(1, Follow, false).unwrap();
        assert_eq!(ids(&p), [1, 3, 4]);
        close(p.measures[1].start_sec, 2.0);
        close(secs(&p, n).0, 4.0);
        p.validate().unwrap();
    }

    #[test]
    fn delete_measure_with_notes_needs_confirmation() {
        let mut p = project(3);
        note(&mut p, 60, (2, 0.0), (2, 1.0));
        note(&mut p, 62, (2, 2.0), (2, 3.0));
        let keep = note(&mut p, 64, (3, 0.0), (3, 1.0));
        let before = p.clone();
        assert_eq!(
            p.delete_measure(1, Follow, false).unwrap_err(),
            EditError::NeedsConfirmation { notes: 2 }
        );
        assert_eq!(p, before);
        p.delete_measure(1, Follow, true).unwrap();
        assert_eq!(p.notes.len(), 1);
        assert_eq!(p.notes[0].id, keep);
        p.validate().unwrap();
    }

    #[test]
    fn delete_measure_cascades_to_grace_notes_and_clears_links() {
        let mut p = project(3);
        let main = note(&mut p, 60, (2, 1.0), (2, 2.0));
        let grace = note(&mut p, 59, (2, 0.9), (2, 1.0));
        p.notes[1].grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
        let slide_start = note(&mut p, 50, (1, 0.0), (1, 1.0));
        p.notes[2].technique = Some(Technique::Slide);
        p.notes[2].link_to = Some(main);
        let hammer = note(&mut p, 62, (3, 0.0), (3, 1.0));
        p.notes[3].legato = Some(LegatoLink { kind: Legato::HammerOn, from: main });
        p.delete_measure(1, Follow, true).unwrap();
        assert!(p.note(main).is_none() && p.note(grace).is_none());
        let start = p.note(slide_start).unwrap();
        assert_eq!((start.link_to, start.technique), (None, None));
        assert_eq!(p.note(hammer).unwrap().legato, None);
        p.validate().unwrap();
    }

    #[test]
    fn grace_note_of_a_surviving_main_is_untouched() {
        let mut p = project(3);
        let main = note(&mut p, 60, (1, 1.0), (1, 2.0));
        let grace = note(&mut p, 59, (1, 0.9), (1, 1.0));
        p.notes[1].grace_of = Some(GraceNote { main, kind: GraceKind::Appoggiatura });
        p.delete_measure(2, Follow, false).unwrap();
        assert!(p.note(grace).is_some());
    }

    #[test]
    fn delete_measure_cuts_notes_that_only_end_inside_it() {
        let mut p = project(3);
        let n = note(&mut p, 60, (1, 3.0), (2, 2.0));
        p.delete_measure(1, Follow, false).unwrap();
        assert_eq!(pos(&p, n).1, TimePos::new(3, 0.0));
        close(secs(&p, n).1, 2.0);
        p.validate().unwrap();
    }

    #[test]
    fn delete_last_measure_cuts_at_previous_end() {
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 3.0), (3, 2.0));
        p.delete_measure(2, Follow, false).unwrap();
        close(secs(&p, n).1, 4.0);
        p.validate().unwrap();
    }

    #[test]
    fn delete_first_measure_keeps_grid_start() {
        let mut p = project(3);
        p.delete_measure(0, Follow, false).unwrap();
        close(p.measures[0].start_sec, 0.0);
        close(p.measures[1].start_sec, 2.0);
    }

    #[test]
    fn cannot_delete_the_only_measure() {
        let mut p = project(1);
        assert_eq!(p.delete_measure(0, Follow, true).unwrap_err(), EditError::LastMeasure);
    }

    #[test]
    fn delete_with_keep_audio_holds_surviving_seconds() {
        let mut p = project(4);
        let n = note(&mut p, 60, (4, 0.0), (4, 1.0));
        p.delete_measure(1, Keep, false).unwrap();
        close(secs(&p, n).0, 6.0);
    }

    #[test]
    fn split_keeps_every_note_at_the_same_time() {
        let mut p = project(2);
        let a = note(&mut p, 60, (1, 1.0), (1, 1.5));
        let b = note(&mut p, 62, (1, 2.5), (1, 3.5));
        let c = note(&mut p, 64, (2, 3.0), (2, 4.0));
        let before: Vec<_> = [a, b, c].iter().map(|&n| secs(&p, n)).collect();
        p.split_measure(0, 2.0).unwrap();
        assert_eq!(ids(&p), [1, 3, 2]);
        assert_eq!(p.measures[0].time_sig, TimeSignature::new(2, 4));
        assert_eq!(p.measures[1].time_sig, TimeSignature::new(2, 4));
        close(p.measures[1].start_sec, 1.0);
        for (&n, (s, e)) in [a, b, c].iter().zip(before) {
            let (s2, e2) = secs(&p, n);
            close(s, s2);
            close(e, e2);
        }
        assert_eq!(p.note(a).unwrap().start, TimePos::new(1, 1.0));
        assert_eq!(p.note(b).unwrap().start.measure, 3);
        close(p.note(b).unwrap().start.beat, 0.5);
        p.validate().unwrap();
    }

    #[test]
    fn split_six_eight_and_validation() {
        let mut p = project(1);
        p.set_time_sig(0, TimeSignature::new(6, 8), Follow).unwrap();
        assert_eq!(p.split_measure(0, 1.5).map(|e| e.label().to_owned()).unwrap(), "Split measure");
        assert_eq!(p.measures[0].time_sig, TimeSignature::new(3, 8));
        assert_eq!(p.measures[1].time_sig, TimeSignature::new(3, 8));
        let mut p = project(1);
        assert_eq!(p.split_measure(0, 1.5).unwrap_err(), EditError::InvalidSplit);
        assert_eq!(p.split_measure(0, 0.0).unwrap_err(), EditError::InvalidSplit);
        assert_eq!(p.split_measure(0, 4.0).unwrap_err(), EditError::InvalidSplit);
        assert_eq!(p.split_measure(0, f64::NAN).unwrap_err(), EditError::InvalidSplit);
        assert!(p.split_measure(4, 2.0).is_err());
    }

    #[test]
    fn merge_keeps_every_note_at_the_same_time() {
        let mut p = project(3);
        let a = note(&mut p, 60, (1, 1.0), (1, 1.5));
        let b = note(&mut p, 62, (2, 0.0), (2, 1.0));
        let c = note(&mut p, 64, (3, 1.0), (3, 2.0));
        let before: Vec<_> = [a, b, c].iter().map(|&n| secs(&p, n)).collect();
        p.merge_measures(0).unwrap();
        assert_eq!(ids(&p), [1, 3]);
        assert_eq!(p.measures[0].time_sig, TimeSignature::new(8, 4));
        close(p.measures[0].tempo_bpm, 120.0);
        for (&n, (s, e)) in [a, b, c].iter().zip(before) {
            let (s2, e2) = secs(&p, n);
            close(s, s2);
            close(e, e2);
        }
        assert_eq!(p.note(b).unwrap().start, TimePos::new(1, 4.0));
        p.validate().unwrap();
    }

    #[test]
    fn merge_with_different_tempos_preserves_total_duration_and_note_seconds() {
        let mut p = project(3);
        p.measures[1].tempo_bpm = 60.0;
        crate::measures::recompute_starts(&mut p.measures, 1);
        let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
        let before = secs(&p, n);
        let third_start = p.measures[2].start_sec;
        p.merge_measures(0).unwrap();
        close(p.measures[0].tempo_bpm, 8.0 * 60.0 / 6.0);
        close(p.measures[1].start_sec, third_start);
        let after = secs(&p, n);
        close(before.0, after.0);
        close(before.1, after.1);
    }

    #[test]
    fn merge_validation() {
        let mut p = project(2);
        p.measures[1].time_sig = TimeSignature::new(6, 8);
        assert_eq!(p.merge_measures(0).unwrap_err(), EditError::IncompatibleTimeSignatures);
        assert!(project(2).merge_measures(1).is_err());
        let mut p = project(2);
        p.measures[0].time_sig = TimeSignature::new(40, 4);
        p.measures[1].time_sig = TimeSignature::new(40, 4);
        assert_eq!(p.merge_measures(0).unwrap_err(), EditError::IncompatibleTimeSignatures);
    }

    #[test]
    fn split_then_merge_round_trips_the_grid() {
        let mut p = project(3);
        let n = note(&mut p, 60, (2, 2.5), (2, 3.5));
        let grid_before = p.measures.clone();
        p.split_measure(1, 2.0).unwrap();
        p.merge_measures(1).unwrap();
        assert_eq!(p.measures.len(), 3);
        for (a, b) in p.measures.iter().zip(&grid_before) {
            assert_eq!(a.id, b.id);
            close(a.start_sec, b.start_sec);
            close(a.tempo_bpm, b.tempo_bpm);
            assert_eq!(a.time_sig, b.time_sig);
        }
        assert_eq!(p.note(n).unwrap().start.measure, 2);
        close(p.note(n).unwrap().start.beat, 2.5);
    }

    #[test]
    fn remove_notes_cascades_and_is_undoable() {
        let mut p = project(2);
        let main = note(&mut p, 60, (1, 1.0), (1, 2.0));
        let grace = note(&mut p, 59, (1, 0.9), (1, 1.0));
        p.notes[1].grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
        let before = p.clone();
        let edit = p.remove_notes(&[main]);
        assert!(p.notes.is_empty());
        edit.undo(&mut p);
        assert_eq!(p, before);
        assert!(p.note(grace).is_some());
    }

    #[test]
    fn json_round_trip_after_edits() {
        let mut p = project(4);
        note(&mut p, 60, (2, 1.0 / 3.0), (3, 0.5));
        p.set_tempo(1, 97.5, Follow).unwrap();
        p.insert_measure(2, Follow).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
    }
}
