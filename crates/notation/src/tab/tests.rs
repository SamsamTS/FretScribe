use fretscribe_core::{GraceKind, GraceNote, Legato, LegatoLink, Measure, Note, NoteTimes, TempoMap, TimePos, Technique, TimeSignature, Tuning};

use super::*;

struct Fixture {
    notes: Vec<Note>,
    tuning: Tuning,
    tempo: TempoMap,
    measure: u64,
}

// 120 bpm in 4/4: one measure is 2 s, so `beat` is 0.5 s.
fn fixture() -> Fixture {
    let mut next = 1;
    let measures: Vec<Measure> = fretscribe_core::generate_measures(0.0, 40.0, 120.0, TimeSignature { numerator: 4, denominator: 4 }, &mut next);
    Fixture { measure: measures[0].id, tempo: TempoMap::new(&measures), notes: Vec::new(), tuning: Tuning::standard() }
}

impl Fixture {
    fn add(&mut self, midi: u8, start: f64, end: f64) -> usize {
        let id = self.notes.len() as u64 + 1;
        let pos = |b: f64| TimePos::new(self.measure, b);
        self.notes.push(Note::new(id, midi, pos(start), pos(end)));
        self.notes.len() - 1
    }

    fn seq(&mut self, midis: &[u8]) -> Vec<usize> {
        midis.iter().enumerate().map(|(i, &m)| self.add(m, i as f64, i as f64 + 0.9)).collect()
    }

    fn solve(&mut self) -> Vec<u64> {
        let times = NoteTimes::build(&self.notes, &self.tempo);
        assign_tabs(&mut self.notes, &times, &self.tuning, 0, &TabOptions::default())
    }

    fn range(&mut self, edited: usize, back: bool) -> Vec<u64> {
        let times = NoteTimes::build(&self.notes, &self.tempo);
        assign_tabs_range(&mut self.notes, &times, &self.tuning, 0, &TabOptions::default(), edited, back)
    }

    fn at(&self, i: usize) -> Option<(u8, u8)> {
        self.notes[i].string_fret
    }

    fn lock(&mut self, i: usize, string: u8, fret: u8) {
        self.notes[i].string_fret = Some((string, fret));
        self.notes[i].locked_fret = true;
    }

    fn link(&mut self, from: usize, to: usize, technique: Technique) {
        self.notes[from].technique = Some(technique);
        self.notes[from].link_to = Some(self.notes[to].id);
    }

    fn legato(&mut self, from: usize, to: usize, kind: Legato) {
        self.notes[to].legato = Some(LegatoLink { kind, from: self.notes[from].id });
    }
}

#[test]
fn c_major_scale_stays_in_first_position() {
    let mut f = fixture();
    let ids = f.seq(&[48, 50, 52, 53, 55, 57, 59, 60]);
    assert!(f.solve().is_empty());
    for i in ids {
        assert!(f.at(i).is_some_and(|(_, fret)| fret <= 3), "{:?}", f.at(i));
    }
}

#[test]
fn melody_near_the_twelfth_fret_does_not_jump_to_open_strings() {
    let mut f = fixture();
    let ids = f.seq(&[71, 69, 67, 64, 67, 69, 71]);
    f.lock(ids[0], 4, 12);
    f.solve();
    for i in ids {
        assert!(f.at(i).is_some_and(|(_, fret)| fret >= 5), "{:?}", f.at(i));
    }
}

#[test]
fn chords_use_distinct_strings_within_the_hand_span() {
    let mut f = fixture();
    let ids: Vec<usize> = [48, 52, 55, 60, 64].iter().map(|&m| f.add(m, 0.0, 2.0)).collect();
    assert!(f.solve().is_empty());
    let mut strings: Vec<u8> = ids.iter().filter_map(|&i| f.at(i)).map(|s| s.0).collect();
    strings.sort_unstable();
    strings.dedup();
    assert_eq!(strings.len(), 5);
    let frets: Vec<u8> = ids.iter().filter_map(|&i| f.at(i)).map(|s| s.1).filter(|&x| x > 0).collect();
    assert!(frets.iter().max().unwrap_or(&0) - frets.iter().min().unwrap_or(&0) <= 4);
}

#[test]
fn locked_notes_are_respected_and_drop_d_gets_its_low_d() {
    let mut f = fixture();
    f.tuning = presets().remove(1);
    let low = f.add(38, 0.0, 1.0);
    let locked = f.add(52, 1.0, 2.0);
    f.lock(locked, 2, 2);
    f.solve();
    assert_eq!(f.at(low), Some((0, 0)));
    assert_eq!(f.at(locked), Some((2, 2)));
}

#[test]
fn out_of_range_notes_are_reported() {
    let mut f = fixture();
    let low = f.add(30, 0.0, 1.0);
    let ok = f.add(52, 1.0, 2.0);
    assert_eq!(f.solve(), vec![f.notes[low].id]);
    assert_eq!(f.at(low), None);
    assert!(f.at(ok).is_some());
}

#[test]
fn slide_stays_on_one_string_even_when_costlier() {
    let mut f = fixture();
    let ids = f.seq(&[57, 64]);
    f.link(ids[0], ids[1], Technique::Slide);
    f.solve();
    let (a, b) = (f.at(ids[0]).unwrap(), f.at(ids[1]).unwrap());
    assert_eq!(a.0, b.0);
    assert_eq!(f.tuning.open_strings[a.0 as usize] + b.1, 64);
}

#[test]
fn bend_and_release_chain_shares_the_fret_of_the_lowest_pitch() {
    let mut f = fixture();
    let ids = f.seq(&[67, 69, 67]);
    f.link(ids[0], ids[1], Technique::Bend);
    f.link(ids[1], ids[2], Technique::Bend);
    f.solve();
    let all: Vec<_> = ids.iter().map(|&i| f.at(i).unwrap()).collect();
    assert!(all.iter().all(|&s| s == all[0]));
    assert_eq!(f.tuning.open_strings[all[0].0 as usize] + all[0].1, 67);
}

#[test]
fn slide_then_bend_bends_from_the_slide_destination() {
    let mut f = fixture();
    let ids = f.seq(&[62, 67, 69]);
    f.link(ids[0], ids[1], Technique::Slide);
    f.link(ids[1], ids[2], Technique::Bend);
    f.solve();
    let (b, c) = (f.at(ids[1]).unwrap(), f.at(ids[2]).unwrap());
    assert_eq!(f.at(ids[0]).unwrap().0, b.0);
    assert_eq!(b, c);
    assert_eq!(f.tuning.open_strings[b.0 as usize] + b.1, 67);
}

#[test]
fn unplayable_chain_clears_all_its_notes() {
    let mut f = fixture();
    let ids = f.seq(&[40, 90]);
    f.link(ids[0], ids[1], Technique::Slide);
    let warned = f.solve();
    assert_eq!(warned.len(), 2);
    assert!(ids.iter().all(|&i| f.at(i).is_none()));
}

#[test]
fn locking_the_middle_note_of_a_chain_fixes_the_string() {
    let mut f = fixture();
    let ids = f.seq(&[60, 62, 64]);
    f.link(ids[0], ids[1], Technique::Slide);
    f.link(ids[1], ids[2], Technique::Slide);
    f.lock(ids[1], 3, 7);
    f.solve();
    assert_eq!([f.at(ids[0]), f.at(ids[1]), f.at(ids[2])], [Some((3, 5)), Some((3, 7)), Some((3, 9))]);
}

#[test]
fn hammer_on_and_pull_off_derive_frets_on_the_source_string() {
    let mut f = fixture();
    let ids = f.seq(&[57, 59, 57]);
    f.legato(ids[0], ids[1], Legato::HammerOn);
    f.legato(ids[1], ids[2], Legato::PullOff);
    f.lock(ids[0], 1, 12);
    f.solve();
    assert_eq!([f.at(ids[0]), f.at(ids[1]), f.at(ids[2])], [Some((1, 12)), Some((1, 14)), Some((1, 12))]);
}

#[test]
fn hammer_on_followed_by_a_slide_is_one_group() {
    let mut f = fixture();
    let ids = f.seq(&[52, 54, 59]);
    f.legato(ids[0], ids[1], Legato::HammerOn);
    f.link(ids[1], ids[2], Technique::Slide);
    f.solve();
    let strings: Vec<u8> = ids.iter().map(|&i| f.at(i).unwrap().0).collect();
    assert!(strings.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn grace_note_hammers_into_its_main_note() {
    let mut f = fixture();
    let main = f.add(57, 1.0, 2.0);
    let grace = f.add(55, 0.9, 1.0);
    f.notes[grace].grace_of = Some(GraceNote { main: f.notes[main].id, kind: GraceKind::Acciaccatura });
    f.legato(grace, main, Legato::HammerOn);
    f.solve();
    assert_eq!(f.at(grace).unwrap().0, f.at(main).unwrap().0);
}

#[test]
fn unlinked_grace_note_prefers_the_main_notes_string() {
    let mut f = fixture();
    let main = f.add(66, 1.0, 2.0);
    let grace = f.add(64, 0.9, 1.0);
    f.notes[grace].grace_of = Some(GraceNote { main: f.notes[main].id, kind: GraceKind::Acciaccatura });
    f.lock(main, 4, 7);
    f.solve();
    assert_eq!(f.at(grace), Some((4, 5)));
}

#[test]
fn legato_group_without_a_placement_is_reported() {
    let mut f = fixture();
    let ids = f.seq(&[40, 70]);
    f.legato(ids[0], ids[1], Legato::HammerOn);
    assert_eq!(f.solve().len(), 2);
}

fn run_line(f: &mut Fixture) -> Vec<usize> {
    let ids = f.seq(&[55, 57, 59, 60, 62, 64, 62, 60]);
    f.solve();
    ids
}

#[test]
fn locking_far_away_pulls_the_following_notes_along() {
    let mut f = fixture();
    let ids = run_line(&mut f);
    f.lock(ids[1], 2, 7);
    let changed = f.range(ids[1], false);
    assert!(!changed.is_empty());
    assert!(f.at(ids[2]).is_some_and(|(_, fret)| fret >= 4), "{:?}", f.at(ids[2]));
    assert!(f.at(ids[3]).is_some_and(|(_, fret)| fret >= 4), "{:?}", f.at(ids[3]));
}

#[test]
fn the_next_locked_note_stops_the_propagation() {
    let mut f = fixture();
    let ids = run_line(&mut f);
    f.lock(ids[5], 5, 0);
    f.solve();
    let after: Vec<_> = (6..8).map(|i| f.at(ids[i])).collect();
    f.lock(ids[1], 2, 7);
    f.range(ids[1], false);
    assert_eq!(f.at(ids[5]), Some((5, 0)));
    assert_eq!((6..8).map(|i| f.at(ids[i])).collect::<Vec<_>>(), after);
}

#[test]
fn earlier_notes_are_untouched_unless_propagating_backwards() {
    let mut f = fixture();
    let ids = run_line(&mut f);
    let before: Vec<_> = (0..2).map(|i| f.at(ids[i])).collect();
    f.lock(ids[3], 2, 10);
    f.range(ids[3], false);
    assert_eq!((0..2).map(|i| f.at(ids[i])).collect::<Vec<_>>(), before);
    f.range(ids[3], true);
    assert!(f.at(ids[2]).is_some_and(|(_, fret)| fret >= 7), "{:?}", f.at(ids[2]));
}

#[test]
fn resolving_is_idempotent_and_reset_notes_rejoin_the_segment() {
    let mut f = fixture();
    let ids = run_line(&mut f);
    f.lock(ids[2], 2, 9);
    f.range(ids[2], false);
    let once: Vec<_> = f.notes.iter().map(|n| n.string_fret).collect();
    assert!(f.range(ids[2], false).is_empty());
    assert_eq!(f.notes.iter().map(|n| n.string_fret).collect::<Vec<_>>(), once);
    f.notes[ids[2]].locked_fret = false;
    f.range(ids[2], false);
    assert!(f.at(ids[2]).is_some());
}

#[test]
fn presets_cover_the_named_tunings() {
    let names: Vec<String> = presets().into_iter().map(|t| t.name).collect();
    assert_eq!(names.len(), 10);
    assert_eq!(names[0], "Standard");
    assert_eq!(candidates(64, &Tuning::standard(), 0, 22).first(), Some(&(1, 19)));
}

#[test]
fn group_options_follow_the_whole_chain() {
    let mut f = fixture();
    let ids = f.seq(&[64, 66, 64]);
    f.notes[ids[0]].link_to = Some(2);
    f.notes[ids[0]].technique = Some(Technique::Bend);
    f.notes[ids[1]].link_to = Some(3);
    f.notes[ids[1]].technique = Some(Technique::Bend);
    let options = group_options(&f.notes, &f.tuning, 0, &TabOptions::default(), 2);
    let strings: Vec<u8> = options.iter().map(|o| o.string).collect();
    assert_eq!(strings, [1, 2, 3, 4, 5]);
    assert!(options.iter().all(|o| o.frets.len() == 3 && o.fret_of(1) == o.fret_of(2) && o.fret_of(2) == o.fret_of(3)));
    assert_eq!(options.last().and_then(|o| o.fret_of(2)), Some(0));
    f.notes[ids[0]].technique = None;
    f.notes[ids[0]].link_to = None;
    assert_eq!(group_options(&f.notes, &f.tuning, 0, &TabOptions::default(), 1).len(), 5);
    assert!(group_options(&f.notes, &f.tuning, 0, &TabOptions::default(), 99).is_empty());
}
