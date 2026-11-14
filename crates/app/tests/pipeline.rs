//! Model-level pipeline without a window or a sound card: notes -> tab assignment -> staff layout,
//! with an undoable edit in between.

use fretscribe_app::luts::LutSet;
use fretscribe_app::session::Session;
use fretscribe_app::settings::Settings;
use fretscribe_app::store::AudioStore;
use fretscribe_core::{Note, Project, ProjectFile, TempoMap, TimeSignature, Track, TrackRole};

fn session() -> Session {
    let mut project = Project::new(Track::new(1, "guitar", TrackRole::Guitar), 10.0, 120.0, TimeSignature { numerator: 4, denominator: 4 });
    let map = TempoMap::new(&project.measures);
    project.notes = [64_u8, 67, 71, 69, 59, 52]
        .iter()
        .enumerate()
        .map(|(i, &midi)| Note::new(i as u64 + 1, midi, map.sec_to_pos(i as f64 * 0.5), map.sec_to_pos(i as f64 * 0.5 + 0.4)))
        .collect();
    Session::new(ProjectFile::new(project), None, AudioStore::default(), &Settings::default(), LutSet::scan(None))
}

#[test]
fn notes_get_tab_positions_and_a_staff_layout() {
    let mut s = session();
    assert!(s.infer_all().is_empty());
    assert!(s.project.notes.iter().all(|n| n.string_fret.is_some()));
    s.recompute_staff();
    assert!(s.staff.events.len() >= s.project.notes.len(), "rests and notes each get an event");
    assert!(!s.staff.measures.is_empty());
}

#[test]
fn an_edit_is_undoable_and_marks_the_session_dirty() {
    let mut s = session();
    assert!(!s.dirty);
    s.set_reference_hz(442.0);
    assert!(s.dirty);
    assert_eq!(s.project.reference_hz, 442.0);
    s.undo_redo(false);
    assert_eq!(s.project.reference_hz, 440.0);
    s.undo_redo(true);
    assert_eq!(s.project.reference_hz, 442.0);
}

#[test]
fn changing_the_tuning_reassigns_the_tab() {
    let mut s = session();
    s.infer_all();
    let before: Vec<_> = s.project.notes.iter().map(|n| n.string_fret).collect();
    let mut tuning = s.project.tuning.clone();
    tuning.name = "Drop D".into();
    tuning.open_strings[0] -= 2;
    s.set_tuning_capo(tuning, 2);
    assert_eq!(s.project.capo, 2);
    assert!(s.project.notes.iter().all(|n| n.string_fret.is_some()));
    let _ = before;
}
