use super::*;
use crate::model::{GraceKind, GraceNote, Note};
use crate::tracks::next_note_id;

fn project() -> Project {
    crate::testkit::project(8.0)
}

/// A note of `len` beats at `beat` of measure number `m` (1-based), on `string`.
fn put(p: &mut Project, m: u64, beat: f64, len: f64, string: u8) -> u64 {
    let id = next_note_id(p);
    let mut n = Note::new(id, 60, TimePos::at(m, beat), TimePos::at(m, beat + len));
    n.string_fret = Some((string, 3));
    p.notes.push(n);
    id
}

fn ring(p: &mut Project, m: usize) {
    p.set_let_ring(m, true, false).unwrap();
}

#[test]
fn ring_stops_at_the_barline_or_the_next_note_on_the_same_string_and_never_shortens() {
    let mut p = project();
    ring(&mut p, 0);
    let (a, b, c, d) = (put(&mut p, 1, 0.0, 1.0, 0), put(&mut p, 1, 1.0, 1.0, 1), put(&mut p, 1, 2.0, 1.0, 0), put(&mut p, 1, 3.0, 2.0, 2));
    let ends = ring_ends(&p);
    assert_eq!(ends[&a], TimePos::at(1, 2.0));
    assert_eq!(ends[&b], TimePos::new(p.measures[1].id, 0));
    assert_eq!(ends[&c], TimePos::new(p.measures[1].id, 0));
    assert_eq!(ends[&d], TimePos::at(1, 5.0), "never before the note's own end");
}

#[test]
fn the_last_measure_ends_at_its_nominal_end() {
    let mut p = project();
    let last = p.measures.len() - 1;
    ring(&mut p, last);
    let id = p.measures[last].id;
    let a = put(&mut p, id, 0.0, 1.0, 0);
    assert_eq!(ring_ends(&p)[&a], TimePos::at(p.measures[last].id, 4.0));
}

#[test]
fn only_chain_last_plain_placed_notes_in_flagged_measures_ring() {
    let mut p = project();
    ring(&mut p, 0);
    let (chained, tail, grace, dead, muted, unplaced, other_measure, main) = (
        put(&mut p, 1, 0.0, 1.0, 0), put(&mut p, 1, 1.0, 1.0, 0), put(&mut p, 1, 1.5, 0.25, 1), put(&mut p, 1, 2.0, 1.0, 2),
        put(&mut p, 1, 2.5, 1.0, 3), put(&mut p, 1, 3.0, 1.0, 4), put(&mut p, 2, 0.0, 1.0, 5), put(&mut p, 1, 3.5, 0.5, 5),
    );
    p.notes.iter_mut().find(|n| n.id == tail).unwrap().midi = 62;
    p.link_notes(chained, tail, crate::links::LinkType::Slide).unwrap();
    p.notes.iter_mut().find(|n| n.id == grace).unwrap().grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
    p.notes.iter_mut().find(|n| n.id == dead).unwrap().technique = Some(Technique::Dead);
    p.notes.iter_mut().find(|n| n.id == muted).unwrap().technique = Some(Technique::PalmMute);
    p.notes.iter_mut().find(|n| n.id == unplaced).unwrap().string_fret = None;
    let mut ids: Vec<u64> = ring_ends(&p).into_keys().collect();
    ids.sort_unstable();
    assert_eq!(ids, [tail, main], "{other_measure}");
}

#[test]
fn tempo_changes_do_not_break_the_comparison() {
    let mut p = project();
    p.set_tempo_scoped(1, 60.0, crate::measures::Propagation::Single, crate::edit::NotePolicy::FollowGrid).unwrap();
    ring(&mut p, 1);
    let id = p.measures[1].id;
    let a = put(&mut p, id, 0.0, 1.0, 0);
    assert_eq!(ring_ends(&p)[&a], TimePos::new(p.measures[2].id, 0));
}

#[test]
fn set_let_ring_propagates_until_the_first_differing_measure_and_can_be_single() {
    let mut p = project();
    p.measures[2].let_ring = true;
    let edit = p.set_let_ring(0, true, true).unwrap();
    assert_eq!(p.measures.iter().map(|m| m.let_ring).collect::<Vec<_>>(), [true, true, true, false]);
    edit.undo(&mut p);
    assert_eq!(p.measures.iter().map(|m| m.let_ring).collect::<Vec<_>>(), [false, false, true, false]);
    p.set_let_ring(0, true, false).unwrap();
    assert_eq!(p.measures.iter().map(|m| m.let_ring).collect::<Vec<_>>(), [true, false, true, false]);
    p.set_let_ring_range(1..3, true).unwrap();
    assert!(p.measures[..3].iter().all(|m| m.let_ring) && !p.measures[3].let_ring);
    assert!(matches!(p.set_let_ring(9, true, true), Err(crate::error::EditError::MeasureOutOfRange { .. })));
    assert!(p.set_let_ring_range(2..9, true).is_err());
}

#[test]
fn grid_edits_keep_or_copy_the_flag_and_old_files_load_with_false() {
    let mut p = project();
    p.set_let_ring(1, true, false).unwrap();
    p.split_measure(1, 2.0).unwrap();
    assert_eq!(p.measures.iter().map(|m| m.let_ring).collect::<Vec<_>>(), [false, true, true, false, false]);
    p.merge_measures(1).unwrap();
    assert!(p.measures[1].let_ring);
    p.insert_measure(1, crate::edit::NotePolicy::FollowGrid).unwrap();
    assert!(!p.measures[1].let_ring && p.measures[2].let_ring);
    let json = serde_json::to_string(&p.measures[0]).unwrap().replace(",\"let_ring\":false", "");
    assert!(!serde_json::from_str::<crate::model::Measure>(&json).unwrap().let_ring);
}
