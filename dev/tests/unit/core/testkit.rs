//! Fixtures shared by the tests of every crate (feature `test-support`; always compiled in core's own tests).

use crate::model::{Note, Project, TimePos, TimeSignature, Track, TrackRole};
use crate::tracks::next_note_id;

/// A one-track (guitar, id 1) project of `seconds` at 120 bpm in 4/4.
pub fn project(seconds: f64) -> Project {
    Project::new(Track::new(1, "Guitar", TrackRole::Guitar), seconds, 120.0, TimeSignature::COMMON)
}

/// `project` plus a drums and a bass track.
pub fn band(seconds: f64) -> Project {
    let mut p = project(seconds);
    p.add_track(Track::new(0, "Drums", TrackRole::Drums));
    p.add_track(Track::new(0, "Bass", TrackRole::Bass));
    p
}

/// Appends a note positioned relative to the first measure; returns its id.
pub fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
    let id = next_note_id(p);
    p.notes.push(Note::new(id, midi, TimePos::at(1, start), TimePos::at(1, end)));
    id
}
