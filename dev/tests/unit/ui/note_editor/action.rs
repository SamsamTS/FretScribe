use super::super::testkit::*;
use super::*;
use fretscribe_core::GraceKind;
use fretscribe_core::Note;

fn sel(ids: &[u64]) -> Selection {
    let mut s = Selection::default();
    s.set(ids.to_vec());
    s
}

fn run(p: &mut Project, ids: &[u64], action: NoteAction) -> Result<Applied, ActionError> {
    apply(p, &sel(ids), action, &SnapGrid::new(&[4, 3]))
}

fn note(p: &Project, id: u64) -> &Note {
    p.note(id).unwrap()
}

#[test]
fn delete_with_and_without_bridge() {
    let mut p = project();
    let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    p.link_notes(b, c, LinkType::Slide).unwrap();
    let before = p.notes.clone();
    let applied = run(&mut p, &[b], NoteAction::Delete { bridge: true }).unwrap();
    assert_eq!(p.chain_of(a), [a, c]);
    assert_eq!(applied.selection, Some(Selection::default()));
    applied.edit.unwrap().undo(&mut p);
    assert_eq!(p.notes, before);
    run(&mut p, &[b], NoteAction::Delete { bridge: false }).unwrap();
    assert_eq!(p.chain_of(a), [a]);
}

#[test]
fn empty_selection_is_refused_for_edits_and_allowed_for_selection_commands() {
    let mut p = project();
    let a = add(&mut p, 60, 0.0, 1.0);
    assert_eq!(run(&mut p, &[], NoteAction::Transpose(1)).unwrap_err(), ActionError::EmptySelection);
    assert_eq!(run(&mut p, &[], NoteAction::SelectAll).unwrap().selection.unwrap().ids, [a]);
    assert!(run(&mut p, &[], NoteAction::LinkHover).unwrap().link_hover);
    let applied = run(&mut p, &[a], NoteAction::LinkHover).unwrap();
    assert!(!applied.link_hover);
    assert_eq!(applied.message.as_deref(), Some("0 linked"));
}

#[test]
fn transpose_moves_by_semitones_and_refuses_to_leave_the_range() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 127, 1.0, 2.0));
    run(&mut p, &[a], NoteAction::Transpose(-12)).unwrap();
    assert_eq!(note(&p, a).midi, 48);
    assert_eq!(run(&mut p, &[a, b], NoteAction::Transpose(1)).unwrap_err(), ActionError::OutOfRange);
    assert_eq!(note(&p, a).midi, 48);
}

#[test]
fn nudge_follows_the_grid_for_the_whole_selection() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 1.25), add(&mut p, 62, 2.0, 3.0));
    run(&mut p, &[a, b], NoteAction::Nudge { steps: 1 }).unwrap();
    assert_eq!((note(&p, a).start, note(&p, b).start), (at(1.25), at(2.25)));
    run(&mut p, &[a, b], NoteAction::Nudge { steps: 2 }).unwrap();
    assert_eq!(note(&p, a).start, at(1.5));
    run(&mut p, &[a, b], NoteAction::Nudge { steps: -1 }).unwrap();
    assert_eq!(note(&p, a).start, at(1.0 + 1.0 / 3.0));
}

#[test]
fn resize_steps_the_end_and_keeps_the_minimum() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 1.25);
    run(&mut p, &[a], NoteAction::Resize { steps: 1 }).unwrap();
    assert_eq!(note(&p, a).end, at(1.0 + 1.0 / 3.0));
    let applied = run(&mut p, &[a], NoteAction::Resize { steps: -5 }).unwrap();
    assert!(applied.edit.is_none());
    assert_eq!(note(&p, a).end, at(1.0 + 1.0 / 3.0));
}

#[test]
fn duplicate_pastes_after_the_selection_and_selects_the_copy() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    let applied = run(&mut p, &[a, b], NoteAction::Duplicate).unwrap();
    let ids = applied.selection.unwrap().ids;
    assert_eq!(ids.len(), 2);
    assert_eq!(note(&p, ids[0]).start, at(3.0));
    assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
}

#[test]
fn link_unlink_and_cycle_report_and_undo_in_one_step() {
    let mut p = project();
    let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0), add(&mut p, 64, 3.0, 4.0)];
    let before = p.notes.clone();
    let applied = run(&mut p, &ids, NoteAction::LinkSelection).unwrap();
    assert_eq!(applied.message.as_deref(), Some("3 linked"));
    applied.edit.unwrap().undo(&mut p);
    assert_eq!(p.notes, before);
    let none = run(&mut p, &[ids[0]], NoteAction::LinkSelection).unwrap();
    assert!(none.edit.is_none());
    assert_eq!(none.message.as_deref(), Some("0 linked"));

    p.link_notes(ids[0], ids[1], LinkType::Slide).unwrap();
    let cycled = run(&mut p, &[ids[0]], NoteAction::CycleLinkType).unwrap();
    assert_eq!(cycled.message.as_deref(), Some("Bend"));
    assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Bend)));
    assert!(run(&mut p, &[ids[3]], NoteAction::CycleLinkType).unwrap().edit.is_none());
    run(&mut p, &[ids[0], ids[3]], NoteAction::UnlinkSelection).unwrap();
    assert_eq!(p.outgoing(ids[0]), None);
    assert!(run(&mut p, &[ids[0]], NoteAction::UnlinkSelection).unwrap().edit.is_none());
}

#[test]
fn locking_a_string_sets_the_fret_and_unlocking_clears_it() {
    let mut p = project();
    let (a, low) = (add(&mut p, 64, 0.0, 1.0), add(&mut p, 40, 1.0, 2.0));
    run(&mut p, &[a], NoteAction::SetString(Some(4))).unwrap();
    assert_eq!((note(&p, a).string_fret, note(&p, a).locked_fret), (Some((4, 5)), true));
    assert_eq!(run(&mut p, &[a, low], NoteAction::SetString(Some(5))).unwrap_err(), ActionError::NoPlacement);
    run(&mut p, &[a], NoteAction::SetString(None)).unwrap();
    assert_eq!((note(&p, a).string_fret, note(&p, a).locked_fret), (None, false));
}

#[test]
fn stepping_selection_exact_edit_and_unavailable_actions() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0));
    let next = run(&mut p, &[a], NoteAction::SelectNext { extend: true }).unwrap().selection.unwrap();
    assert_eq!(next.ids, [a, b]);
    assert_eq!(run(&mut p, &[b], NoteAction::OpenExactEdit).unwrap().open_exact, Some(b));
    assert_eq!(run(&mut p, &[a], NoteAction::Grace).unwrap().grace_entry, Some(a));
}

#[test]
fn force_legato_links_one_note_to_the_previous_and_reports_skips() {
    let mut p = project();
    let (a, b, c) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0), add(&mut p, 62, 1.0, 1.5));
    let one = run(&mut p, &[b], NoteAction::ForceLegato).unwrap();
    assert_eq!((one.message.as_deref(), one.affected.clone()), (Some("1 linked"), vec![b]));
    assert_eq!(p.outgoing(a), Some((b, LinkType::Legato)));
    one.edit.unwrap().undo(&mut p);
    assert_eq!(p.outgoing(a), None);
    let none = run(&mut p, &[a], NoteAction::ForceLegato).unwrap();
    assert!(none.edit.is_none());
    assert_eq!(none.message.as_deref(), Some("0 linked, 1 skipped: there is no earlier note to link from"));
    let same = run(&mut p, &[c], NoteAction::ForceLegato).unwrap();
    assert_eq!(same.message.as_deref(), Some("0 linked, 1 skipped: both notes have the same pitch"));
    let all = run(&mut p, &[a, b], NoteAction::ForceLegato).unwrap();
    assert_eq!(all.message.as_deref(), Some("1 linked"));
}

#[test]
fn grace_on_a_grace_note_starts_an_entry_on_its_main_and_h_toggles_the_legato() {
    let mut p = project();
    let main = add(&mut p, 62, 1.0, 2.0);
    let grace = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    let applied = run(&mut p, &[grace], NoteAction::Grace).unwrap();
    assert_eq!((applied.grace_entry, applied.edit.is_some(), note(&p, grace).grace_of.unwrap().kind), (Some(main), false, GraceKind::Acciaccatura));
    assert_eq!(p.outgoing(grace), Some((main, LinkType::Legato)));
    let off = run(&mut p, &[main], NoteAction::ForceLegato).unwrap();
    assert_eq!(p.outgoing(grace), None, "H/P on a legato removes it");
    off.edit.unwrap().undo(&mut p);
    assert_eq!(p.outgoing(grace), Some((main, LinkType::Legato)));
    let moved = run(&mut p, &[grace], NoteAction::Transpose(1)).unwrap();
    assert_eq!(note(&p, grace).midi, 61);
    assert_eq!(moved.affected, [grace]);
    run(&mut p, &[grace], NoteAction::Delete { bridge: false }).unwrap();
    assert!(p.note(grace).is_none() && p.note(main).is_some());
}

#[test]
fn beat_nudge_moves_whole_beats_and_snap_presets_wrap() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 2.0);
    let applied = run(&mut p, &[a], NoteAction::NudgeBeat(2)).unwrap();
    assert!(applied.edit.is_some());
    assert_eq!(p.note(a).unwrap().start, at(3.0));
    run(&mut p, &[a], NoteAction::NudgeBeat(-1)).unwrap();
    assert_eq!(p.note(a).unwrap().start, at(2.0));

    let grid = |d: &[u8]| SnapGrid::new(d);
    let step = |g: &SnapGrid, next| apply(&mut project(), &sel(&[]), NoteAction::SnapPreset { next }, g).unwrap().snap_grid.unwrap();
    assert_eq!(step(&grid(&[4, 3]), true), grid(&[4, 6]));
    assert_eq!(step(&grid(&[4, 3]), false), grid(&[2, 3]));
    assert_eq!(step(&grid(&[4, 6]), true), grid(&[1]));
    assert_eq!(step(&grid(&[1]), false), grid(&[4, 6]));
    assert_eq!(step(&grid(&[4, 8]), true), grid(&[1]));
}

#[test]
fn string_keys_count_from_the_highest_string() {
    let mut p = project();
    let a = add(&mut p, 64, 0.0, 1.0);
    let string = |p: &Project| p.note(a).unwrap().string_fret.map(|s| s.0);
    run(&mut p, &[a], NoteAction::SetStringFromTop(1)).unwrap();
    assert_eq!(string(&p), Some(5));
    run(&mut p, &[a], NoteAction::SetStringFromTop(2)).unwrap();
    assert_eq!(string(&p), Some(4));
    for n in [0, 7] {
        assert_eq!(run(&mut p, &[a], NoteAction::SetStringFromTop(n)).unwrap_err(), ActionError::NoPlacement);
    }
    p.tuning.open_strings = vec![28, 33, 38, 43];
    assert_eq!(run(&mut p, &[a], NoteAction::SetStringFromTop(5)).unwrap_err(), ActionError::NoPlacement);
}
