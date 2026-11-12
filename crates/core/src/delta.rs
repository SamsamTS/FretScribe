//! The undoable difference between two project states.
//!
//! A `Snapshot` is captured before an edit and only lives for its duration; afterwards it is diffed
//! against the project into a `Delta`. Notes are keyed by id (`Project::notes` is sorted by id); the
//! other undoable groups are stored whole, only when they changed. Live track fields are never part
//! of a delta's effect (see `Track`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::UndoError;
use crate::model::{Measure, Note, Project, Track, Tuning};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    tracks: Vec<Track>,
    source_track: u64,
    measures: Vec<Measure>,
    next_measure_id: u64,
    notes: Vec<Note>,
    tuning: Tuning,
    capo: u8,
    reference_hz: f32,
}

impl Snapshot {
    pub(crate) fn capture(p: &Project) -> Self {
        Self {
            tracks: p.tracks.clone(),
            source_track: p.source_track,
            measures: p.measures.clone(),
            next_measure_id: p.next_measure_id,
            notes: p.notes.clone(),
            tuning: p.tuning.clone(),
            capo: p.capo,
            reference_hz: p.reference_hz,
        }
    }

    pub(crate) fn restore(&self, p: &mut Project) {
        p.tracks = self.tracks.clone();
        p.source_track = self.source_track;
        p.measures = self.measures.clone();
        p.next_measure_id = self.next_measure_id;
        p.notes = self.notes.clone();
        p.tuning = self.tuning.clone();
        p.capo = self.capo;
        p.reference_hz = self.reference_hz;
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Change<T> {
    before: T,
    after: T,
}

impl<T> Change<T> {
    fn new(before: T, after: T, same: impl Fn(&T, &T) -> bool) -> Option<Self> {
        (!same(&before, &after)).then_some(Self { before, after })
    }

    fn expected(&self, undo: bool) -> &T {
        if undo { &self.after } else { &self.before }
    }

    fn target(&self, undo: bool) -> &T {
        self.expected(!undo)
    }

    /// First `before`, last `after`; `None` when they end up equal.
    fn compose(first: Option<Self>, later: Option<Self>, same: impl Fn(&T, &T) -> bool) -> Option<Self> {
        match (first, later) {
            (Some(a), Some(b)) => Self::new(a.before, b.after, same),
            (a, b) => a.or(b),
        }
    }
}

fn holds<T>(change: &Option<Change<T>>, undo: bool, now: impl FnOnce(&T) -> bool) -> bool {
    change.as_ref().is_none_or(|c| now(c.expected(undo)))
}

#[allow(clippy::ptr_arg)] // used as `Fn(&Vec<Track>, &Vec<Track>)`
fn same_tracks(a: &Vec<Track>, b: &Vec<Track>) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_undoable(y))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Grid {
    measures: Vec<Measure>,
    next_id: u64,
}

type NoteChange = (u64, Change<Option<Note>>);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Delta {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<NoteChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tracks: Option<Change<Vec<Track>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_track: Option<Change<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grid: Option<Change<Grid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tuning: Option<Change<Tuning>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capo: Option<Change<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_hz: Option<Change<f32>>,
}

impl Delta {
    pub(crate) fn diff(before: &Snapshot, after: &Project) -> Self {
        let old: HashMap<u64, &Note> = before.notes.iter().map(|n| (n.id, n)).collect();
        let new: HashMap<u64, &Note> = after.notes.iter().map(|n| (n.id, n)).collect();
        let ids: BTreeSet<u64> = old.keys().chain(new.keys()).copied().collect();
        let notes = ids
            .into_iter()
            .filter_map(|id| {
                let (b, a) = (old.get(&id).copied(), new.get(&id).copied());
                (a != b).then(|| (id, Change { before: b.cloned(), after: a.cloned() }))
            })
            .collect();
        let grid = |measures: &[Measure], next_id| Grid { measures: measures.to_vec(), next_id };
        Self {
            notes,
            tracks: Change::new(before.tracks.clone(), after.tracks.clone(), same_tracks),
            source_track: Change::new(before.source_track, after.source_track, u64::eq),
            grid: Change::new(
                grid(&before.measures, before.next_measure_id),
                grid(&after.measures, after.next_measure_id),
                Grid::eq,
            ),
            tuning: Change::new(before.tuning.clone(), after.tuning.clone(), Tuning::eq),
            capo: Change::new(before.capo, after.capo, u8::eq),
            reference_hz: Change::new(before.reference_hz, after.reference_hz, f32::eq),
        }
    }

    /// One delta equivalent to `self` followed by `later`.
    pub(crate) fn compose(self, later: Delta) -> Delta {
        let mut notes: BTreeMap<u64, Change<Option<Note>>> = self.notes.into_iter().collect();
        for (id, c) in later.notes {
            match notes.remove(&id) {
                Some(first) => notes.extend(Change::new(first.before, c.after, Option::eq).map(|c| (id, c))),
                None => {
                    notes.insert(id, c);
                }
            }
        }
        Delta {
            notes: notes.into_iter().collect(),
            tracks: Change::compose(self.tracks, later.tracks, same_tracks),
            source_track: Change::compose(self.source_track, later.source_track, u64::eq),
            grid: Change::compose(self.grid, later.grid, Grid::eq),
            tuning: Change::compose(self.tuning, later.tuning, Tuning::eq),
            capo: Change::compose(self.capo, later.capo, u8::eq),
            reference_hz: Change::compose(self.reference_hz, later.reference_hz, f32::eq),
        }
    }

    /// Every touched item must currently equal what the delta expects.
    fn expects(&self, p: &Project, undo: bool) -> bool {
        let note_at = |id: u64| p.notes.binary_search_by_key(&id, |n| n.id).ok().map(|i| &p.notes[i]);
        self.notes.iter().all(|(id, c)| note_at(*id) == c.expected(undo).as_ref())
            && holds(&self.tracks, undo, |t| same_tracks(&p.tracks, t))
            && holds(&self.source_track, undo, |s| p.source_track == *s)
            && holds(&self.grid, undo, |g| p.measures == g.measures && p.next_measure_id == g.next_id)
            && holds(&self.tuning, undo, |t| p.tuning == *t)
            && holds(&self.capo, undo, |c| p.capo == *c)
            && holds(&self.reference_hz, undo, |r| p.reference_hz == *r)
    }

    /// Applies nothing unless the project still matches the delta (`UndoError::Diverged`).
    pub(crate) fn apply(&self, p: &mut Project, undo: bool) -> Result<(), UndoError> {
        if !self.expects(p, undo) {
            return Err(UndoError::Diverged);
        }
        if !self.notes.is_empty() {
            let changed: HashSet<u64> = self.notes.iter().map(|(id, _)| *id).collect();
            p.notes.retain(|n| !changed.contains(&n.id));
            for (id, c) in &self.notes {
                if let Some(note) = c.target(undo) {
                    let at = p.notes.partition_point(|n| n.id < *id);
                    p.notes.insert(at, note.clone());
                }
            }
        }
        if let Some(c) = &self.tracks {
            let current = std::mem::take(&mut p.tracks);
            p.tracks = c
                .target(undo)
                .iter()
                .map(|t| current.iter().find(|x| x.id == t.id).map_or_else(|| t.clone(), |x| t.with_live_of(x)))
                .collect();
        }
        if let Some(c) = &self.source_track {
            p.source_track = *c.target(undo);
        }
        if let Some(c) = &self.grid {
            let g = c.target(undo);
            (p.measures, p.next_measure_id) = (g.measures.clone(), g.next_id);
        }
        if let Some(c) = &self.tuning {
            p.tuning = c.target(undo).clone();
        }
        if let Some(c) = &self.capo {
            p.capo = *c.target(undo);
        }
        if let Some(c) = &self.reference_hz {
            p.reference_hz = *c.target(undo);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::edit::{NotePolicy, ProjectEdit};
    use crate::model::{TimePos, TimeSignature, TrackRole};
    use crate::tracks::{next_note_id, shift_track};

    pub fn project() -> Project {
        let mut p = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 8.0, 120.0, TimeSignature::COMMON);
        p.add_track(Track::new(0, "Drums", TrackRole::Drums));
        p.add_track(Track::new(0, "Bass", TrackRole::Bass));
        p
    }

    pub fn add_note(p: &mut Project, midi: u8) -> ProjectEdit {
        p.edit_notes("Add note", |p| {
            let id = next_note_id(p);
            p.notes.push(Note::new(id, midi, TimePos::new(1, 0.0), TimePos::new(1, 1.0)));
            Ok(())
        })
        .unwrap()
    }

    pub fn set_capo(p: &mut Project, capo: u8) -> ProjectEdit {
        p.transact_infallible("Capo", |p| p.capo = capo)
    }

    #[test]
    fn notes_stay_sorted_by_id_through_undo_and_redo() {
        let mut p = project();
        for midi in 60..66 {
            add_note(&mut p, midi);
        }
        let full = p.clone();
        let removed = p.remove_notes(&[2, 4]);
        let added = add_note(&mut p, 70);
        added.undo(&mut p);
        removed.undo(&mut p);
        assert_eq!(p, full);
        removed.redo(&mut p);
        assert!(p.notes.windows(2).all(|w| w[0].id < w[1].id));
    }

    #[test]
    fn live_track_fields_survive_undo_and_redo_but_the_rest_reverts() {
        let mut p = project();
        let note = add_note(&mut p, 60);
        let shift = shift_track(&mut p, 1, 0.5, false).unwrap();
        let rename = p.transact_infallible("Rename", |p| p.tracks[1].name = "Kick".into());
        let reorder = p.transact_infallible("Reorder", |p| p.move_track(3, 0).unwrap());
        p.tracks[0].volume = 0.25;
        p.tracks[1].muted = true;
        p.tracks[2].solo = true;
        p.tracks[0].spectrogram_collapsed = true;
        let live_now = |p: &Project| -> Vec<_> {
            let mut v: Vec<_> = p.tracks.iter().map(|t| (t.id, t.volume, t.muted, t.solo, t.spectrogram_collapsed)).collect();
            v.sort_by_key(|t| t.0);
            v
        };
        let sorted = live_now(&p);

        reorder.undo(&mut p);
        assert_eq!(p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
        rename.undo(&mut p);
        assert_eq!(p.tracks[1].name, "Drums");
        shift.undo(&mut p);
        assert_eq!(p.tracks[0].offset_sec, 0.0);
        note.undo(&mut p);
        assert!(p.notes.is_empty());
        assert_eq!(live_now(&p), sorted);
        note.redo(&mut p);
        shift.redo(&mut p);
        assert_eq!((p.tracks[0].offset_sec, p.notes.len()), (0.5, 1));
        assert_eq!(live_now(&p), sorted);

        let merged = note.clone().merge(shift.clone());
        merged.undo(&mut p);
        assert_eq!((p.tracks[0].offset_sec, p.notes.len()), (0.0, 0));
        assert_eq!(live_now(&p), sorted);
    }

    #[test]
    fn removing_a_track_is_undone_with_the_delta_live_values_and_the_source() {
        let mut p = project();
        p.tracks[0].set_volume(0.7);
        p.tracks[0].muted = true;
        let (edit, _) = p.try_edit("Remove track", |p| p.remove_track(1)).unwrap();
        assert_eq!(p.source_track, 2);
        edit.undo(&mut p);
        assert_eq!(p.source_track, 1);
        assert_eq!((p.tracks[0].id, p.tracks[0].volume, p.tracks[0].muted), (1, 0.7, true));
    }

    #[test]
    fn reference_pitch_is_undoable_and_validated() {
        let mut p = project();
        add_note(&mut p, 60);
        let before = p.clone();
        let edit = p.set_reference(432.0).unwrap();
        assert_eq!((p.reference_hz, p.notes.clone()), (432.0, before.notes.clone()));
        edit.undo(&mut p);
        assert_eq!(p, before);
        edit.redo(&mut p);
        assert_eq!(p.reference_hz, 432.0);
        let current = p.clone();
        assert!(p.set_reference(500.0).is_err() && p.set_reference(f32::NAN).is_err());
        assert_eq!(p, current);
    }

    #[test]
    fn a_project_changed_behind_the_history_is_left_alone() {
        let mut p = project();
        let edit = add_note(&mut p, 60);
        p.notes[0].midi = 61;
        let changed = p.clone();
        assert_eq!(edit.try_undo(&mut p), Err(UndoError::Diverged));
        edit.undo(&mut p);
        assert_eq!(p, changed);

        let mut p = project();
        let edit = add_note(&mut p, 60);
        p.tracks[0].volume = 0.1;
        p.tracks[1].muted = true;
        assert_eq!(edit.try_undo(&mut p), Ok(()));
        let undone = set_capo(&mut p, 3);
        p.capo = 4;
        assert_eq!(undone.try_undo(&mut p), Err(UndoError::Diverged));
        assert_eq!(p.capo, 4);
    }

    #[test]
    fn deltas_are_a_small_fraction_of_a_snapshot() {
        let mut p = project();
        p.notes = (1..=3000).map(|id| Note::new(id, 60, TimePos::new(1, 0.0), TimePos::new(1, 1.0))).collect();
        let snapshot = serde_json::to_string(&p).unwrap().len();
        let total: usize = (0..200u64)
            .map(|k| {
                let at = (1 + k * 10) as usize - 1;
                let edit = p.edit_notes("Transpose", |p| {
                    p.notes[at].midi = 61 + (k % 5) as u8;
                    Ok(())
                }).unwrap();
                serde_json::to_string(&edit).unwrap().len()
            })
            .sum();
        assert!(total < snapshot / 4, "{total} vs {snapshot}");
    }

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as usize % n
        }
    }

    #[test]
    fn random_mixed_edits_round_trip_and_merge() {
        let mut p = project();
        let mut rng = Lcg(7);
        let (mut states, mut edits) = (vec![p.clone()], Vec::new());
        for _ in 0..200 {
            let m = rng.next(p.measures.len());
            let edit = match rng.next(9) {
                0..=2 => {
                    let (id, beat, midi) = (p.measures[m].id, rng.next(4) as f64, 40 + rng.next(40) as u8);
                    p.edit_notes("Add note", |p| {
                        let n = next_note_id(p);
                        p.notes.push(Note::new(n, midi, TimePos::new(id, beat), TimePos::new(id, beat + 0.5)));
                        Ok(())
                    })
                    .unwrap()
                }
                3 if !p.notes.is_empty() => {
                    let id = p.notes[rng.next(p.notes.len())].id;
                    p.remove_notes(&[id])
                }
                4 => p.set_tempo(m, 60.0 + rng.next(100) as f64, NotePolicy::FollowGrid).unwrap(),
                5 => p.insert_measure(m, NotePolicy::KeepAudioPosition).unwrap(),
                6 if p.measures.len() > 2 => p.delete_measure(m, NotePolicy::KeepAudioPosition, true).unwrap(),
                7 if p.tracks.len() < 5 => p.transact_infallible("Add track", |p| {
                    p.add_track(Track::new(0, "T", TrackRole::Other));
                }),
                7 => {
                    let id = p.tracks[rng.next(p.tracks.len())].id;
                    match p.try_edit("Remove track", |p| p.remove_track(id)) {
                        Ok((edit, _)) => edit,
                        Err(_) => continue,
                    }
                }
                8 => p.set_reference(430.0 + rng.next(20) as f32).unwrap(),
                _ => set_capo(&mut p, rng.next(5) as u8),
            };
            edits.push(edit);
            states.push(p.clone());
        }
        assert!(edits.len() > 100);
        for (i, pair) in edits.windows(2).enumerate() {
            let merged = pair[0].clone().merge(pair[1].clone());
            let mut q = states[i].clone();
            merged.try_redo(&mut q).unwrap();
            assert_eq!(q, states[i + 2], "merge {i}");
            merged.try_undo(&mut q).unwrap();
            assert_eq!(q, states[i], "unmerge {i}");
        }
        for edit in edits.iter().rev() {
            edit.try_undo(&mut p).unwrap();
        }
        assert_eq!(p, states[0]);
        for edit in &edits {
            edit.try_redo(&mut p).unwrap();
        }
        assert_eq!(Some(&p), states.last());
    }
}
