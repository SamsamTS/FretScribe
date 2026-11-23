use super::*;
use crate::testkit::add;

use crate::LinkType;

fn project() -> Project {
    crate::testkit::project(8.0)
}

fn spans(p: &Project, midi: u8) -> Vec<(f64, f64)> {
    let mut v: Vec<_> = p
        .notes
        .iter()
        .filter(|n| n.midi == midi)
        .map(|n| (n.start.beat(), n.end.beat()))
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn overlapped_notes_are_cut_split_or_removed_and_other_pitches_stay() {
    let mut p = project();
    let (a, b, c) = (
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 60, 1.0, 2.0),
        add(&mut p, 60, 3.0, 3.5),
    );
    let other = add(&mut p, 64, 0.0, 4.0);
    let by_end = add(&mut p, 60, 0.5, 1.5);
    p.resolve_overlaps(&[by_end]);
    assert_eq!(
        spans(&p, 60),
        [(0.0, 0.5), (0.5, 1.5), (1.5, 2.0), (3.0, 3.5)]
    );
    assert!(p.note(a).is_some() && p.note(b).is_some() && p.note(c).is_some());
    assert_eq!(p.note(other).unwrap().end.beat(), 4.0);

    let big = add(&mut p, 60, 0.25, 3.25);
    p.resolve_overlaps(&[big]);
    assert_eq!(spans(&p, 60), [(0.0, 0.25), (0.25, 3.25), (3.25, 3.5)]);
    assert!(p.note(b).is_none());
}

#[test]
fn a_note_in_the_middle_splits_the_one_under_it_and_the_tail_keeps_the_links() {
    let mut p = project();
    let (a, long, next) = (
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 62, 1.0, 4.0),
        add(&mut p, 65, 4.0, 5.0),
    );
    p.link_notes(a, long, LinkType::Legato).unwrap();
    p.link_notes(long, next, LinkType::Legato).unwrap();
    let mid = add(&mut p, 62, 2.0, 3.0);
    p.resolve_overlaps(&[mid]);
    assert_eq!(spans(&p, 62), [(1.0, 2.0), (2.0, 3.0), (3.0, 4.0)]);
    let tail = p
        .notes
        .iter()
        .find(|n| n.midi == 62 && n.start.beat() == 3.0)
        .unwrap()
        .id;
    assert_eq!(p.note(next).unwrap().legato.map(|l| l.from), Some(tail));
    assert_eq!(p.note(long).unwrap().legato.map(|l| l.from), Some(a));
    assert!(p.note(tail).unwrap().legato.is_none());
}

#[test]
fn a_split_with_a_sliver_on_one_side_drops_it_and_a_covered_note_goes_with_its_links() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 2.0), add(&mut p, 64, 2.0, 3.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    let mid = add(&mut p, 60, 1.0 / 48.0, 1.0);
    p.resolve_overlaps(&[mid]);
    assert_eq!(spans(&p, 60), [(1.0 / 48.0, 1.0), (1.0, 2.0)]);
    let cover = add(&mut p, 60, 0.0, 3.0);
    p.resolve_overlaps(&[cover]);
    assert_eq!(spans(&p, 60), [(0.0, 3.0)]);
    assert_eq!(p.outgoing(a), None);
    assert!(p.note(b).is_some());
}

#[test]
fn resolving_one_note_of_a_huge_linked_project_stays_linear() {
    let mut p = project();
    let n = 20_000;
    for i in 0..n {
        let id = add(&mut p, 30 + (i % 60) as u8, i as f64, i as f64 + 0.5);
        if i + 1 < n {
            let note = p.notes.last_mut().unwrap();
            (note.link_to, note.technique) = (Some(id + 1), Some(Technique::Slide));
        }
    }
    let start = std::time::Instant::now();
    p.resolve_overlaps(&[1]);
    assert!(start.elapsed() < std::time::Duration::from_secs(1), "{:?}", start.elapsed());
    assert_eq!(p.note(2).unwrap().start, p.note(1).unwrap().end);
}
