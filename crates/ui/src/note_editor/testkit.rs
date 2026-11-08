use fretscribe_core::{next_note_id, Note, Project, SnapGrid, TempoMap, TimePos, TimeSignature, Track, TrackRole};

use super::EditCtx;

pub const PX_PER_SEC: f64 = 200.0;

pub fn project() -> Project {
    Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 8.0, 120.0, TimeSignature::COMMON)
}

pub fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
    let id = next_note_id(p);
    p.notes.push(Note::new(id, midi, TimePos::new(1, start), TimePos::new(1, end)));
    id
}

pub fn at(beat: f64) -> TimePos {
    TimePos::new(1, beat)
}

/// Runs `f` with a context for `p` at 200 px/s (so one beat is 100 px) and a 12 px snap threshold.
pub fn with_ctx<R>(p: &Project, grid: &[u8], alt: bool, f: impl FnOnce(&EditCtx) -> R) -> R {
    let tempo = TempoMap::new(&p.measures);
    let grid = SnapGrid::new(grid);
    f(&EditCtx { project: p, tempo: &tempo, grid: &grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, onsets: None, alt })
}

/// Seconds of a beat in the first measure at 120 bpm.
pub fn sec(beat: f64) -> f64 {
    beat * 0.5
}
