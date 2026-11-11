use fretscribe_core::{GraceKind, GraceNote, Legato, LegatoLink, Measure, Note, SnapGrid, TimePos, TimeSignature};

use super::*;

fn measures(n: usize, sig: TimeSignature) -> Vec<Measure> {
    let mut next = 1;
    let secs = f64::from(sig.numerator) * 4.0 / f64::from(sig.denominator) * 0.5 * n as f64;
    fretscribe_core::generate_measures(0.0, secs, 120.0, sig, &mut next)
}

fn note(ms: &[Measure], id: u64, midi: u8, from: f64, to: f64) -> Note {
    let pos = |b: f64| TimePos::new(ms[0].id, b);
    Note::new(id, midi, pos(from), pos(to))
}

fn run(notes: &[Note], n: usize, grid: &[u8]) -> StaffLayout {
    let opts = StaffOptions { grid: SnapGrid { divisions: grid.to_vec() }, ..Default::default() };
    layout(notes, &measures(n, TimeSignature::COMMON), &opts)
}

fn kinds(l: &StaffLayout) -> Vec<(bool, i64)> {
    l.events.iter().map(|e| (e.rest, e.duration.ticks())).collect()
}

#[test]
fn whole_measure_rest_only_up_to_last_note() {
    let ms = measures(3, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 4.0, 5.0)], &ms, &StaffOptions::default());
    assert!(l.events[0].whole_measure && l.events[0].rest);
    assert_eq!(l.events.iter().filter(|e| e.measure == 2).count(), 0);
}

#[test]
fn quarter_notes_and_rests() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 1.0), note(&ms, 2, 62, 2.0, 3.0)], &ms, &StaffOptions::default());
    assert_eq!(kinds(&l), vec![(false, 24), (true, 24), (false, 24), (true, 24)]);
}

#[test]
fn half_note_may_cross_the_middle_but_not_off_beat() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 2.0)], &ms, &StaffOptions::default());
    assert_eq!(kinds(&l), vec![(false, 48), (true, 24), (true, 24)]);
    let l = layout(&[note(&ms, 1, 60, 1.0, 3.0)], &ms, &StaffOptions::default());
    assert!(!l.events.iter().any(|e| !e.rest && e.duration.ticks() == 48));
    assert!(l.events.iter().filter(|e| !e.rest).all(|e| e.duration.ticks() == 24));
}

#[test]
fn tie_across_barline() {
    let ms = measures(2, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 3.0, 5.0)], &ms, &StaffOptions::default());
    let held: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(held.len(), 2);
    assert!(held[0].notes[0].tie_next && !held[1].notes[0].tie_next);
    assert_eq!((held[0].measure, held[1].measure), (0, 1));
}

#[test]
fn triplet_beat_and_bracket() {
    let ms = measures(1, TimeSignature::COMMON);
    let third = 1.0 / 3.0;
    let notes: Vec<Note> = (0..3).map(|i| note(&ms, i + 1, 60, f64::from(i as u32) * third, (f64::from(i as u32) + 1.0) * third)).collect();
    let l = layout(&notes, &ms, &StaffOptions::default());
    let held: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(held.len(), 3);
    assert!(held.iter().all(|e| e.duration.triplet && e.duration.ticks() == 8));
    assert_eq!(l.tuplets.len(), 1);
    assert_eq!(l.tuplets[0].events.len(), 3);
}

#[test]
fn sixteenths_are_beamed_per_beat() {
    let ms = measures(1, TimeSignature::COMMON);
    let notes: Vec<Note> = (0..8).map(|i| note(&ms, i + 1, 60, f64::from(i as u32) * 0.25, f64::from(i as u32) * 0.25 + 0.25)).collect();
    let l = run(&notes, 1, &[4]);
    let beamed: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(beamed.len(), 8);
    assert_eq!(l.beams.len(), 2);
    assert!(l.beams.iter().all(|b| b.events.len() == 4));
}

#[test]
fn chord_cuts_where_sounding_set_changes() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 2.0), note(&ms, 2, 64, 0.0, 1.0)], &ms, &StaffOptions::default());
    assert_eq!(l.events[0].notes.len(), 2);
    assert_eq!(l.events[1].notes.len(), 1);
    assert!(l.events[0].notes[0].tie_next && !l.events[0].notes[1].tie_next);
}

#[test]
fn spelling_and_guitar_octave() {
    // E2 sounds, E3 is written; E4 is the bottom line of the treble staff.
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 40, 0.0, 1.0)], &ms, &StaffOptions::default());
    assert_eq!(l.events[0].notes[0].step, 0 - 7);
    let l = layout(&[note(&ms, 1, 40, 0.0, 1.0)], &ms, &StaffOptions { guitar_octave: false, ..Default::default() });
    assert_eq!(l.events[0].notes[0].step, 0 - 14);
    let (g, ab) = (spell(61, KeySignature(1)), spell(61, KeySignature(-4)));
    assert_eq!((g.letter, g.alter), (0, 1), "C#4 in G major");
    assert_eq!((ab.letter, ab.alter), (1, -1), "Db4 in Ab major");
}

#[test]
fn accidental_once_per_measure_and_natural_after() {
    let ms = measures(2, TimeSignature::COMMON);
    let notes = [note(&ms, 1, 61, 0.0, 1.0), note(&ms, 2, 61, 1.0, 2.0), note(&ms, 3, 60, 2.0, 3.0)];
    let l = run(&notes, 1, &[4]);
    let held: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(held[0].notes[0].accidental, Some(Accidental::Sharp));
    assert_eq!(held[1].notes[0].accidental, None);
    assert_eq!(held[2].notes[0].accidental, Some(Accidental::Natural));
}

#[test]
fn grace_notes_take_no_time() {
    let ms = measures(1, TimeSignature::COMMON);
    let mut g = note(&ms, 2, 62, 0.9, 1.0);
    g.grace_of = Some(GraceNote { main: 1, kind: GraceKind::Acciaccatura });
    let l = layout(&[note(&ms, 1, 64, 1.0, 2.0), g], &ms, &StaffOptions::default());
    let main = l.events.iter().find(|e| !e.rest).unwrap();
    assert_eq!(main.graces.len(), 1);
    assert!(main.graces[0].slashed);
    assert!(l.note_ref(2).unwrap().grace);
    assert_eq!(l.events.iter().map(|e| e.duration.ticks()).sum::<i64>(), 96);
}

#[test]
fn links_are_reported() {
    let ms = measures(1, TimeSignature::COMMON);
    let mut a = note(&ms, 1, 60, 0.0, 1.0);
    let mut b = note(&ms, 2, 62, 1.0, 2.0);
    b.legato = Some(LegatoLink { kind: Legato::HammerOn, from: 1 });
    a.link_to = None;
    let l = layout(&[a, b], &ms, &StaffOptions::default());
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].kind, LinkKind::HammerOn);
}

#[test]
fn compound_meter_beat_is_dotted_quarter() {
    assert_eq!(value::main_beat_ticks(6, 8), 36);
    assert_eq!(value::main_beat_ticks(3, 4), 24);
}

#[test]
fn x_follows_real_time() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.02, 1.0)], &ms, &StaffOptions::default());
    assert!((l.events[0].x_sec - 0.01).abs() < 1e-6, "{}", l.events[0].x_sec);
}

#[test]
fn five_eighths_is_tied_components() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 2.5)], &ms, &StaffOptions::default());
    let held: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(held.iter().map(|e| e.duration.ticks()).sum::<i64>(), 60);
    assert!(held.len() >= 2 && held[..held.len() - 1].iter().all(|e| e.notes[0].tie_next));
}

#[test]
fn quarter_on_beat_two_gets_quarter_rests() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 1.0, 2.0)], &ms, &StaffOptions::default());
    assert_eq!(kinds(&l), vec![(true, 24), (false, 24), (true, 24), (true, 24)]);
}

#[test]
fn empty_middle_measure_is_one_whole_rest_and_trailing_has_none() {
    let ms = measures(4, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 1.0), note(&ms, 2, 60, 8.0, 9.0)], &ms, &StaffOptions::default());
    let in_measure = |m: usize| l.events.iter().filter(|e| e.measure == m).collect::<Vec<_>>();
    assert!(in_measure(1)[0].whole_measure && in_measure(1).len() == 1);
    assert!(in_measure(3).is_empty());
    assert_eq!(in_measure(2).iter().map(|e| e.duration.ticks()).sum::<i64>(), 96);
}

#[test]
fn grace_notes_do_not_change_rests() {
    let ms = measures(1, TimeSignature::COMMON);
    let main = note(&ms, 1, 64, 1.0, 2.0);
    let plain = layout(std::slice::from_ref(&main), &ms, &StaffOptions::default());
    let mut g = note(&ms, 2, 62, 0.95, 1.0);
    g.grace_of = Some(GraceNote { main: 1, kind: GraceKind::Acciaccatura });
    let with = layout(&[main, g], &ms, &StaffOptions::default());
    assert_eq!(kinds(&plain), kinds(&with));
}
