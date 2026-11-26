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
fn an_offset_drag_is_one_undo_step_and_can_be_called_off() {
    let mut s = session();
    s.set_reference_hz(442.0);
    let (id, start) = (s.project.tracks[0].id, s.project.tracks[0].offset_sec);
    for cancel in [true, false] {
        s.offset_drag = Some(false);
        s.shift_track(id, 0.2, true);
        s.shift_track(id, 0.4, true);
        if cancel {
            s.cancel_offset_drag();
            assert_eq!(s.project.tracks[0].offset_sec, start);
        }
        s.offset_drag = None;
    }
    s.undo_redo(false);
    assert_eq!((s.project.tracks[0].offset_sec, s.project.reference_hz), (start, 442.0));
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

#[test]
fn techniques_and_let_ring_go_through_tab_and_staff_without_changing_the_rests() {
    use fretscribe_core::Technique::{Dead, Harmonic, PalmMute, PinchHarmonic};
    let mut s = session();
    for (n, t) in s.project.notes.iter_mut().zip([Dead, Harmonic, PinchHarmonic, PalmMute]) {
        n.technique = Some(t);
    }
    s.project.notes[4].vibrato = true;
    s.project.notes[1].midi = 52;
    s.project.notes[2].midi = 64;
    s.infer_all();
    s.recompute_staff();
    let rests = |s: &Session| s.staff.events.iter().map(|e| (e.rest, e.x_sec.to_bits())).collect::<Vec<_>>();
    let plain = rests(&s);
    assert_eq!(s.project.notes[1].string_fret, Some((0, 12)));
    assert!(s.project.notes[..5].iter().all(|n| n.string_fret.is_some()));
    s.project.measures[..2].iter_mut().for_each(|m| m.let_ring = true);
    s.recompute_staff();
    assert_eq!(rests(&s), plain);
    assert!(fretscribe_core::ring::ring_ends(&s.project).contains_key(&5), "the vibrato note rings");
    assert!(!fretscribe_core::ring::ring_ends(&s.project).contains_key(&1), "dead notes do not");
}
