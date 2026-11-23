use super::*;
use crate::measures::tests::{close, near, secs};
use crate::model::{GraceKind, GraceNote, Legato, LegatoLink, Note};
use NotePolicy::{FollowGrid as Follow, KeepAudioPosition as Keep};

/// Runs after every edit made by a test: undo restores the snapshot and redo reproduces the project.
pub(super) fn check_undo_redo(project: &Project, edit: &ProjectEdit, before: &Snapshot, label: &str) {
    let mut undone = project.clone();
    edit.undo(&mut undone);
    assert_eq!(Snapshot::capture(&undone), *before, "undo of {label:?}");
    let mut redone = project.clone();
    before.restore(&mut redone);
    edit.redo(&mut redone);
    assert_eq!(redone, *project, "redo of {label:?}");
}

fn project(measures: usize) -> Project {
    crate::testkit::project(measures as f64 * 2.0)
}

fn note(p: &mut Project, midi: u8, start: (u64, f64), end: (u64, f64)) -> u64 {
    let id = crate::tracks::next_note_id(p);
    p.notes.push(Note::new(
        id,
        midi,
        TimePos::at(start.0, start.1),
        TimePos::at(end.0, end.1),
    ));
    id
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
    p.set_tempo_scoped(0, 60.0, Propagation::Following, Follow).unwrap();
    assert_eq!(pos(&p, n), (TimePos::at(2, 1.0), TimePos::at(2, 2.0)));
    assert_eq!(secs(&p, n), (5.0, 6.0));
}

#[test]
fn keep_audio_position_keeps_seconds_and_changes_beats() {
    let mut p = project(4);
    let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
    p.set_tempo_scoped(0, 60.0, Propagation::Following, Keep).unwrap();
    let (s, e) = secs(&p, n);
    close(s, 2.5);
    close(e, 3.0);
    let (start, end) = pos(&p, n);
    assert_eq!(start.measure, 1);
    close(start.beat(), 2.5);
    close(end.beat(), 3.0);
}

#[test]
fn keep_audio_position_leaves_untouched_notes_bit_exact() {
    let mut p = project(4);
    let third = 1.0 / 3.0;
    let n = note(&mut p, 60, (1, third), (1, 2.0 * third));
    p.set_tempo_scoped(3, 90.0, Propagation::Following, Keep).unwrap();
    assert_eq!(
        pos(&p, n),
        (TimePos::at(1, third), TimePos::at(1, 2.0 * third))
    );
}

#[test]
fn note_on_beat_two_stays_on_beat_two_of_the_same_measure() {
    let mut p = project(6);
    let n = note(&mut p, 60, (5, 1.0), (5, 2.0));
    p.set_tempo_scoped(2, 90.0, Propagation::Following, Follow).unwrap();
    p.set_time_sig_scoped(0, TimeSignature::new(4, 4), Propagation::Following, Follow).unwrap();
    assert_eq!(p.note(n).unwrap().start, TimePos::at(5, 1.0));
}

#[test]
fn tempo_change_with_propagation_moves_all_equal_measures() {
    let mut p = project(4);
    let n = note(&mut p, 60, (3, 0.0), (3, 4.0));
    p.set_tempo_scoped(1, 60.0, Propagation::Following, Follow).unwrap();
    assert_eq!(
        p.measures.iter().map(|m| m.tempo_bpm).collect::<Vec<_>>(),
        [120.0, 60.0, 60.0, 60.0]
    );
    let (s, e) = secs(&p, n);
    close(s, 6.0);
    close(e, 10.0);
    assert_eq!(pos(&p, n).1, TimePos::at(4, 0.0));
}

#[test]
fn note_spanning_a_barline_follows_both_ends() {
    let mut p = project(3);
    let n = note(&mut p, 60, (1, 3.0), (2, 1.0));
    assert_eq!(secs(&p, n), (1.5, 2.5));
    p.set_tempo_scoped(0, 60.0, Propagation::Following, Follow).unwrap();
    assert_eq!(secs(&p, n), (3.0, 5.0));
    assert_eq!(pos(&p, n), (TimePos::at(1, 3.0), TimePos::at(2, 1.0)));
}

#[test]
fn note_spanning_a_barline_with_keep_audio() {
    let mut p = project(3);
    let n = note(&mut p, 60, (1, 3.0), (2, 1.0));
    p.set_tempo_scoped(0, 60.0, Propagation::Following, Keep).unwrap();
    let (s, e) = secs(&p, n);
    close(s, 1.5);
    close(e, 2.5);
}

#[test]
fn unnormalised_end_beat_is_normalised_after_a_time_signature_edit() {
    let mut p = project(3);
    let n = note(&mut p, 60, (1, 1.0), (1, 3.5));
    p.set_time_sig_scoped(0, TimeSignature::new(3, 4), Propagation::Following, Follow).unwrap();
    assert_eq!(p.note(n).unwrap().start, TimePos::at(1, 1.0));
    let end = p.note(n).unwrap().end;
    assert_eq!(end.measure, 2);
    close(end.beat(), 0.5);
}

#[test]
fn time_signature_edit_follow_and_keep() {
    let mut p = project(3);
    let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
    p.set_time_sig_scoped(0, TimeSignature::new(3, 4), Propagation::Following, Keep).unwrap();
    let (s, e) = secs(&p, n);
    close(s, 2.5);
    close(e, 3.0);
    let mut p = project(3);
    let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
    p.set_time_sig_scoped(0, TimeSignature::new(3, 4), Propagation::Following, Follow).unwrap();
    let (s, e) = secs(&p, n);
    close(s, 2.0);
    close(e, 2.5);
}

#[test]
fn failed_edit_changes_nothing() {
    let mut p = project(3);
    note(&mut p, 60, (1, 0.0), (1, 1.0));
    let before = p.clone();
    assert!(p.set_tempo_scoped(9, 100.0, Propagation::Following, Follow).is_err());
    assert!(p.set_tempo_scoped(0, 0.0, Propagation::Following, Follow).is_err());
    assert!(p.delete_measure(0, Follow, false).is_err());
    assert_eq!(p, before);
}

#[test]
fn undo_and_redo_restore_the_project() {
    let mut p = project(4);
    note(&mut p, 60, (2, 1.0), (2, 2.0));
    let before = p.clone();
    let edit = p.set_tempo_scoped(0, 60.0, Propagation::Following, Follow).unwrap();
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
    let edit = p
        .set_tempo_scoped(0, 60.0, Propagation::Single, Follow)
        .unwrap();
    let tempos: Vec<f64> = p.measures.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [60.0, 120.0, 120.0, 120.0]);
    close(p.measures[1].start_sec, 4.0);
    close(secs(&p, n).0, 6.0);
    p.set_time_sig_scoped(1, TimeSignature::new(3, 4), Propagation::Single, Follow)
        .unwrap();
    assert_eq!(p.measures[1].time_sig, TimeSignature::new(3, 4));
    assert_eq!(p.measures[2].time_sig, TimeSignature::COMMON);
    p.validate().unwrap();
    assert_eq!(edit.label(), "Set tempo");
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
    near(secs(&p, n).0, 4.0);
    near(secs(&p, n).1, 4.5);
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
        FittedMeasure {
            start_sec: 0.2,
            tempo_bpm: 118.0,
            time_sig: TimeSignature::COMMON,
        },
        FittedMeasure {
            start_sec: 0.2 + 4.0 * 60.0 / 118.0,
            tempo_bpm: 118.0,
            time_sig: TimeSignature::COMMON,
        },
        FittedMeasure {
            start_sec: 0.2 + 8.0 * 60.0 / 118.0,
            tempo_bpm: 118.0,
            time_sig: TimeSignature::COMMON,
        },
    ];
    p.auto_fit(&fitted, Keep).unwrap();
    assert_eq!(p.measures.len(), 3);
    assert_eq!(&ids(&p)[..2], [1, 2]);
    assert_eq!(p.measures[2].id, 3);
    assert!(p.measures.iter().all(|m| m.anchored));
    let (s1, e1) = secs(&p, n);
    near(s0, s1);
    near(e0, e1);
    assert_eq!(p.next_measure_id, 4);
}

#[test]
fn auto_fit_dropping_measures_re_anchors_notes_by_seconds() {
    let mut p = project(4);
    let n = note(&mut p, 60, (4, 1.0), (4, 2.0));
    let (s0, _) = secs(&p, n);
    let fitted = [FittedMeasure {
        start_sec: 0.0,
        tempo_bpm: 60.0,
        time_sig: TimeSignature::COMMON,
    }];
    p.auto_fit(&fitted, Follow).unwrap();
    assert_eq!(p.measures.len(), 1);
    close(secs(&p, n).0, s0);
    p.validate().unwrap();
}

#[test]
fn auto_fit_validation() {
    let mut p = project(2);
    let ok = FittedMeasure {
        start_sec: 0.0,
        tempo_bpm: 120.0,
        time_sig: TimeSignature::COMMON,
    };
    assert!(p.auto_fit(&[], Keep).is_err());
    assert!(p.auto_fit(&[ok, ok], Keep).is_err());
    assert!(p
        .auto_fit(
            &[FittedMeasure {
                tempo_bpm: 1.0,
                ..ok
            }],
            Keep
        )
        .is_err());
    assert!(p
        .auto_fit(
            &[FittedMeasure {
                time_sig: TimeSignature::new(4, 5),
                ..ok
            }],
            Keep
        )
        .is_err());
}

#[test]
fn insert_measure_keeps_ids_stable_and_notes_in_their_measures() {
    let mut p = project(3);
    let n = note(&mut p, 60, (2, 1.0), (2, 2.0));
    p.measures[2].tempo_bpm = 90.0;
    let edit = p.insert_measure(1, Follow).unwrap();
    assert_eq!(ids(&p), [1, 4, 2, 3]);
    assert_eq!(p.next_measure_id, 5);
    assert_eq!(pos(&p, n), (TimePos::at(2, 1.0), TimePos::at(2, 2.0)));
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
    p.notes[1].grace_of = Some(GraceNote {
        main,
        kind: GraceKind::Acciaccatura,
    });
    let slide_start = note(&mut p, 50, (1, 0.0), (1, 1.0));
    p.notes[2].technique = Some(Technique::Slide);
    p.notes[2].link_to = Some(main);
    let hammer = note(&mut p, 62, (3, 0.0), (3, 1.0));
    p.notes[3].legato = Some(LegatoLink {
        kind: Legato::HammerOn,
        from: main,
    });
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
    p.notes[1].grace_of = Some(GraceNote {
        main,
        kind: GraceKind::Appoggiatura,
    });
    p.delete_measure(2, Follow, false).unwrap();
    assert!(p.note(grace).is_some());
}

#[test]
fn delete_measure_cuts_notes_that_only_end_inside_it() {
    let mut p = project(3);
    let n = note(&mut p, 60, (1, 3.0), (2, 2.0));
    p.delete_measure(1, Follow, false).unwrap();
    assert_eq!(pos(&p, n).1, TimePos::at(3, 0.0));
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
    assert_eq!(
        p.delete_measure(0, Follow, true).unwrap_err(),
        EditError::LastMeasure
    );
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
    assert_eq!(p.note(a).unwrap().start, TimePos::at(1, 1.0));
    assert_eq!(p.note(b).unwrap().start.measure, 3);
    close(p.note(b).unwrap().start.beat(), 0.5);
    p.validate().unwrap();
}

#[test]
fn split_six_eight_and_validation() {
    let mut p = project(1);
    p.set_time_sig_scoped(0, TimeSignature::new(6, 8), Propagation::Following, Follow).unwrap();
    assert_eq!(
        p.split_measure(0, 1.5)
            .map(|e| e.label().to_owned())
            .unwrap(),
        "Split measure"
    );
    assert_eq!(p.measures[0].time_sig, TimeSignature::new(3, 8));
    assert_eq!(p.measures[1].time_sig, TimeSignature::new(3, 8));
    let mut p = project(1);
    assert_eq!(
        p.split_measure(0, 1.5).unwrap_err(),
        EditError::InvalidSplit
    );
    assert_eq!(
        p.split_measure(0, 0.0).unwrap_err(),
        EditError::InvalidSplit
    );
    assert_eq!(
        p.split_measure(0, 4.0).unwrap_err(),
        EditError::InvalidSplit
    );
    assert_eq!(
        p.split_measure(0, f64::NAN).unwrap_err(),
        EditError::InvalidSplit
    );
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
    assert_eq!(p.note(b).unwrap().start, TimePos::at(1, 4.0));
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
    assert_eq!(
        p.merge_measures(0).unwrap_err(),
        EditError::IncompatibleTimeSignatures
    );
    assert!(project(2).merge_measures(1).is_err());
    let mut p = project(2);
    p.measures[0].time_sig = TimeSignature::new(40, 4);
    p.measures[1].time_sig = TimeSignature::new(40, 4);
    assert_eq!(
        p.merge_measures(0).unwrap_err(),
        EditError::IncompatibleTimeSignatures
    );
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
    close(p.note(n).unwrap().start.beat(), 2.5);
}

#[test]
fn remove_notes_cascades_and_is_undoable() {
    let mut p = project(2);
    let main = note(&mut p, 60, (1, 1.0), (1, 2.0));
    let grace = note(&mut p, 59, (1, 0.9), (1, 1.0));
    p.notes[1].grace_of = Some(GraceNote {
        main,
        kind: GraceKind::Acciaccatura,
    });
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
    p.set_tempo_scoped(1, 97.5, Propagation::Following, Follow).unwrap();
    p.insert_measure(2, Follow).unwrap();
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
}
