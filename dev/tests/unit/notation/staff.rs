use fretscribe_core::{GraceKind, GraceNote, Legato, LegatoLink, Measure, Note, TimePos, TimeSignature};

use super::*;

fn measures(n: usize, sig: TimeSignature) -> Vec<Measure> {
    let mut next = 1;
    let secs = f64::from(sig.numerator) * 4.0 / f64::from(sig.denominator) * 0.5 * n as f64;
    fretscribe_core::generate_measures(0.0, secs, 120.0, sig, &mut next)
}

fn note(ms: &[Measure], id: u64, midi: u8, from: f64, to: f64) -> Note {
    let pos = |b: f64| TimePos::at(ms[0].id, b);
    Note::new(id, midi, pos(from), pos(to))
}

fn run(notes: &[Note], n: usize) -> StaffLayout {
    layout(notes, &measures(n, TimeSignature::COMMON), &StaffOptions::default())
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
    assert_eq!(kinds(&l), vec![(false, 48), (true, 48), (false, 48), (true, 48)]);
}

#[test]
fn half_note_may_cross_the_middle_but_not_off_beat() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 2.0)], &ms, &StaffOptions::default());
    assert_eq!(kinds(&l), vec![(false, 96), (true, 48), (true, 48)]);
    let l = layout(&[note(&ms, 1, 60, 1.0, 3.0)], &ms, &StaffOptions::default());
    assert!(!l.events.iter().any(|e| !e.rest && e.duration.ticks() == 96));
    assert!(l.events.iter().filter(|e| !e.rest).all(|e| e.duration.ticks() == 48));
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
    assert!(held.iter().all(|e| e.duration.triplet && e.duration.ticks() == 16));
    assert_eq!(l.tuplets.len(), 1);
    assert_eq!(l.tuplets[0].events.len(), 3);
}

#[test]
fn sixteenths_are_beamed_per_beat() {
    let ms = measures(1, TimeSignature::COMMON);
    let notes: Vec<Note> = (0..8).map(|i| note(&ms, i + 1, 60, f64::from(i as u32) * 0.25, f64::from(i as u32) * 0.25 + 0.25)).collect();
    let l = run(&notes, 1);
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
    let l = run(&notes, 1);
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
    assert_eq!(l.events.iter().map(|e| e.duration.ticks()).sum::<i64>(), 192);
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
    assert_eq!(value::main_beat_ticks(6, 8), 72);
    assert_eq!(value::main_beat_ticks(3, 4), 48);
}

#[test]
fn x_follows_real_time() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 1.0 / 48.0, 1.0)], &ms, &StaffOptions::default());
    assert!((l.events[0].x_sec - 0.5 / 48.0).abs() < 1e-6, "{}", l.events[0].x_sec);
}

#[test]
fn five_eighths_is_tied_components() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 2.5)], &ms, &StaffOptions::default());
    let held: Vec<_> = l.events.iter().filter(|e| !e.rest).collect();
    assert_eq!(held.iter().map(|e| e.duration.ticks()).sum::<i64>(), 120);
    assert!(held.len() >= 2 && held[..held.len() - 1].iter().all(|e| e.notes[0].tie_next));
}

#[test]
fn quarter_on_beat_two_gets_quarter_rests() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 1.0, 2.0)], &ms, &StaffOptions::default());
    assert_eq!(kinds(&l), vec![(true, 48), (false, 48), (true, 48), (true, 48)]);
}

#[test]
fn empty_middle_measure_is_one_whole_rest_and_trailing_has_none() {
    let ms = measures(4, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 1.0), note(&ms, 2, 60, 8.0, 9.0)], &ms, &StaffOptions::default());
    let in_measure = |m: usize| l.events.iter().filter(|e| e.measure == m).collect::<Vec<_>>();
    assert!(in_measure(1)[0].whole_measure && in_measure(1).len() == 1);
    assert!(in_measure(3).is_empty());
    assert_eq!(in_measure(2).iter().map(|e| e.duration.ticks()).sum::<i64>(), 192);
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

fn ticks_of(l: &StaffLayout) -> Vec<(i64, u8, bool)> {
    l.events.iter().filter(|e| !e.rest).map(|e| (e.duration.ticks(), e.duration.dots, e.duration.triplet)).collect()
}

#[test]
fn half_plus_quarter_is_one_dotted_half_and_double_dots_exist() {
    let ms = measures(1, TimeSignature::COMMON);
    assert_eq!(ticks_of(&run(&[note(&ms, 1, 60, 0.0, 3.0)], 1)), vec![(144, 1, false)]);
    assert_eq!(ticks_of(&run(&[note(&ms, 1, 60, 0.0, 3.5)], 1)), vec![(168, 2, false)]);
    assert_eq!(ticks_of(&run(&[note(&ms, 1, 60, 0.0, 1.5)], 1)), vec![(72, 1, false)]);
}

#[test]
fn the_length_of_a_note_not_the_grid_decides_its_value() {
    let ms = measures(1, TimeSignature::COMMON);
    // A 1/32 triplet (1/12 beat) starting on a sixteenth position, nowhere near a triplet line.
    let l = run(&[note(&ms, 1, 60, 0.25, 0.25 + 1.0 / 12.0)], 1);
    assert_eq!(ticks_of(&l), vec![(4, 0, true)]);
    // A 1/64 note.
    assert_eq!(ticks_of(&run(&[note(&ms, 1, 60, 0.0, 1.0 / 16.0)], 1)), vec![(3, 0, false)]);
}

fn tied(a: Note, b: &Note) -> Note {
    let mut a = a;
    a.link_to = Some(b.id);
    a.technique = Some(fretscribe_core::Technique::Tie);
    a
}

#[test]
fn tied_notes_resolve_into_the_fewest_values() {
    let ms = measures(1, TimeSignature::COMMON);
    type Spans = &'static [(f64, f64)];
    let cases: [(Spans, (i64, u8)); 3] = [
        (&[(0.0, 0.25), (0.25, 0.5)], (24, 0)),
        (&[(0.0, 0.5), (0.5, 1.0), (1.0, 2.0)], (96, 0)),
        (&[(0.0, 0.5), (0.5, 1.0), (1.0, 1.5)], (72, 1)),
    ];
    for (spans, (ticks, dots)) in cases {
        let mut notes: Vec<Note> = spans.iter().enumerate().map(|(i, &(a, b))| note(&ms, i as u64 + 1, 60, a, b)).collect();
        for i in 0..notes.len() - 1 {
            let next = notes[i + 1].clone();
            notes[i] = tied(notes[i].clone(), &next);
        }
        let l = run(&notes, 1);
        assert_eq!(ticks_of(&l), vec![(ticks, dots, false)], "{spans:?}");
        assert!(!l.events[0].notes[0].tie_next);
        assert_eq!(l.note_ref(notes.len() as u64).map(|r| r.first), Some(0), "followers point at the merged event");
    }
}

#[test]
fn beat_rule_off_keeps_a_syncopated_quarter_whole() {
    let ms = measures(1, TimeSignature::COMMON);
    let n = [note(&ms, 1, 67, 0.75, 1.75)];
    let strict = layout(&n, &ms, &StaffOptions::default());
    assert_eq!(ticks_of(&strict), vec![(12, 0, false), (36, 1, false)]);
    let loose = layout(&n, &ms, &StaffOptions { beat_rule: false, ..Default::default() });
    assert_eq!(ticks_of(&loose), vec![(48, 0, false)]);
    // Still split when no single value has the length (2.5 beats), and rests keep the rule.
    let long = layout(&[note(&ms, 1, 67, 0.0, 2.5)], &ms, &StaffOptions { beat_rule: false, ..Default::default() });
    assert!(ticks_of(&long).len() >= 2);
    assert!(loose.events.iter().filter(|e| e.rest).all(|e| e.duration.ticks() <= 48 || e.beat_pos % 1.0 == 0.0));
}

#[test]
fn touching_notes_leave_no_rest() {
    let ms = measures(1, TimeSignature::COMMON);
    let l = layout(&[note(&ms, 1, 60, 0.0, 3.3), note(&ms, 2, 67, 3.3, 3.8)], &ms, &StaffOptions::default());
    let last = l.events.iter().rposition(|e| !e.rest).unwrap();
    assert!(l.events[..last].iter().all(|e| !e.rest), "{:?}", kinds(&l));
}

/// Stress test: 300 random monophonic notes whose edges sit on any straight (1/64) or triplet (1/64T) lattice position, touching or not.
/// Starts and ends must survive quantisation exactly, the layout must keep the sounding and silent totals and no event crosses a bar line (20 seeds, beat rule on and off).
#[test]
fn random_lattice_notes_survive_quantisation() {
    (1..=20).for_each(|seed| lattice_case(0x2545_F491_4F6C_DD1D ^ (seed << 20), seed % 2 == 0));
}

fn lattice_case(mut seed: u64, beat_rule: bool) {
    let mut rand = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let on = |t: i64, step: i64| (t + step - 1) / step * step;
    let (mut cursor, mut spans) = (0i64, Vec::new());
    for _ in 0..300 {
        let s = if rand(10) < 7 { cursor } else { cursor + rand(60) as i64 };
        let s = on(s, [2, 3][rand(2) as usize]);
        // Edges closer than the shortest value (2 ticks) cannot be written; they are merged.
        let s = if s - cursor == 1 { cursor } else { s };
        let step = [2, 3][rand(2) as usize];
        let e = on(s + 1 + rand(100) as i64, step).max(s + step);
        spans.push((s, e));
        cursor = e;
    }
    let ms = measures((cursor / 192 + 2) as usize, TimeSignature::COMMON);
    let beat = |t: i64| t as f64 / 48.0;
    let notes: Vec<Note> = spans.iter().enumerate().map(|(i, &(s, e))| note(&ms, i as u64 + 1, 60, beat(s), beat(e))).collect();

    let q = quant::quantise(&notes, &fretscribe_core::TempoMap::new(&ms));
    let got: Vec<(i64, i64)> = q.notes.iter().map(|n| (n.s, n.e)).collect();
    let bad: Vec<_> = got.iter().zip(&spans).enumerate().filter(|(_, (g, w))| g != w).collect();

    let l = layout(&notes, &ms, &StaffOptions { beat_rule, ..Default::default() });
    assert!(l.events.iter().all(|e| e.beat_pos + e.duration_beats <= 4.0 + 1e-9), "an event crosses a bar line");
    let sum = |rest: bool| l.events.iter().filter(|e| e.rest == rest).map(|e| e.duration.ticks()).sum::<i64>();
    let sound: i64 = spans.iter().map(|(s, e)| e - s).sum();
    let silence = spans.iter().zip(spans.iter().skip(1)).map(|(a, b)| b.0 - a.1).sum::<i64>() + spans[0].0 + (cursor - 1) / 192 * 192 + 192 - cursor;
    assert!(
        got.len() == spans.len() && bad.is_empty() && sum(false) == sound && sum(true) == silence,
        "{} of {} notes; sound {} vs {sound}, silence {} vs {silence}; first mismatches (index, got, want): {:?}",
        bad.len(),
        spans.len(),
        sum(false),
        sum(true),
        &bad[..bad.len().min(8)]
    );
}

#[test]
fn technique_runs_group_consecutive_events_and_a_plain_note_ends_them() {
    use fretscribe_core::Technique;
    let ms = measures(2, TimeSignature::COMMON);
    let techniques = [Some(Technique::PalmMute), Some(Technique::PalmMute), None, Some(Technique::PalmMute), None];
    let notes: Vec<Note> = techniques.iter().enumerate().map(|(i, &t)| Note { technique: t, vibrato: i == 4, ..note(&ms, i as u64 + 1, 60 + i as u8, i as f64, i as f64 + 1.0) }).collect();
    let l = run(&notes, 2);
    let held = |t| move |n: &crate::staff::StaffNote| n.technique == Some(t);
    let runs = technique_runs(&l, held(Technique::PalmMute));
    assert_eq!(runs.len(), 2);
    assert!(runs[0].first_sec < runs[0].last_sec && runs[1].first_sec == runs[1].last_sec);
    assert_eq!(technique_runs(&l, |n| n.vibrato).len(), 1);
    assert!(technique_runs(&l, held(Technique::Dead)).is_empty());
}
