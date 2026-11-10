use super::*;
use crate::links::LinkType;
use crate::model::{TimeSignature, Track, TrackRole};

fn project() -> Project {
    Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 8.0, 120.0, TimeSignature::COMMON)
}

fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
    let tempo = TempoMap::new(&p.measures);
    let id = next_note_id(p);
    p.notes.push(Note::new(id, midi, tempo.sec_to_pos(start), tempo.sec_to_pos(end)));
    id
}

fn secs(p: &Project, id: u64) -> (f64, f64) {
    let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
    (times.start_sec(id).unwrap(), times.end_sec(id).unwrap())
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn on_string(p: &mut Project, id: u64, string: u8, fret: u8) {
    let n = p.notes.iter_mut().find(|n| n.id == id).unwrap();
    n.string_fret = Some((string, fret));
}

#[test]
fn grace_timing_pitch_and_ids() {
    let mut p = project();
    let main = add(&mut p, 60, 2.0, 3.0);
    let g = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    assert_ne!(g, main);
    let note = p.note(g).unwrap();
    assert_eq!((note.midi, note.grace_of), (58, Some(GraceNote { main, kind: GraceKind::Acciaccatura })));
    let (s, e) = secs(&p, g);
    close(e, 2.0);
    close(s, 2.0 - ACCIACCATURA_SEC);
    let other = add(&mut p, 62, 5.0, 6.0);
    let a = p.add_grace_note(other, Some(64), GraceKind::Appoggiatura).unwrap();
    close(secs(&p, a).1 - secs(&p, a).0, APPOGGIATURA_SEC);
    assert_eq!(p.add_grace_note(main, Some(200), GraceKind::Acciaccatura), Err(GraceError::Pitch));
    assert_eq!(p.add_grace_note(g, None, GraceKind::Acciaccatura), Err(GraceError::MainIsGrace));
    assert_eq!(p.add_grace_note(999, None, GraceKind::Acciaccatura), Err(GraceError::UnknownNote(999)));
}

#[test]
fn length_is_limited_by_half_the_gap_to_the_previous_note() {
    let mut p = project();
    add(&mut p, 55, 1.95, 2.0);
    let main = add(&mut p, 60, 2.0, 3.0);
    let g = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    let (s, e) = secs(&p, g);
    close(e - s, 0.025);
    assert!(s > 1.95);
    let on_beat = p.add_grace_note(main, None, GraceKind::Appoggiatura).unwrap();
    close(secs(&p, on_beat).1 - secs(&p, on_beat).0, APPOGGIATURA_SEC);
    let mut p = project();
    add(&mut p, 55, 1.9985, 2.0);
    let main = add(&mut p, 60, 2.0, 3.0);
    assert_eq!(p.add_grace_note(main, None, GraceKind::Acciaccatura), Err(GraceError::NoRoom));
}

#[test]
fn several_graces_chain_backwards_without_overlap() {
    let mut p = project();
    let main = add(&mut p, 60, 2.0, 3.0);
    let first = p.add_grace_note(main, Some(57), GraceKind::Acciaccatura).unwrap();
    let second = p.add_grace_note(main, Some(55), GraceKind::Acciaccatura).unwrap();
    close(secs(&p, second).1, secs(&p, first).0);
    assert!(secs(&p, second).0 < secs(&p, first).0 && secs(&p, first).1 <= 2.0 + 1e-9);
}

#[test]
fn kinds_are_the_two_placements_around_the_beat() {
    let mut p = project();
    add(&mut p, 50, 1.0, 1.5);
    let main = add(&mut p, 60, 2.0, 3.0);
    let g = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    close(secs(&p, g).1, 2.0);
    p.set_grace_kind(g, GraceKind::Appoggiatura).unwrap();
    assert_eq!(p.note(g).unwrap().grace_of.unwrap().kind, GraceKind::Appoggiatura);
    close(secs(&p, g).0, 2.0);
    close(secs(&p, g).1 - secs(&p, g).0, ACCIACCATURA_SEC);
    assert_eq!(p.set_grace_kind(main, GraceKind::Acciaccatura), Err(GraceError::NotGrace(main)));
    p.set_grace_timing(g, GraceKind::Appoggiatura, 0.2).unwrap();
    close(secs(&p, g).1, 2.2);
    p.set_grace_timing(g, GraceKind::Appoggiatura, 50.0).unwrap();
    close(secs(&p, g).1, 3.0);
    p.set_grace_timing(g, GraceKind::Acciaccatura, 0.3).unwrap();
    close(secs(&p, g).0, 1.7);
    close(secs(&p, g).1, 2.0);
    p.set_grace_timing(g, GraceKind::Acciaccatura, 50.0).unwrap();
    assert!(secs(&p, g).0 > 1.0, "never reaches the start of the previous note");
    assert_eq!(p.grace_link(g), LinkType::Legato, "on the beat or before it, the same links are valid");
    p.set_grace_timing(g, GraceKind::Acciaccatura, 0.04).unwrap();

    let h = add(&mut p, 59, 3.0, 3.5);
    p.link_legato(main, h, None).unwrap();
    p.remove_grace(g).unwrap();
    assert!(p.note(g).is_none() && p.note(main).unwrap().legato.is_none());
    assert_eq!(p.remove_grace(main), Err(GraceError::NotGrace(main)));
    assert_eq!(p.set_grace_timing(main, GraceKind::Acciaccatura, 0.1), Err(GraceError::NotGrace(main)));
}

#[test]
fn main_note_removal_takes_its_graces_and_clears_legato() {
    let mut p = project();
    let main = add(&mut p, 60, 2.0, 3.0);
    let g = p.add_grace_note(main, Some(57), GraceKind::Acciaccatura).unwrap();
    assert_eq!(p.grace_link(g), LinkType::Legato);
    let next = add(&mut p, 64, 3.0, 3.5);
    p.link_legato(main, next, None).unwrap();
    p.delete_notes(&[main], false);
    assert!(p.note(g).is_none());
    assert_eq!(p.note(next).unwrap().legato, None);
}

#[test]
fn legato_kind_is_inferred_and_checked() {
    let mut p = project();
    let (a, b, c) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0), add(&mut p, 60, 1.0, 1.5));
    assert_eq!(p.link_legato(a, b, Some(Legato::PullOff)), Err(LegatoError::KindMismatch(Legato::PullOff)));
    assert_eq!(p.link_legato(a, b, None), Ok(Legato::HammerOn));
    assert_eq!(p.link_legato(b, c, Some(Legato::PullOff)), Ok(Legato::PullOff));
    assert_eq!(p.note(c).unwrap().legato, Some(crate::model::LegatoLink { kind: Legato::PullOff, from: b }));
    assert!(p.unlink_legato(c) && !p.unlink_legato(c));
}

#[test]
fn rejected_cases_and_the_one_link_rule_across_types() {
    let mut p = project();
    let (a, b, c, d) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0), add(&mut p, 62, 1.0, 1.5), add(&mut p, 64, 1.5, 2.0));
    let err = |r: Result<Legato, LegatoError>| r.unwrap_err();
    assert_eq!(err(p.link_legato(b, c, None)), LinkError::SamePitch.into());
    assert_eq!(err(p.link_legato(b, a, None)), LinkError::NotInOrder.into());
    assert_eq!(err(p.link_legato(a, a, None)), LinkError::SameNote.into());
    assert_eq!(err(p.link_legato(a, d, None)), LinkError::NotAdjacent.into());
    p.link_notes(a, b, LinkType::Slide).unwrap();
    assert_eq!(err(p.link_legato(a, d, None)), LinkError::HasOutgoing(a).into());
    assert_eq!(err(p.link_legato(c, b, None)), LinkError::NotInOrder.into());
    p.link_legato(b, d, None).unwrap_err();
    p.unlink(a);
    p.link_legato(a, b, None).unwrap();
    let e = add(&mut p, 50, 0.2, 0.4);
    assert_eq!(p.link_notes(e, b, LinkType::Slide), Err(LinkError::HasIncoming(b)));
    let before = p.notes.clone();
    assert_eq!(err(p.link_legato(b, a, None)), LinkError::NotInOrder.into());
    assert_eq!(p.notes, before);
}

#[test]
fn cycle_switches_representation_in_one_edit() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0));
    p.link_notes(a, b, LinkType::Bend).unwrap();
    let before = p.notes.clone();
    let (edit, kind) = p.try_edit("Type", |p| p.cycle_link_type(a).ok_or(())).unwrap();
    assert_eq!(kind, LinkType::Legato);
    assert!(p.note(a).unwrap().link_to.is_none() && p.note(b).unwrap().legato.is_some());
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
}

#[test]
fn different_strings_ask_for_a_placement_and_force_moves_and_locks() {
    let mut p = project();
    let (a, b) = (add(&mut p, 55, 0.0, 0.5), add(&mut p, 57, 0.5, 1.0));
    on_string(&mut p, a, 3, 0);
    on_string(&mut p, b, 2, 7);
    assert_eq!(p.link_legato(a, b, None), Err(LegatoError::NeedsSameString { candidates: vec![(3, 2)] }));
    assert_eq!(p.infer_link_type(a, b), LinkType::Slide);
    assert_eq!(p.force_legato(a, b), Ok(Legato::HammerOn));
    let n = p.note(b).unwrap();
    assert_eq!((n.string_fret, n.locked_fret), (Some((3, 2)), true));

    let mut p = project();
    let (a, b) = (add(&mut p, 64, 0.0, 0.5), add(&mut p, 40, 0.5, 1.0));
    on_string(&mut p, a, 5, 0);
    on_string(&mut p, b, 0, 0);
    assert_eq!(p.force_legato(a, b), Err(LinkError::NoSameString.into()));
    assert!(!p.note(b).unwrap().locked_fret);
}

#[test]
fn force_legato_selection_single_note_and_groups() {
    let mut p = project();
    let ids = [add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0), add(&mut p, 59, 1.0, 1.5)];
    let report = p.force_legato_selection(&[ids[1]]);
    assert_eq!((report.linked, p.note(ids[1]).unwrap().legato.map(|l| l.from)), (1, Some(ids[0])));
    assert_eq!(p.force_legato_selection(&ids[1..]).linked, 1);
    let report = p.force_legato_selection(&ids);
    assert_eq!((report.linked, report.skipped.len()), (0, 2));
    assert_eq!(p.force_legato_selection(&[ids[0]]).skipped.len(), 1);
}

#[test]
fn grace_notes_are_always_linked_legato_first_dashed_when_nothing_else_is_valid() {
    let mut p = project();
    add(&mut p, 40, 0.0, 0.5);
    let main = add(&mut p, 60, 3.0, 4.0);
    let g = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    assert_eq!((p.grace_link(g), p.outgoing(g)), (LinkType::Legato, Some((main, LinkType::Legato))));
    assert_eq!(p.note(main).unwrap().legato.map(|l| l.from), Some(g));
    // On the beat, however long: the same kinds are valid and the link is kept.
    p.set_grace_timing(g, GraceKind::Appoggiatura, 0.3).unwrap();
    assert_eq!(p.grace_link(g), LinkType::Legato);
    assert_eq!(p.valid_links(g), [LinkType::Slide, LinkType::Bend, LinkType::Legato, LinkType::Dashed]);
    let quick = add(&mut p, 66, 10.0, 11.0);
    let q = p.add_grace_note(quick, Some(64), GraceKind::Appoggiatura).unwrap();
    assert_eq!(p.grace_link(q), LinkType::Legato);
    // Same pitch: neither is possible.
    let same = add(&mut p, 62, 6.0, 7.0);
    let g2 = p.add_grace_note(same, Some(62), GraceKind::Appoggiatura).unwrap();
    assert_eq!((p.grace_link(g2), p.valid_links(g2)), (LinkType::Dashed, vec![LinkType::Dashed]));
    // The main note already ends a link: only dashed.
    let (a, b) = (add(&mut p, 50, 8.0, 8.5), add(&mut p, 52, 8.5, 9.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    let g3 = p.add_grace_note(b, Some(55), GraceKind::Acciaccatura).unwrap();
    assert_eq!(p.valid_links(g3), vec![LinkType::Dashed]);
    // Dashed is for grace notes only.
    assert_eq!(p.can_link(a, b, LinkType::Dashed), Err(LinkError::DashedNeedsGrace));
}

#[test]
fn cycling_walks_the_valid_kinds_in_one_edit_for_grace_notes_too() {
    let mut p = project();
    let main = add(&mut p, 60, 3.0, 4.0);
    let g = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
    let before = p.notes.clone();
    let (edit, first) = p.try_edit("Grace link", |p| p.cycle_link_type(g).ok_or(())).unwrap();
    assert_eq!((first, p.grace_link(g)), (LinkType::Dashed, LinkType::Dashed));
    assert!(p.note(main).unwrap().legato.is_none() && p.outgoing(g).is_none());
    assert_eq!(p.cycle_link_type(g), Some(LinkType::Slide));
    assert_eq!(p.cycle_link_type(g), Some(LinkType::Bend));
    assert_eq!(p.note(g).unwrap().technique, Some(crate::model::Technique::Bend));
    assert_eq!(p.cycle_link_type(g), Some(LinkType::Legato));
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
    assert_eq!(p.cycle_link_type(main), None);
    let same = add(&mut p, 62, 6.0, 7.0);
    let g2 = p.add_grace_note(same, Some(62), GraceKind::Acciaccatura).unwrap();
    assert_eq!(p.cycle_link_type(g2), None);
    assert_eq!(p.set_grace_link(g2, LinkType::Slide), Err(GraceError::LinkInvalid));
}

#[test]
fn grace_notes_follow_the_no_overlap_rule() {
    let mut p = project();
    let long = add(&mut p, 58, 0.0, 3.0);
    let main = add(&mut p, 60, 3.0, 4.0);
    let g = p.add_grace_note(main, Some(58), GraceKind::Acciaccatura).unwrap();
    let (gs, ge) = secs(&p, g);
    close(secs(&p, long).1, gs);
    assert!(secs(&p, long).0 < gs && ge <= 3.0 + 1e-9, "the grace note cut the same-pitch note it landed on");
    let run_over = add(&mut p, 58, gs - 0.01, 3.5);
    p.resolve_overlaps(&[run_over]);
    assert!(p.note(g).is_none(), "an overlapped grace note is removed");
}

#[test]
fn several_graces_link_into_the_next_one_and_moves_stay_between_neighbours() {
    let mut p = project();
    let main = add(&mut p, 60, 3.0, 4.0);
    let first = p.add_grace_note(main, Some(57), GraceKind::Acciaccatura).unwrap();
    let second = p.add_grace_note(main, Some(55), GraceKind::Acciaccatura).unwrap();
    assert_eq!((p.grace_target(second), p.grace_target(first)), (Some(first), Some(main)));
    assert_eq!(p.outgoing(second).map(|o| o.0), Some(first));
    p.set_grace_timing(second, GraceKind::Acciaccatura, 0.5).unwrap();
    close(secs(&p, second).1, secs(&p, first).0);
    p.set_grace_timing(first, GraceKind::Acciaccatura, 0.2).unwrap();
    close(secs(&p, first).1, 3.0);
    close(secs(&p, first).0, 2.8);
    assert_eq!(p.grace_target(second), Some(first));
}
