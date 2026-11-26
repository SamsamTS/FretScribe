use super::super::testkit::*;
use super::*;

fn entry(p: &Project, main: u64) -> GraceEntry {
    GraceEntry::new(p, main).unwrap()
}

#[test]
fn keys_change_pitch_string_fret_and_flags() {
    let mut p = project();
    let main = add(&mut p, 64, 1.0, 2.0);
    let mut e = entry(&p, main);
    assert_eq!((e.midi, e.kind, e.link), (62, GraceKind::Acciaccatura, None));
    e.key(&p, EntryKey::Pitch { up: true, tone: false });
    e.key(&p, EntryKey::Pitch { up: false, tone: true });
    assert_eq!(e.midi, 61);
    e.key(&p, EntryKey::Pitch { up: true, tone: false });
    assert_eq!(e.placement(&p), Some((4, 3)));
    e.key(&p, EntryKey::String { up: false });
    assert_eq!(e.placement(&p), Some((3, 7)));
    e.key(&p, EntryKey::Digit(5));
    assert_eq!((e.midi, e.placement(&p)), (60, Some((3, 5))));
    e.key(&p, EntryKey::Digit(2));
    assert_eq!(e.midi, 57, "52 is no fret: the digit starts over");
    e.key(&p, EntryKey::Pitch { up: true, tone: false });
    e.key(&p, EntryKey::Digit(1));
    e.key(&p, EntryKey::Digit(2));
    assert_eq!((e.midi, e.placement(&p)), (67, Some((3, 12))));
    e.key(&p, EntryKey::String { up: true });
    assert_eq!(e.placement(&p), Some((4, 8)));
    e.key(&p, EntryKey::ToggleKind);
    assert_eq!(e.kind, GraceKind::Appoggiatura);
    assert_eq!(e.key(&p, EntryKey::Confirm), EntryStep::Done);
}

#[test]
fn the_pointer_side_gives_before_or_on_the_beat_and_the_distance_the_length() {
    let mut p = project();
    add(&mut p, 40, 0.0, 0.5);
    let main = add(&mut p, 64, 3.0, 4.0);
    let mut e = entry(&p, main);
    let placed = |e: &GraceEntry, p: &Project| {
        let mut s = p.clone();
        let id = e.place(&mut s).unwrap();
        let times = fretscribe_core::NoteTimes::build(&s.notes, &fretscribe_core::TempoMap::new(&s.measures));
        (times.start_sec(id).unwrap(), times.end_sec(id).unwrap(), s.grace_link(id))
    };
    let near = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
    let (start, end, link) = placed(&e, &p);
    near(end, 1.5);
    assert_eq!((e.kind, link), (GraceKind::Acciaccatura, LinkType::Legato));
    assert!(start < end);
    e.pointer(&p, 1.0, 62.2);
    assert_eq!((e.kind, e.midi), (GraceKind::Acciaccatura, 62));
    let (start, end, link) = placed(&e, &p);
    near(start, 1.0);
    near(end, 1.5);
    assert_eq!(link, LinkType::Legato, "before the beat the end touches the note");
    e.pointer(&p, 1.9, 62.0);
    assert_eq!(e.kind, GraceKind::Appoggiatura);
    let (start, end, link) = placed(&e, &p);
    near(start, 1.5);
    near(end, 1.9);
    assert_eq!(link, LinkType::Legato, "on the beat keeps the legato");
    e.key(&p, EntryKey::CycleLink);
    assert_eq!((e.link, placed(&e, &p).2), (Some(LinkType::Dashed), LinkType::Dashed));
    e.key(&p, EntryKey::CycleLink);
    assert_eq!(e.link, Some(LinkType::Slide));
    e.link = None;
    e.pointer(&p, 1.55, 62.0);
    assert_eq!(placed(&e, &p).2, LinkType::Legato);
    e.pointer(&p, 1.505, 63.0);
    assert_eq!((e.len, e.kind, e.midi), (None, GraceKind::Appoggiatura, 63), "dead zone: default length, same kind");
    e.key(&p, EntryKey::ToggleKind);
    assert_eq!(e.kind, GraceKind::Acciaccatura);
}

#[test]
fn preview_shows_the_grace_note_and_its_link() {
    let mut p = project();
    let main = add(&mut p, 64, 1.0, 2.0);
    let mut e = entry(&p, main);
    let (ghost, fb) = e.preview(&p).unwrap();
    assert_eq!((ghost.glyphs.len(), ghost.glyphs[0].grace, ghost.glyphs[1].legato_from), (2, true, Some(0)));
    assert_eq!(fb.labels[0].2, "B 3");
    e.key(&p, EntryKey::CycleLink);
    assert_eq!(e.preview(&p).unwrap().0.glyphs[0].link_to, Some((1, LinkKind::Dashed)));
    e.key(&p, EntryKey::CycleLink);
    assert_eq!(e.preview(&p).unwrap().0.glyphs[0].link_to, Some((1, LinkKind::Slide)));
    assert_eq!(p.notes.len(), 1);
}

#[test]
fn commit_is_one_undo_step_with_start_lock_and_link() {
    let mut p = project();
    let main = add(&mut p, 64, 1.0, 2.0);
    let before = p.notes.clone();
    let mut e = entry(&p, main);
    e.key(&p, EntryKey::String { up: false });
    e.pointer(&p, 0.65, 60.0);
    e.key(&p, EntryKey::String { up: false });
    let (edit, affected) = e.commit(&mut p).unwrap();
    let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    assert_eq!((grace.midi, grace.string_fret, grace.locked_fret), (60, Some((3, 5)), true));
    assert_eq!((grace.grace_of.unwrap().kind, p.grace_link(grace.id)), (GraceKind::Appoggiatura, LinkType::Legato));
    assert_eq!(affected, [grace.id, main]);
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
}

#[test]
fn the_default_legato_is_made_in_the_same_step() {
    let mut p = project();
    let main = add(&mut p, 64, 1.0, 2.0);
    let (edit, _) = entry(&p, main).commit(&mut p).unwrap();
    let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap().id;
    assert_eq!(p.note(main).unwrap().legato.map(|l| l.from), Some(grace));
    edit.undo(&mut p);
    assert_eq!(p.notes.len(), 1);
}

#[test]
fn x_makes_a_dead_grace_note_with_a_dashed_connector() {
    let mut p = project();
    let main = add(&mut p, 64, 1.0, 2.0);
    let mut dead = entry(&p, main);
    dead.key(&p, EntryKey::ToggleDead);
    dead.commit(&mut p).unwrap();
    let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    assert_eq!(grace.technique, Some(fretscribe_core::Technique::Dead));
    assert_eq!(p.grace_link(grace.id), LinkType::Dashed);
    assert!(p.note(main).unwrap().legato.is_none());
}
