use super::super::testkit::*;
use super::*;
use fretscribe_core::{GraceKind, GraceNote, LinkType};

#[test]
fn copy_keeps_relative_timing_and_internal_links_and_paste_gives_fresh_ids() {
    let mut p = project();
    let (a, b, outside) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.5, 3.0), add(&mut p, 64, 3.0, 4.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    p.link_notes(b, outside, LinkType::Slide).unwrap();
    let tempo = TempoMap::new(&p.measures);
    let clip = Clipboard::copy(&p, &tempo, &[a, b]).unwrap();
    let (edit, ids) = clip.commit(&mut p, &tempo, at(5.0), "Paste").unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.iter().all(|id| ![a, b, outside].contains(id)));
    let (na, nb) = (p.note(ids[0]).unwrap(), p.note(ids[1]).unwrap());
    let beats = |p| tempo.pos_to_beats(p).unwrap();
    assert_eq!((beats(na.start), beats(na.end)), (5.0, 6.5));
    assert_eq!((beats(nb.start), beats(nb.end)), (6.5, 7.0));
    assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
    assert_eq!(p.outgoing(ids[1]), None);
    assert_eq!(p.note(ids[1]).unwrap().technique, None);
    assert_eq!(p.outgoing(b), Some((outside, LinkType::Slide)));
    edit.undo(&mut p);
    assert_eq!(p.notes.len(), 3);
}

#[test]
fn grace_notes_travel_with_their_main_note_and_orphans_stay_behind() {
    let mut p = project();
    let (main, grace, lone) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 0.9, 1.0), add(&mut p, 64, 3.0, 4.0));
    for id in [grace, lone] {
        let m = if id == grace { main } else { 999 };
        p.notes.iter_mut().find(|n| n.id == id).unwrap().grace_of = Some(GraceNote { main: m, kind: GraceKind::Acciaccatura });
    }
    let tempo = TempoMap::new(&p.measures);
    let clip = Clipboard::copy(&p, &tempo, &[main, lone]).unwrap();
    let notes = clip.paste_notes(&tempo, at(8.0), 100);
    assert_eq!(notes.len(), 2);
    let g = notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    assert_eq!(g.grace_of.unwrap().main, 100);
    assert!((tempo.pos_to_beats(g.start).unwrap() - 8.0).abs() < 1e-9);
}

#[test]
fn ghost_shows_the_pasted_links_and_duplicate_lands_after_the_selection() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0));
    p.link_notes(a, b, LinkType::Legato).unwrap();
    let tempo = TempoMap::new(&p.measures);
    let clip = Clipboard::copy(&p, &tempo, &[a, b]).unwrap();
    let ghost = clip.ghost(&tempo, at(4.0));
    assert_eq!(ghost.glyphs.len(), 2);
    assert_eq!(ghost.glyphs[1].legato_from, Some(0));
    assert_eq!(ghost.glyphs[0].start_sec, sec(4.0));
    assert_eq!(clip.end_pos(&tempo), at(3.0));
}

#[test]
fn copying_nothing_gives_no_clipboard() {
    let p = project();
    assert!(Clipboard::copy(&p, &TempoMap::new(&p.measures), &[]).is_none());
}

#[test]
fn paste_drops_automatic_strings_but_keeps_locked_ones() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0));
    for (id, lock) in [(a, false), (b, true)] {
        let n = p.notes.iter_mut().find(|n| n.id == id).unwrap();
        (n.string_fret, n.locked_fret) = (Some((2, 5)), lock);
    }
    let tempo = TempoMap::new(&p.measures);
    let notes = Clipboard::copy(&p, &tempo, &[a, b]).unwrap().paste_notes(&tempo, at(5.0), 100);
    assert_eq!((notes[0].string_fret, notes[1].string_fret), (None, Some((2, 5))));
}
