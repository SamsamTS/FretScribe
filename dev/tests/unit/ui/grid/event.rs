use super::*;
use fretscribe_core::{Note, TimePos, UndoStack};

const FOLLOW: NotePolicy = NotePolicy::FollowGrid;

fn project() -> Project {
    fretscribe_core::testkit::project(8.0)
}

fn apply_all(events: &[GridEvent], p: &mut Project, undo: &mut UndoStack) {
    for e in events {
        if let Some(result) = e.apply(p, false) {
            undo.push(result.unwrap());
        }
    }
}

#[test]
fn events_edit_the_project_and_undo_restores_it() {
    let mut p = project();
    let original = p.clone();
    let mut undo = UndoStack::new();
    apply_all(
        &[
            GridEvent::TempoApplied { index: 0, bpm: 60.0, propagate: true, policy: FOLLOW },
            GridEvent::TimeSigApplied { index: 1, sig: TimeSignature::new(3, 4), propagate: false, policy: FOLLOW },
            GridEvent::LetRingToggled { index: 0, on: true, propagate: true },
            GridEvent::LockToggled { index: 3, on: true, propagate: false },
            GridEvent::LockToggled { index: 3, on: false, propagate: false },
            GridEvent::MeasureMoved { index: 2, new_start_sec: 9.5, policy: FOLLOW },
            GridEvent::InsertMeasure { index: 1, policy: FOLLOW },
            GridEvent::SplitMeasure { index: 0, at_beat: 2.0 },
            GridEvent::MergeMeasures { index: 0 },
        ],
        &mut p,
        &mut undo,
    );
    assert_ne!(p, original);
    while undo.undo(&mut p).is_some() {}
    assert_eq!(p, original);
}

#[test]
fn tempo_event_honours_the_propagation_choice() {
    let tempos = |propagate| {
        let mut p = project();
        let e = GridEvent::TempoApplied { index: 0, bpm: 90.0, propagate, policy: FOLLOW };
        e.apply(&mut p, false).unwrap().unwrap();
        p.measures.iter().map(|m| m.tempo_bpm).collect::<Vec<_>>()
    };
    assert_eq!(tempos(true), [90.0; 4]);
    assert_eq!(tempos(false), [90.0, 120.0, 120.0, 120.0]);
}

#[test]
fn delete_needs_confirmation_when_notes_would_go() {
    let mut p = project();
    let id = p.measures[1].id;
    p.notes.push(Note::new(1, 60, TimePos::at(id, 0.0), TimePos::at(id, 1.0)));
    let delete = GridEvent::DeleteMeasure { index: 1, policy: FOLLOW };
    assert_eq!(
        delete.apply(&mut p, false).unwrap().unwrap_err(),
        EditError::NeedsConfirmation { notes: 1 }
    );
    assert_eq!(p.measures.len(), 4);
    delete.apply(&mut p, true).unwrap().unwrap();
    assert_eq!((p.measures.len(), p.notes.len()), (3, 0));
}

#[test]
fn non_editing_events_return_none() {
    let mut p = project();
    assert!(GridEvent::MeasureSelected { index: 0, extend: false }.apply(&mut p, false).is_none());
    assert!(GridEvent::AutoFitRequested { index: 0 }.apply(&mut p, false).is_none());
}

#[test]
fn invalid_edits_report_the_core_error() {
    let mut p = project();
    let bad = GridEvent::MeasureMoved { index: 9, new_start_sec: 1.0, policy: FOLLOW };
    assert!(matches!(
        bad.apply(&mut p, false),
        Some(Err(EditError::MeasureOutOfRange { index: 9, .. }))
    ));
}
