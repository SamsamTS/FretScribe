use super::super::testkit::*;
use super::*;

fn pt(beat: f64, midi: f32) -> Pt {
    Pt { sec: sec(beat), midi }
}

fn draw(p: &Project, press: Pt, cur: Pt, was_slide: bool, alt: bool) -> Option<DrawPlan> {
    with_ctx(p, &[4, 3], alt, |ctx| plan_draw(press, cur, was_slide, (None, None), ctx))
}

#[test]
fn drawing_either_way_gives_the_same_note() {
    let p = project();
    let right = draw(&p, pt(1.0, 64.1), pt(2.02, 64.0), false, false).unwrap();
    let left = draw(&p, pt(2.02, 64.0), pt(1.0, 64.1), false, false).unwrap();
    assert_eq!(right.creation, left.creation);
    assert_eq!(right.creation.notes, [NewNote { midi: 64, start: at(1.0), end: at(2.0) }]);
    assert!(right.creation.links.is_empty());
}

#[test]
fn a_different_pitch_makes_two_linked_slide_notes_with_pitches_following_their_points() {
    let p = project();
    let plan = draw(&p, pt(1.0, 60.0), pt(3.0, 62.1), false, false).unwrap();
    let c = plan.creation;
    assert_eq!(c.notes[0], NewNote { midi: 60, start: at(1.0), end: at(3.0) });
    assert_eq!(c.notes[1], NewNote { midi: 62, start: at(3.0), end: at(3.25) });
    assert_eq!(c.links, [(Ref::New(0), Ref::New(1), LinkType::Slide)]);
    let back = draw(&p, pt(3.0, 62.0), pt(1.0, 60.0), false, false).unwrap().creation;
    assert_eq!((back.notes[0].midi, back.notes[1].midi), (60, 62));
}

#[test]
fn the_slide_source_ends_where_its_destination_starts() {
    let p = project();
    for end in [1.5, 1.0 + 1.0 / 3.0, 3.0] {
        let c = draw(&p, pt(1.0, 60.0), pt(end, 62.0), false, false).unwrap().creation;
        assert_eq!(c.notes[0].end, c.notes[1].start);
    }
}

#[test]
fn slide_hysteresis_enters_at_075_and_leaves_at_05() {
    let p = project();
    let notes = |midi: f32, was| draw(&p, pt(1.0, 60.0), pt(2.0, midi), was, false).unwrap().creation.notes.len();
    assert_eq!(notes(60.7, false), 1);
    assert_eq!(notes(60.8, false), 2);
    assert_eq!(notes(60.6, true), 2);
    assert_eq!(notes(60.4, true), 1);
    assert_eq!(notes(59.2, false), 2);
}

#[test]
fn a_click_or_a_tiny_span_creates_nothing() {
    let p = project();
    assert!(draw(&p, pt(1.0, 60.0), pt(1.0, 60.0), false, false).is_none());
    assert!(draw(&p, pt(1.0, 60.0), pt(1.04, 60.0), false, false).is_none());
    assert!(draw(&p, pt(1.0, 60.0), pt(1.32, 60.0), false, false).is_some());
}

#[test]
fn the_press_snaps_to_the_closest_line_and_the_note_is_at_least_one_division_long() {
    let p = project();
    let plan = draw(&p, pt(1.03, 60.0), pt(1.09, 60.0), false, false);
    assert!(plan.is_none(), "less than half a division is still a click");
    let plan = draw(&p, pt(1.03, 60.0), pt(1.2, 60.0), false, false).unwrap();
    let n = &plan.creation.notes[0];
    assert_eq!(n.start, at(1.0));
    assert!((n.end.beat() - n.start.beat() - 0.25).abs() < 1e-9, "one 1/16 long: {n:?}");
    let back = draw(&p, pt(1.03, 60.0), pt(0.85, 60.0), false, false).unwrap();
    assert_eq!(back.creation.notes[0].end, at(1.0));
}

#[test]
fn both_ends_snap_and_alt_disables_it() {
    let p = project();
    let plan = draw(&p, pt(1.03, 60.0), pt(2.27, 60.0), false, false).unwrap();
    assert_eq!(plan.creation.notes[0].start, at(1.0));
    assert_eq!(plan.creation.notes[0].end, at(2.25));
    let free = draw(&p, pt(1.03, 60.0), pt(2.27, 60.0), false, true).unwrap();
    assert!((free.creation.notes[0].start.beat() - 1.03).abs() < 0.011);
    assert!((free.creation.notes[0].end.beat() - 2.27).abs() < 0.011);
}

#[test]
fn drawing_snaps_across_a_tempo_change() {
    let mut p = project();
    p.set_tempo_scoped(1, 60.0, fretscribe_core::Propagation::Single, fretscribe_core::NotePolicy::FollowGrid).unwrap();
    let (m1, m2) = (p.measures[0].id, p.measures[1].id);
    let tempo = TempoMap::new(&p.measures);
    let grid = fretscribe_core::SnapGrid::default();
    let ctx = EditCtx { project: &p, tempo: &tempo, grid: &grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, onsets: None, alt: false };
    let start = Pt { sec: tempo.pos_to_sec(TimePos::at(m1, 3.0)), midi: 60.0 };
    let end = Pt { sec: tempo.pos_to_sec(TimePos::at(m2, 1.02)), midi: 60.0 };
    let c = plan_draw(start, end, false, (None, None), &ctx).unwrap().creation;
    assert_eq!((c.notes[0].start, c.notes[0].end), (TimePos::at(m1, 3.0), TimePos::at(m2, 1.0)));
}

#[test]
fn chain_extension_links_from_the_last_note_and_follows_a_bend() {
    let mut p = project();
    let a = add(&mut p, 60, 0.0, 1.0);
    let plan = with_ctx(&p, &[4, 3], false, |ctx| plan_chain(a, pt(2.0, 62.0), (None, None), ctx)).unwrap();
    assert_eq!(plan.creation.links, [(Ref::Existing(a), Ref::New(0), LinkType::Slide)]);
    assert_eq!(plan.creation.notes, [NewNote { midi: 62, start: at(2.0), end: at(2.25) }]);
    let (_, ids) = commit_creation(&mut p, &plan.creation, "Chain").unwrap();
    p.unlink(a);
    p.link_notes(a, ids[0], LinkType::Bend).unwrap();
    let again = with_ctx(&p, &[4, 3], false, |ctx| plan_chain(ids[0], pt(4.0, 64.0), (None, None), ctx)).unwrap();
    assert_eq!(again.creation.links[0].2, LinkType::Bend);
    assert!(with_ctx(&p, &[4, 3], false, |ctx| plan_chain(a, pt(4.0, 64.0), (None, None), ctx)).is_none());
    assert!(with_ctx(&p, &[4, 3], false, |ctx| plan_chain(ids[0], pt(4.0, 62.2), (None, None), ctx)).is_none());
}

#[test]
fn committing_a_slide_is_one_undoable_edit_with_fresh_ids() {
    let mut p = project();
    let existing = add(&mut p, 50, 0.0, 1.0);
    let plan = draw(&p, pt(2.0, 60.0), pt(3.0, 62.0), false, false).unwrap();
    let before = p.notes.clone();
    let (edit, ids) = commit_creation(&mut p, &plan.creation, "Draw").unwrap();
    assert_eq!(ids, [existing + 1, existing + 2]);
    assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
}

fn aim(p: &Project, src: u64, cur: Pt, on: Option<u64>) -> LinkAim {
    with_ctx(p, &[4, 3], false, |ctx| plan_aim(src, cur, on, (None, None), None, ctx))
}

fn start(p: &Project, src: u64, cur: Pt) -> LinkStart {
    let LinkAim::Start { start, .. } = aim(p, src, cur, None) else { panic!("start") };
    start
}

#[test]
fn inside_the_source_is_a_legato_on_the_near_half_s_side() {
    let mut p = project();
    let s = add(&mut p, 60, 2.0, 3.0);
    let fwd = start(&p, s, pt(2.8, 62.0));
    assert_eq!((fwd.forward, fwd.kind, fwd.midi, fwd.near), (true, LinkType::Legato, 62, at(3.0)));
    let rev = start(&p, s, pt(2.2, 58.0));
    assert_eq!((rev.forward, rev.kind, rev.near), (false, LinkType::Legato, at(2.0)));
    let far = add(&mut p, 40, 5.0, 6.0);
    assert_eq!(start(&p, far, pt(5.8, 65.0)).kind, LinkType::Slide, "no string fits");
}

#[test]
fn past_either_end_is_a_slide_starting_at_the_pointer() {
    let mut p = project();
    let s = add(&mut p, 60, 2.0, 3.0);
    let fwd = start(&p, s, pt(4.02, 62.0));
    assert_eq!((fwd.forward, fwd.kind, fwd.near), (true, LinkType::Slide, TimePos::at(2, 0.0)));
    let rev = start(&p, s, pt(1.0, 58.0));
    assert_eq!((rev.forward, rev.kind, rev.near), (false, LinkType::Slide, at(1.0)));
    let LinkAim::Start { snapped, .. } = aim(&p, s, pt(4.02, 62.0), None) else { panic!() };
    assert_eq!(snapped, (Some(TimePos::at(2, 0.0)), None));
}

#[test]
fn aim_needs_a_different_pitch_and_a_free_side() {
    let mut p = project();
    let (s, t) = (add(&mut p, 60, 2.0, 3.0), add(&mut p, 62, 3.0, 4.0));
    assert_eq!(aim(&p, s, pt(4.0, 60.2), None), LinkAim::Hidden);
    p.link_notes(s, t, LinkType::Slide).unwrap();
    assert_eq!(aim(&p, s, pt(5.0, 65.0), None), LinkAim::Hidden);
    assert!(matches!(aim(&p, s, pt(1.0, 65.0), None), LinkAim::Start { .. }));
}

#[test]
fn extending_runs_from_the_fixed_point_and_never_collapses() {
    let mut p = project();
    let s = add(&mut p, 60, 2.0, 3.0);
    let fwd = start(&p, s, pt(2.8, 62.0));
    let run = |st: &LinkStart, sec: Option<f64>| with_ctx(&p, &[4, 3], false, |ctx| plan_extend(st, sec, (None, None), ctx));
    let plan = run(&fwd, Some(sec(5.0)));
    assert_eq!(plan.creation.notes, [NewNote { midi: 62, start: at(3.0), end: TimePos::at(2, 1.0) }]);
    assert_eq!(plan.creation.links, [(Ref::Existing(s), Ref::New(0), LinkType::Legato)]);
    assert_eq!(plan.snapped, (None, Some(TimePos::at(2, 1.0))));
    assert_eq!(run(&fwd, None).creation.notes[0].end, at(3.25));
    assert_eq!(run(&fwd, Some(sec(2.0))).creation.notes[0].end, at(3.25));
    let rev = start(&p, s, pt(2.2, 58.0));
    let plan = run(&rev, Some(sec(0.5)));
    assert_eq!(plan.creation.notes, [NewNote { midi: 58, start: at(0.5), end: at(2.0) }]);
    assert_eq!(plan.creation.links, [(Ref::New(0), Ref::Existing(s), LinkType::Legato)]);
}

#[test]
fn aim_over_a_note_joins_it_when_valid_in_either_direction() {
    let mut p = project();
    let (s, t) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.05, 2.0));
    let LinkAim::Join(c) = aim(&p, s, pt(1.5, 62.0), Some(t)) else { panic!("join") };
    assert!(c.notes.is_empty());
    assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(t), LinkType::Legato)]);
    let LinkAim::Join(c) = aim(&p, t, pt(0.3, 60.0), Some(s)) else { panic!("join before") };
    assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(t), LinkType::Legato)]);
    let far = add(&mut p, 64, 3.0, 4.0);
    let LinkAim::Join(c) = aim(&p, s, pt(3.5, 64.0), Some(far)) else { panic!("far") };
    assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(far), LinkType::Slide)]);
    let same = add(&mut p, 60, 5.0, 6.0);
    assert_eq!(aim(&p, s, pt(5.5, 60.0), Some(same)), LinkAim::Hidden);
}

#[test]
fn slide_end_follows_the_pointer_and_keeps_the_minimal_length_before_the_start() {
    let p = project();
    let plan = draw(&p, pt(0.0, 60.0), pt(2.0, 62.0), false, false).unwrap();
    let run = |sec| with_ctx(&p, &[4, 3], false, |ctx| plan_slide_end(&plan.creation, sec, (None, None), ctx));
    let (c, snapped) = run(sec(4.0));
    let end = TimePos::at(2, 0.0);
    assert_eq!((c.notes[1].start, c.notes[1].end, snapped.1), (at(2.0), end, Some(end)));
    assert_eq!(c.notes[0], plan.creation.notes[0]);
    assert_eq!(run(sec(1.0)).0, plan.creation);
}

#[test]
fn intervals_use_bend_vocabulary() {
    assert_eq!(interval_label(2), "+1 (full)");
    assert_eq!(interval_label(1), "+½");
    assert_eq!(interval_label(-2), "-1");
    assert_eq!(interval_label(-1), "-½");
    assert_eq!(interval_label(3), "+1½");
    assert_eq!(interval_label(4), "+2");
}

#[test]
fn feedback_labels_ends_and_warns_without_a_same_string_placement() {
    let mut p = project();
    let _ = add(&mut p, 50, 0.0, 1.0);
    let ok = draw(&p, pt(2.0, 64.0), pt(3.0, 66.0), false, false).unwrap().creation;
    let tempo = TempoMap::new(&p.measures);
    let fb = feedback(&ok, &p, &tempo);
    assert_eq!(fb.labels.iter().map(|l| l.2.as_str()).collect::<Vec<_>>(), ["e 0", "e 2"]);
    assert_eq!(fb.interval.as_deref(), Some("+1 (full)"));
    assert_eq!(fb.warning, None);
    let bad = draw(&p, pt(2.0, 40.0), pt(3.0, 65.0), false, false).unwrap().creation;
    assert_eq!(feedback(&bad, &p, &tempo).warning.as_deref(), Some("No same-string placement"));
}

#[test]
fn ghost_links_new_notes_by_index_and_repeats_an_existing_source() {
    let mut p = project();
    let s = add(&mut p, 60, 0.0, 1.0);
    let tempo = TempoMap::new(&p.measures);
    let plan = draw(&p, pt(2.0, 60.0), pt(3.0, 62.0), false, false).unwrap();
    let ghost = ghost_of(&plan.creation, &p, &tempo);
    assert_eq!(ghost.glyphs.len(), 2);
    assert_eq!(ghost.glyphs[0].link_to, Some((1, LinkKind::Slide)));
    let d = with_ctx(&p, &[4, 3], false, |ctx| plan_extend(&start(&p, s, pt(0.8, 62.0)), None, (None, None), ctx));
    let ghost = ghost_of(&d.creation, &p, &tempo);
    assert_eq!(ghost.glyphs.len(), 2);
    assert_eq!(ghost.glyphs[0].legato_from, Some(1));
}
