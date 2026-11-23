use super::super::testkit::*;
use super::*;
use fretscribe_core::{GraceKind, GraceNote, LinkType, TempoMap};

fn pt(beat: f64, midi: f32) -> Pt {
    Pt { sec: sec(beat), midi }
}

fn mv(p: &Project, grab: u64, sel: &[u64], pointer: Pt, offset: f64, shift: bool, alt: bool) -> Vec<NoteChange> {
    with_ctx(p, &[4, 3], alt, |ctx| plan_move(grab, sel, pointer, offset, shift, (None, None), ctx)).unwrap().0
}

#[test]
fn moving_a_selection_keeps_lengths_links_and_relative_timing() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.5));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    let outsider = add(&mut p, 50, 0.0, 0.5);
    let changes = mv(&p, a, &[a, b], pt(2.51, 62.0), 0.0, false, false);
    assert_eq!(changes.len(), 2);
    let by = |id| changes.iter().find(|c| c.id == id).unwrap();
    assert_eq!((by(a).start, by(a).end, by(a).midi), (at(2.5), at(3.5), 62));
    assert_eq!((by(b).start, by(b).end, by(b).midi), (at(3.5), TimePos::at(2, 1.0), 66));
    assert!(changes.iter().all(|c| c.id != outsider));
    let edit = apply_changes(&mut p, &changes, "Move").unwrap();
    assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
    edit.undo(&mut p);
    assert_eq!(p.note(a).unwrap().start, at(1.0));
}

#[test]
fn a_non_selected_note_moves_alone() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0));
    let changes = mv(&p, b, &[a], pt(3.0, 64.0), 0.0, false, false);
    assert_eq!(changes.iter().map(|c| c.id).collect::<Vec<_>>(), [b]);
    assert_eq!(changes[0].start, at(3.0));
}

#[test]
fn shift_keeps_the_pitch_and_the_grab_offset_keeps_the_note_under_the_pointer() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 2.0);
    let c = &mv(&p, a, &[], pt(3.4, 65.0), sec(0.4), true, false)[0];
    assert_eq!((c.midi, c.start), (60, at(3.0)));
}

#[test]
fn alt_moves_freely_and_the_start_snaps_to_a_neighbour_end() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 2.0);
    let _ = add(&mut p, 50, 0.0, 0.6);
    let free = &mv(&p, a, &[], pt(0.7, 60.0), 0.0, false, true)[0];
    assert!((free.start.beat() - 0.7).abs() < 0.02);
    let snapped = &mv(&p, a, &[], pt(0.62, 60.0), 0.0, false, false)[0];
    near(snapped.start, 0.6);
}

#[test]
fn pitch_is_clamped_to_the_midi_range_for_the_whole_selection() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 126, 1.0, 2.0));
    let changes = mv(&p, a, &[a, b], pt(1.0, 90.0), 0.0, false, false);
    assert_eq!(changes.iter().find(|c| c.id == b).unwrap().midi, 127);
    assert_eq!(changes.iter().find(|c| c.id == a).unwrap().midi, 61);
}

#[test]
fn grace_notes_follow_their_main_note() {
    let mut p = project();
    let main = add(&mut p, 60, 1.0, 2.0);
    let grace = add(&mut p, 62, 0.9, 1.0);
    p.notes.iter_mut().find(|n| n.id == grace).unwrap().grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
    let changes = mv(&p, main, &[], pt(2.0, 60.0), 0.0, false, false);
    assert_eq!(changes.len(), 2);
    near(changes.iter().find(|c| c.id == grace).unwrap().start, 1.9);
}

fn plan(p: &Project, n: u64, edge: Edge, beat: f64) -> NoteChange {
    with_ctx(p, &[4, 3], false, |ctx| plan_resize(n, edge, sec(beat), (None, None), ctx)).unwrap().0
}

fn rs(p: &Project, n: u64, edge: Edge, beat: f64) -> NoteChange {
    plan(p, n, edge, beat)
}

#[test]
fn resizing_a_link_end_drags_the_note_it_is_tied_to_and_stops_before_squashing_it() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 3.0, 4.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    assert_eq!(p.note(a).unwrap().end, at(3.0));
    near(rs(&p, a, Edge::End, 2.5).end, 2.5);
    assert!(beats_of(&p, rs(&p, a, Edge::End, 5.0).end) < 4.0, "b keeps a lattice unit");
    near(rs(&p, b, Edge::Start, 0.2).start, 1.0 + 1.0 / 24.0);
    let change = rs(&p, a, Edge::End, 2.5);
    let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
    assert_eq!((p.note(a).unwrap().end, p.note(b).unwrap().start), (at(2.5), at(2.5)));
    assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
    edit.undo(&mut p);
    assert_eq!((p.note(a).unwrap().end, p.note(b).unwrap().start), (at(3.0), at(3.0)));
}

#[test]
fn the_locked_edges_of_grace_notes_do_not_move_and_a_grace_note_only_sits_on_or_before_the_beat() {
    let mut p = project();
    let main = add(&mut p, 60, 2.0, 3.0);
    let before = p.add_grace_note(main, Some(62), GraceKind::Acciaccatura).unwrap();
    let on = p.add_grace_note(main, Some(64), GraceKind::Appoggiatura).unwrap();
    let none = |p: &Project, n, edge| with_ctx(p, &[4, 3], false, |ctx| plan_resize(n, edge, sec(2.5), (None, None), ctx));
    assert!(none(&p, before, Edge::End).is_none() && none(&p, on, Edge::Start).is_none());
    assert!(none(&p, before, Edge::Start).is_some() && none(&p, on, Edge::End).is_some());
    let kind = |p: &Project, id| p.note(id).unwrap().grace_of.unwrap().kind;
    let moved = mv(&p, before, &[], pt(2.4, 66.0), 0.0, false, false);
    assert_eq!((moved.len(), moved[0].midi), (1, 66));
    apply_changes(&mut p, &moved, "Move").unwrap();
    assert_eq!((kind(&p, before), p.note(before).unwrap().start), (GraceKind::Appoggiatura, at(2.0)));
    let moved = mv(&p, before, &[], pt(1.5, 66.0), 0.0, false, false);
    apply_changes(&mut p, &moved, "Move").unwrap();
    assert_eq!(kind(&p, before), GraceKind::Acciaccatura);
    assert_eq!(p.note(before).unwrap().end, at(2.0));
}

#[test]
fn resizing_the_start_of_a_note_moves_its_grace_notes_with_it() {
    let mut p = project();
    let main = add(&mut p, 60, 2.0, 4.0);
    let before = p.add_grace_note(main, Some(62), GraceKind::Acciaccatura).unwrap();
    let on = p.add_grace_note(main, Some(64), GraceKind::Appoggiatura).unwrap();
    let old = [before, on].map(|id| (p.note(id).unwrap().start, p.note(id).unwrap().end));
    let change = rs(&p, main, Edge::Start, 3.0);
    let preview = preview_changes(&p, std::slice::from_ref(&change));
    assert_eq!(preview.len(), 3, "the grace notes are part of the preview");
    let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
    for (id, (s, e)) in [before, on].into_iter().zip(old) {
        let n = p.note(id).unwrap();
        assert!((beats_of(&p, n.start) - beats_of(&p, s) - 1.0).abs() < 1e-9 && (beats_of(&p, n.end) - beats_of(&p, e) - 1.0).abs() < 1e-9);
    }
    assert_eq!(p.note(on).unwrap().grace_of.unwrap().kind, GraceKind::Appoggiatura);
    edit.undo(&mut p);
    assert_eq!(p.note(before).unwrap().start, old[0].0);
}

fn beats_of(p: &Project, pos: TimePos) -> f64 {
    TempoMap::new(&p.measures).pos_to_beats(pos).unwrap()
}

#[test]
fn landing_on_a_same_pitch_note_cuts_it_and_other_pitches_are_left_alone() {
    let mut p = project();
    let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 60, 2.0, 3.0), add(&mut p, 64, 2.0, 3.0));
    let before = p.notes.clone();
    let change = plan(&p, a, Edge::End, 2.5);
    let preview = preview_changes(&p, std::slice::from_ref(&change));
    assert_eq!(preview.iter().map(|c| (c.id, c.start, c.end)).collect::<Vec<_>>(), [(a, at(0.0), at(2.5)), (b, at(2.5), at(3.0))]);
    let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
    assert_eq!((p.note(b).unwrap().start, p.note(c).unwrap().start), (at(2.5), at(2.0)));
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
    let change = plan(&p, a, Edge::End, 3.5);
    let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
    assert!(p.note(b).is_none() && p.note(c).is_some());
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
}

#[test]
fn moving_a_note_into_the_middle_of_another_splits_it() {
    let mut p = project();
    let (long, short) = (add(&mut p, 60, 0.0, 4.0), add(&mut p, 60, 6.0, 7.0));
    let moved = mv(&p, short, &[], pt(1.0, 60.0), 0.0, false, false);
    assert_eq!(preview_changes(&p, &moved).len(), 3, "the moved note, the cut head and the new tail");
    apply_changes(&mut p, &moved, "Move").unwrap();
    let mut spans: Vec<_> = p.notes.iter().map(|n| (n.start, n.end)).collect();
    spans.sort_by(|a, b| a.0.beat().total_cmp(&b.0.beat()));
    assert_eq!(spans, [(at(0.0), at(1.0)), (at(1.0), at(2.0)), (at(2.0), at(4.0))]);
    assert!(p.note(long).is_some());
}

#[test]
fn resizing_an_edge_snaps_keeps_links_and_moves_no_other_note() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 3.0, 4.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    let c = rs(&p, a, Edge::End, 2.76);
    assert_eq!((c.start, c.end), (at(1.0), at(2.75)));
    apply_changes(&mut p, &[c], "Resize").unwrap();
    assert_eq!(p.note(b).unwrap().start, at(2.75), "the tied start follows");
    assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
    let c = rs(&p, a, Edge::Start, 0.51);
    assert_eq!((c.start, c.end), (at(0.5), at(2.75)));
}

#[test]
fn resizing_never_shrinks_below_the_minimal_subdivision() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 2.0);
    let c = rs(&p, a, Edge::End, 0.2);
    assert_eq!(c.end, at(1.25));
    let c = rs(&p, a, Edge::Start, 3.0);
    assert_eq!(c.start, at(1.75));
}

#[test]
fn noop_detection() {
    let mut p = project();
    let a = add(&mut p, 60, 1.0, 2.0);
    let same = NoteChange { id: a, midi: 60, start: at(1.0), end: at(2.0) };
    assert!(is_noop(&p, std::slice::from_ref(&same)));
    assert!(!is_noop(&p, &[NoteChange { midi: 61, ..same }]));
}
