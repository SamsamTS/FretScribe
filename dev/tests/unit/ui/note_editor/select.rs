use super::super::testkit::*;
use super::*;

fn setup() -> (fretscribe_core::Project, [u64; 4]) {
    let mut p = project();
    let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 72, 1.0, 2.0), add(&mut p, 48, 2.0, 3.0), add(&mut p, 55, 1.5, 2.5)];
    (p, ids)
}

#[test]
fn time_order_ignores_pitch_across_time_and_sorts_chords_low_to_high() {
    let (p, [a, b, c, d]) = setup();
    let tempo = TempoMap::new(&p.measures);
    assert_eq!(time_order(&p.notes, &tempo), [a, b, d, c]);
}

#[test]
fn click_ctrl_and_shift_range() {
    let (p, [a, b, c, d]) = setup();
    let order = time_order(&p.notes, &TempoMap::new(&p.measures));
    let mut s = Selection::default();
    s.click(a, false, false, &order);
    assert_eq!(s.ids, [a]);
    s.click(c, true, false, &order);
    assert_eq!(s.ids, [a, c]);
    s.click(a, true, false, &order);
    assert_eq!(s.ids, [c]);
    s.click(b, false, false, &order);
    s.click(c, false, true, &order);
    assert_eq!(s.ids, [b, d, c]);
    s.click(d, false, false, &order);
    assert_eq!(s.ids, [d]);
}

#[test]
fn shift_range_runs_backwards_too_and_adds_to_the_selection() {
    let (p, [a, b, c, d]) = setup();
    let order = time_order(&p.notes, &TempoMap::new(&p.measures));
    let mut s = Selection::default();
    s.click(c, false, false, &order);
    s.click(a, false, true, &order);
    assert_eq!(s.ids.len(), 4);
    assert!([a, b, c, d].iter().all(|&n| s.contains(n)));
}

#[test]
fn marquee_touches_and_ctrl_adds() {
    let (p, [a, b, _, d]) = setup();
    let tempo = TempoMap::new(&p.measures);
    let times = NoteTimes::build(&p.notes, &tempo);
    let hits = marquee_hits(&p.notes, &times, (0.3, 0.45), (59.6, 60.4));
    assert_eq!(hits, [a]);
    let mut s = Selection::default();
    s.click(b, false, false, &[]);
    s.marquee(&hits, true);
    assert_eq!(s.ids, [b, a]);
    s.marquee(&marquee_hits(&p.notes, &times, (0.0, 4.0), (54.0, 56.0)), false);
    assert_eq!(s.ids, [d]);
}

#[test]
fn step_and_select_all() {
    let (p, [a, b, c, d]) = setup();
    let order = time_order(&p.notes, &TempoMap::new(&p.measures));
    let mut s = Selection::default();
    s.step(true, false, &order);
    assert_eq!(s.ids, [a]);
    s.step(true, true, &order);
    assert_eq!(s.ids, [a, b]);
    s.step(false, false, &order);
    assert_eq!(s.ids, [a]);
    s.step(false, false, &order);
    assert_eq!(s.ids, [a]);
    s.select_all(&order);
    assert_eq!(s.ids, [a, b, d, c]);
}
