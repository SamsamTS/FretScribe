use egui::{pos2, vec2, Context, CentralPanel, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use fretscribe_core::{next_note_id, LinkType, Note, NoteTimes, Project, TempoMap, TimePos, TimeSignature, Track, TrackRole, Tuning};
use fretscribe_notation::tab::{assign_tabs, TabOptions};

use super::*;
use crate::view::TimelineView;

fn project() -> Project {
    Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 20.0, 120.0, TimeSignature::COMMON)
}

fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
    let id = next_note_id(p);
    p.notes.push(Note::new(id, midi, TimePos::new(1, start), TimePos::new(1, end)));
    id
}

fn solved(midis: &[u8]) -> Project {
    let mut p = project();
    for (i, &m) in midis.iter().enumerate() {
        add(&mut p, m, i as f64, i as f64 + 0.9);
    }
    solve(&mut p);
    p
}

fn solve(p: &mut Project) {
    let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
    assign_tabs(&mut p.notes, &times, &p.tuning, p.capo, &TabOptions::default());
}

fn string(p: &Project, id: u64) -> Option<u8> {
    p.note(id).and_then(|n| n.string_fret).map(|s| s.0)
}

fn set(ids: &[u64], string: u8) -> TabEdit {
    TabEdit::SetString { ids: ids.to_vec(), string }
}

#[test]
fn geometry_maps_strings_and_time() {
    let g = Geo { left: 100.0, top: 50.0, gap: 10.0, strings: 6 };
    assert_eq!((g.y(5), g.y(0)), (50.0, 100.0));
    for s in 0..6 {
        assert_eq!(g.nearest_string(g.y(s) + 3.0), s);
    }
    assert_eq!((g.nearest_string(-1000.0), g.nearest_string(1000.0)), (5, 0));
    let view = TimelineView::new(2.0, 100.0);
    assert!((g.sec(&view, g.x(&view, 3.5)) - 3.5).abs() < 1e-4);
    assert_eq!(g.x(&view, 2.0), 100.0);
}

#[test]
fn a_manual_choice_locks_and_moves_the_following_automatic_notes_in_one_undo_step() {
    let mut p = solved(&[64, 64, 64, 64]);
    assert!(p.notes.iter().all(|n| n.string_fret.is_some_and(|s| s.0 == 5)));
    let before = p.clone();
    let applied = apply(&mut p, &set(&[2], 3), &TabOptions::default(), false).unwrap();
    assert_eq!((p.note(2).unwrap().string_fret, p.note(2).unwrap().locked_fret), (Some((3, 9)), true));
    assert_eq!(string(&p, 1), Some(5));
    assert!(applied.changed.iter().all(|id| *id != 2));
    assert!(!applied.changed.is_empty() && applied.changed.iter().all(|&id| string(&p, id) == Some(3)));
    applied.edit.undo(&mut p);
    assert_eq!(p, before);
    applied.edit.redo(&mut p);
    assert_ne!(p, before);
}

#[test]
fn propagating_backwards_is_optional() {
    let mut p = solved(&[64, 64, 64]);
    let mut q = p.clone();
    apply(&mut p, &set(&[3], 3), &TabOptions::default(), false).unwrap();
    apply(&mut q, &set(&[3], 3), &TabOptions::default(), true).unwrap();
    assert_eq!(string(&p, 1), Some(5));
    assert_eq!(string(&q, 1), Some(3));
}

#[test]
fn a_linked_group_moves_as_one_and_only_the_clicked_note_locks() {
    let mut p = project();
    let ids = [add(&mut p, 64, 0.0, 1.0), add(&mut p, 66, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0)];
    assert!(p.link_notes(ids[0], ids[1], LinkType::Bend).is_ok() && p.link_notes(ids[1], ids[2], LinkType::Bend).is_ok());
    solve(&mut p);
    apply(&mut p, &set(&[ids[1]], 3), &TabOptions::default(), false).unwrap();
    let frets: Vec<_> = ids.iter().map(|&i| p.note(i).unwrap().string_fret).collect();
    assert_eq!(frets, vec![Some((3, 9)); 3]);
    assert_eq!(ids.map(|i| p.note(i).unwrap().locked_fret), [false, true, false]);
    let options = placements(&p, &TabOptions::default(), ids[1]);
    assert!(options.iter().all(|&(_, f)| f <= 22) && options.len() >= 3);
}

#[test]
fn impossible_moves_change_nothing() {
    let mut p = solved(&[40, 64]);
    let before = p.clone();
    assert_eq!(apply(&mut p, &set(&[1], 5), &TabOptions::default(), false).unwrap_err(), TabError::NoPlacement);
    assert_eq!(apply(&mut p, &TabEdit::MoveString { ids: vec![1], up: false }, &TabOptions::default(), false).unwrap_err(), TabError::NoPlacement);
    assert_eq!(apply(&mut p, &set(&[], 1), &TabOptions::default(), false).unwrap_err(), TabError::Empty);
    assert_eq!(p, before);
}

#[test]
fn adjacent_string_cycling_lock_and_reset() {
    let mut p = solved(&[64]);
    let opts = TabOptions::default();
    apply(&mut p, &TabEdit::MoveString { ids: vec![1], up: false }, &opts, false).unwrap();
    assert_eq!((string(&p, 1), p.note(1).unwrap().locked_fret), (Some(4), true));
    apply(&mut p, &TabEdit::MoveString { ids: vec![1], up: true }, &opts, false).unwrap();
    assert_eq!(string(&p, 1), Some(5));
    let strings: Vec<u8> = (0..6)
        .map(|_| {
            apply(&mut p, &TabEdit::Cycle { ids: vec![1], back: false }, &opts, false).unwrap();
            string(&p, 1).unwrap()
        })
        .collect();
    assert_eq!(strings, [4, 3, 2, 1, 5, 4], "next placement in order of fret, wrapping");
    apply(&mut p, &TabEdit::Cycle { ids: vec![1], back: true }, &opts, false).unwrap();
    assert_eq!(string(&p, 1), Some(5));
    apply(&mut p, &TabEdit::Reset { ids: vec![1] }, &opts, false).unwrap();
    assert_eq!((string(&p, 1), p.note(1).unwrap().locked_fret), (Some(5), false));
    apply(&mut p, &TabEdit::Lock { ids: vec![1] }, &opts, false).unwrap();
    assert!(p.note(1).unwrap().locked_fret);
}

#[test]
fn multi_selection_moves_what_it_can() {
    let mut p = solved(&[64, 45, 66]);
    apply(&mut p, &set(&[1, 2, 3], 1), &TabOptions::default(), false).unwrap();
    assert_eq!([1, 2, 3].map(|i| string(&p, i)), [Some(1), Some(1), Some(1)]);
    let mut q = solved(&[64, 40]);
    apply(&mut q, &set(&[1, 2], 0), &TabOptions::default(), false).unwrap();
    assert_eq!((string(&q, 1), string(&q, 2)), (Some(5), Some(0)), "the note that cannot go there is skipped");
}

#[test]
fn technique_menu_edits_and_legato_from_previous() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0));
    solve(&mut p);
    let opts = TabOptions::default();
    let applied = apply(&mut p, &TabEdit::Legato { ids: vec![b] }, &opts, false).unwrap();
    assert_eq!(p.outgoing(a).map(|l| l.1), Some(LinkType::Legato));
    apply(&mut p, &TabEdit::SetLink { id: a, kind: Some(LinkType::Slide) }, &opts, false).unwrap();
    assert_eq!(p.outgoing(a).map(|l| l.1), Some(LinkType::Slide));
    apply(&mut p, &TabEdit::SetLink { id: a, kind: None }, &opts, false).unwrap();
    assert_eq!(p.outgoing(a), None);
    assert_eq!(apply(&mut p, &TabEdit::SetLink { id: a, kind: None }, &opts, false).unwrap_err(), TabError::NoPlacement);
    applied.edit.undo(&mut p);
}

#[test]
fn tuning_changes_are_undoable_and_keep_locks_that_still_play() {
    let mut p = solved(&[64, 52]);
    let opts = TabOptions::default();
    apply(&mut p, &set(&[2], 1), &opts, false).unwrap();
    let before = p.clone();
    let drop_d = Tuning { name: "Drop D".into(), open_strings: vec![38, 45, 50, 55, 59, 64] };
    let edit = apply_tuning(&mut p, drop_d.clone(), 2, &opts);
    assert_eq!((p.tuning.clone(), p.capo), (drop_d, 2));
    assert_eq!((p.note(2).unwrap().string_fret, p.note(2).unwrap().locked_fret), (Some((1, 7)), true));
    let short = apply_tuning(&mut p, Tuning { name: "Bass".into(), open_strings: vec![28, 33, 38, 43] }, 0, &opts);
    assert!(!p.note(2).unwrap().locked_fret || string(&p, 2).is_some_and(|s| s < 4));
    short.undo(&mut p);
    edit.undo(&mut p);
    assert_eq!(p, before);
}

#[test]
fn menu_lists_placements_with_labels_auto_and_the_selection_rule() {
    let p = solved(&[64]);
    let opts = TabOptions::default();
    let m = menu_model(&p, &[], 1, &opts).unwrap();
    let labels: Vec<&str> = m.placements.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["e 0", "B 5", "G 9", "D 14", "A 19"]);
    assert_eq!(m.placements.iter().filter(|p| p.auto).map(|p| p.string).collect::<Vec<_>>(), [5]);
    assert_eq!(m.current, Some((5, 0)));
    let mut q = solved(&[64, 67]);
    q.notes[1].string_fret = Some((5, 3));
    let many = menu_model(&q, &[1, 2], 1, &opts).unwrap();
    assert!(many.placements.is_empty() && many.strings.contains(&5) && many.ids == [1, 2]);
    assert_eq!(menu_model(&q, &[2], 1, &opts).unwrap().ids, [1], "an unselected note acts alone");
    assert!(menu_model(&q, &[], 99, &opts).is_none());
}

struct Harness {
    ctx: Context,
    frame: u32,
}

impl Harness {
    fn new() -> Self {
        Self { ctx: Context::default(), frame: 0 }
    }

    fn run(&mut self, p: &Project, selected: &[u64], state: &mut TabViewState, events: Vec<Event>) -> (Vec<TabViewEvent>, Rect) {
        self.frame += 1;
        let modifiers = events.iter().rev().find_map(|e| match e {
            Event::PointerButton { modifiers, .. } | Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        });
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 400.0))), modifiers: modifiers.unwrap_or_default(), events, time: Some(f64::from(self.frame) * 0.02), ..Default::default() };
        let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
        let (opts, style) = (TabOptions::default(), TabStyle::default());
        let mut out = (Vec::new(), Rect::NOTHING);
        let _ = self.ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let o = TabView { project: p, times: &times, staff: None, selected, playhead: Some(0.3), style: &style, opts: &opts, timeline_offset: 60.0 }.show(ui, &TimelineView::new(0.0, 200.0), state);
                out = (o.events, o.response.rect);
            });
        });
        out
    }

    fn press(&mut self, pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> Vec<Event> {
        vec![Event::PointerMoved(pos), Event::PointerButton { pos, button, pressed, modifiers }]
    }
}

fn position(rect: Rect, string: u8, sec: f64, strings: usize) -> Pos2 {
    let g = Geo { left: rect.left() + 60.0, top: rect.top() + TabStyle::default().top_margin(), gap: TabStyle::default().gap, strings };
    pos2(g.x(&TimelineView::new(0.0, 200.0), sec), g.y(string))
}

fn settle(h: &mut Harness, p: &Project, state: &mut TabViewState) -> Rect {
    let rect = h.run(p, &[], state, vec![]).1;
    h.run(p, &[], state, vec![]);
    rect
}

#[test]
fn clicks_select_seek_and_cycle() {
    let p = solved(&[64]);
    let (mut h, mut state) = (Harness::new(), TabViewState::default());
    let rect = settle(&mut h, &p, &mut state);
    let note = position(rect, 5, 0.0, 6);
    let on = h.press(note, PointerButton::Primary, true, Modifiers::NONE);
    h.run(&p, &[], &mut state, on);
    let off = h.press(note, PointerButton::Primary, false, Modifiers::NONE);
    let (events, _) = h.run(&p, &[], &mut state, off);
    assert_eq!(events, vec![TabViewEvent::NoteClicked { id: 1, ctrl: false, shift: false }]);
    assert_eq!(state.menu.as_ref().map(|m| m.placements.len()), Some(5), "a plain left click opens the menu");
    assert!(state.menu_open());

    let empty = pos2(note.x + 300.0, note.y);
    h.run(&p, &[], &mut state, vec![Event::PointerMoved(empty)]);
    let on = h.press(empty, PointerButton::Primary, true, Modifiers::NONE);
    h.run(&p, &[], &mut state, on);
    assert!(state.menu.is_none(), "a press outside closes the menu");
    let off = h.press(empty, PointerButton::Primary, false, Modifiers::NONE);
    let (events, _) = h.run(&p, &[], &mut state, off);
    assert!(matches!(events.as_slice(), [TabViewEvent::SeekRequested(s)] if (*s - 1.5).abs() < 0.01), "{events:?}");

    let alt = Modifiers::ALT;
    let on = h.press(note, PointerButton::Primary, true, alt);
    h.run(&p, &[], &mut state, on);
    let off = h.press(note, PointerButton::Primary, false, alt);
    let (events, _) = h.run(&p, &[], &mut state, off);
    assert!(events.contains(&TabViewEvent::Edit(TabEdit::Cycle { ids: vec![1], back: false })), "{events:?}");
}

#[test]
fn dragging_a_number_to_another_string_edits_only_where_the_pitch_reaches() {
    let p = solved(&[64]);
    let (mut h, mut state) = (Harness::new(), TabViewState::default());
    let rect = settle(&mut h, &p, &mut state);
    let from = position(rect, 5, 0.0, 6);
    for target in [4u8, 0] {
        let on = h.press(from, PointerButton::Primary, true, Modifiers::NONE);
        h.run(&p, &[], &mut state, on);
        h.run(&p, &[], &mut state, vec![Event::PointerMoved(from + vec2(0.0, 10.0))]);
        let to = position(rect, target, 0.0, 6);
        h.run(&p, &[], &mut state, vec![Event::PointerMoved(to)]);
        let off = vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }];
        let (events, _) = h.run(&p, &[], &mut state, off);
        let sent = events.contains(&TabViewEvent::Edit(TabEdit::SetString { ids: vec![1], string: target }));
        assert_eq!(sent, target == 4, "string {target}: {events:?}");
    }
}

fn click(h: &mut Harness, p: &Project, selected: &[u64], state: &mut TabViewState, at: Pos2, button: PointerButton, mods: Modifiers) -> Vec<TabViewEvent> {
    let on = h.press(at, button, true, mods);
    h.run(p, selected, state, on);
    let off = h.press(at, button, false, mods);
    h.run(p, selected, state, off).0
}

#[test]
fn right_button_follows_the_note_editor_scheme() {
    let p = solved(&[64, 67]);
    let (mut h, mut state) = (Harness::new(), TabViewState::default());
    let rect = settle(&mut h, &p, &mut state);
    let note = position(rect, 5, 0.0, 6);
    let empty = pos2(note.x + 300.0, note.y);
    assert_eq!(click(&mut h, &p, &[], &mut state, note, PointerButton::Secondary, Modifiers::NONE), vec![TabViewEvent::NoteContext(1)]);
    assert_eq!(click(&mut h, &p, &[1], &mut state, empty, PointerButton::Secondary, Modifiers::NONE), vec![TabViewEvent::ClearSelection]);
    assert!(state.menu.is_none(), "the menu is on the left button");

    for (mods, add) in [(Modifiers::NONE, false), (Modifiers::COMMAND, true)] {
        let from = note - vec2(30.0, 40.0);
        let to = position(rect, 0, 0.7, 6) + vec2(0.0, 12.0);
        let on = h.press(from, PointerButton::Secondary, true, mods);
        h.run(&p, &[], &mut state, on);
        h.run(&p, &[], &mut state, vec![Event::PointerMoved(from + vec2(5.0, 10.0))]);
        h.run(&p, &[], &mut state, vec![Event::PointerMoved(to)]);
        let off = vec![Event::PointerButton { pos: to, button: PointerButton::Secondary, pressed: false, modifiers: mods }];
        let (events, _) = h.run(&p, &[], &mut state, off);
        assert_eq!(events, vec![TabViewEvent::Marquee { ids: vec![1, 2], add }], "{mods:?}");
    }
}

#[test]
fn holding_the_left_button_shows_the_string_colours_after_a_delay_without_moving() {
    let p = solved(&[64]);
    let (mut h, mut state) = (Harness::new(), TabViewState::default());
    let rect = settle(&mut h, &p, &mut state);
    let note = position(rect, 5, 0.0, 6);
    let on = h.press(note, PointerButton::Primary, true, Modifiers::NONE);
    h.run(&p, &[], &mut state, on);
    assert!(!state.holding(h.frame as f64 * 0.02), "a click does not flash the colours");
    for _ in 0..10 {
        h.run(&p, &[], &mut state, vec![]);
    }
    assert!(state.holding(h.frame as f64 * 0.02));
    let off = h.press(note, PointerButton::Primary, false, Modifiers::NONE);
    h.run(&p, &[], &mut state, off);
    assert!(state.drag.is_none());
}

#[test]
fn keys_edit_the_selection_while_hovering() {
    let p = solved(&[64]);
    let (mut h, mut state) = (Harness::new(), TabViewState::default());
    let rect = settle(&mut h, &p, &mut state);
    let note = position(rect, 5, 0.0, 6);
    h.run(&p, &[1], &mut state, vec![Event::PointerMoved(note)]);
    let key = |key, modifiers| vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }];
    let (events, _) = h.run(&p, &[1], &mut state, key(Key::ArrowDown, Modifiers::NONE));
    assert_eq!(events, vec![TabViewEvent::Edit(TabEdit::MoveString { ids: vec![1], up: false })]);
    let (events, _) = h.run(&p, &[1], &mut state, key(Key::Tab, Modifiers::SHIFT));
    assert_eq!(events, vec![TabViewEvent::Edit(TabEdit::Cycle { ids: vec![1], back: true })]);
}

#[test]
fn draws_every_technique_unassigned_notes_and_the_stems_without_panicking() {
    use fretscribe_core::{GraceKind, GraceNote, Legato, LegatoLink};
    use fretscribe_notation::staff::{layout, StaffOptions};
    let mut p = project();
    let ids: Vec<u64> = (0..10).map(|i| add(&mut p, 60 + i as u8, f64::from(i) * 0.5, f64::from(i) * 0.5 + 0.5)).collect();
    p.link_notes(ids[0], ids[1], LinkType::Slide).unwrap();
    p.link_notes(ids[2], ids[3], LinkType::Bend).unwrap();
    p.link_notes(ids[3], ids[4], LinkType::Bend).unwrap();
    p.notes[6].legato = Some(LegatoLink { kind: Legato::HammerOn, from: ids[5] });
    p.notes[8].grace_of = Some(GraceNote { main: ids[9], kind: GraceKind::Acciaccatura });
    add(&mut p, 10, 6.0, 6.5);
    solve(&mut p);
    let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
    let staff = layout(&p.notes, &p.measures, &StaffOptions::default());
    let mut state = TabViewState { show_stems: true, string_colors: true, ..Default::default() };
    state.flash(&[1, 2], 0.0);
    let ctx = Context::default();
    let (opts, style) = (TabOptions::default(), TabStyle::default());
    for _ in 0..2 {
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 400.0))), ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                TabView { project: &p, times: &times, staff: Some(&staff), selected: &[3], playhead: None, style: &style, opts: &opts, timeline_offset: 60.0 }.show(ui, &TimelineView::new(0.0, 150.0), &mut state);
            });
        });
    }
}
