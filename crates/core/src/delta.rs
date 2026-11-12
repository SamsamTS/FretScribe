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

    #[cfg(test)]
    pub(crate) fn note_count(&self) -> usize {
        self.notes.len()
    }
}
