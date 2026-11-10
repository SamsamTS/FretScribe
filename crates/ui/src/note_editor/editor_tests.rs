use egui::pos2;
use fretscribe_core::{LinkType, Project, SnapGrid, TimePos};

use super::testkit::*;
use super::*;
use crate::spectro::{HitZone, SpectroEvent};

struct H {
    p: Project,
    ed: NoteEditor,
    grid: SnapGrid,
    now: f64,
    hovered: Option<u64>,
}

type Hit = Option<(u64, HitZone)>;

fn s(beat: f64, midi: f32, hit: Hit) -> Sample {
    let sec = sec(beat);
    Sample { pos: pos2((sec * 200.0) as f32, -midi * 10.0), sec, midi, refined: midi, hit }
}

fn body(id: u64) -> Hit {
    Some((id, HitZone::Body))
}

const NONE: Mods = Mods { shift: false, ctrl: false, alt: false };
const SHIFT: Mods = Mods { shift: true, ctrl: false, alt: false };
const CTRL: Mods = Mods { shift: false, ctrl: true, alt: false };
const ALT: Mods = Mods { shift: false, ctrl: false, alt: true };

impl H {
    fn new() -> Self {
        Self { p: project(), ed: NoteEditor::default(), grid: SnapGrid::new(&[4, 3]), now: 0.0, hovered: None }
    }

    fn send(&mut self, input: Input) -> Vec<Effect> {
        let mut env = Env { project: &mut self.p, grid: &self.grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, onsets: None, now: self.now, hovered: self.hovered };
        self.ed.handle(&mut env, input)
    }

    fn press(&mut self, button: Button, at: Sample, mods: Mods) -> Vec<Effect> {
        self.send(Input::Press { button, at, mods })
    }

    fn mv(&mut self, at: Sample, mods: Mods) -> Vec<Effect> {
        self.send(Input::Move { at, mods })
    }

    fn rel(&mut self, button: Button, at: Sample, mods: Mods) -> Vec<Effect> {
        self.send(Input::Release { button, at, mods })
    }

    fn drag(&mut self, button: Button, from: Sample, to: Sample, mods: Mods) -> Vec<Effect> {
        self.press(button, from, mods);
        self.mv(to, mods);
        self.rel(button, to, mods)
    }

    fn click(&mut self, button: Button, at: Sample, mods: Mods) -> Vec<Effect> {
        self.press(button, at, mods);
        self.rel(button, at, mods)
    }

    fn draw(&mut self, press: (f64, f32), end: (f64, f32), mods: Mods) -> Vec<Effect> {
        let ev = |f: fn(u8, f32, f64) -> SpectroEvent, p: (f64, f32)| Input::Spectro(f(p.1.round() as u8, p.1, sec(p.0)), mods);
        self.send(ev(|midi, exact_midi_float, time_sec| SpectroEvent::DrawBegan { midi, exact_midi_float, time_sec }, press));
        self.send(ev(|midi, exact_midi_float, time_sec| SpectroEvent::DrawMoved { midi, exact_midi_float, time_sec }, end));
        self.send(ev(|midi, exact_midi_float, time_sec| SpectroEvent::DrawEnded { midi, exact_midi_float, time_sec }, end))
    }
}

fn edits(effects: &[Effect]) -> usize {
    effects.iter().filter(|e| matches!(e, Effect::Edit { .. })).count()
}

fn message(effects: &[Effect]) -> Option<&str> {
    effects.iter().find_map(|e| if let Effect::Message(m) = e { Some(m.as_str()) } else { None })
}

#[test]
fn a_click_on_empty_space_seeks_and_creates_nothing() {
    let mut h = H::new();
    let out = h.send(Input::Spectro(SpectroEvent::SeekRequested(2.5), NONE));
    assert!(matches!(out.as_slice(), [Effect::Seek(t)] if *t == 2.5));
    assert!(h.p.notes.is_empty());
    assert!(h.click(Button::Left, s(1.0, 60.0, None), NONE).is_empty());
}

#[test]
fn drawing_left_to_right_and_right_to_left_gives_the_same_note_in_one_undo_step() {
    let mut a = H::new();
    let out = a.draw((1.0, 64.0), (2.02, 64.0), NONE);
    assert_eq!(edits(&out), 1);
    let mut b = H::new();
    b.draw((2.02, 64.0), (1.0, 64.1), NONE);
    assert_eq!(a.p.notes, b.p.notes);
    assert_eq!(a.p.notes.len(), 1);
    assert_eq!(a.ed.selection.ids, [a.p.notes[0].id]);
    let Some(Effect::Edit { edit, .. }) = out.into_iter().next() else { panic!("edit") };
    edit.undo(&mut a.p);
    assert!(a.p.notes.is_empty());
}

#[test]
fn a_different_pitch_makes_two_linked_notes_and_a_tiny_span_nothing() {
    let mut h = H::new();
    assert_eq!(edits(&h.draw((1.0, 60.0), (3.0, 62.0), NONE)), 0);
    assert!(h.p.notes.is_empty() && h.ed.in_mode() && h.ed.ghost.is_some(), "the slide waits for its end");
    h.mv(s(4.5, 62.0, None), NONE);
    assert_eq!(edits(&h.click(Button::Left, s(4.5, 62.0, None), NONE)), 1);
    assert_eq!(h.p.notes.len(), 2);
    assert_eq!((h.p.notes[1].start.beat, h.p.notes[1].end.beat), (3.0, 0.5));
    assert_eq!(h.p.outgoing(h.p.notes[0].id), Some((h.p.notes[1].id, LinkType::Slide)));
    assert_eq!(h.ed.selection.ids.len(), 2);
    assert!(h.send(Input::Spectro(SpectroEvent::SeekRequested(2.0), NONE)).is_empty());
    let mut h = H::new();
    h.draw((1.0, 60.0), (3.0, 62.0), NONE);
    h.send(Input::Escape);
    assert!(!h.ed.in_mode() && h.ed.ghost.is_none() && h.p.notes.is_empty());
    h.draw((1.0, 60.0), (3.0, 62.0), NONE);
    h.click(Button::Right, s(4.0, 62.0, None), NONE);
    assert!(!h.ed.in_mode() && h.p.notes.is_empty());
    let mut h = H::new();
    assert_eq!(edits(&h.draw((1.0, 60.0), (1.04, 60.0), NONE)), 0);
    assert!(h.p.notes.is_empty());
}

#[test]
fn alt_disables_snapping_while_drawing() {
    let mut h = H::new();
    h.draw((1.03, 60.0), (2.27, 60.0), ALT);
    assert!((h.p.notes[0].start.beat - 1.03).abs() < 1e-9);
    assert!((h.p.notes[0].end.beat - 2.27).abs() < 1e-9);
}

fn two_notes() -> (H, u64, u64) {
    let mut h = H::new();
    let (a, b) = (add(&mut h.p, 60, 0.0, 1.0), add(&mut h.p, 62, 2.0, 3.0));
    (h, a, b)
}

#[test]
fn escape_and_the_opposite_button_cancel_every_drag_and_swallow_both_releases() {
    type Start = fn(&mut H, u64, u64);
    let starts: [(&str, Button, Start); 6] = [
        ("move", Button::Left, |h, a, _| {
            h.press(Button::Left, s(0.5, 60.0, body(a)), NONE);
            h.mv(s(3.5, 62.0, None), NONE);
        }),
        ("resize", Button::Left, |h, a, _| {
            h.press(Button::Left, s(1.0, 60.0, Some((a, HitZone::RightEdge))), NONE);
            h.mv(s(2.0, 60.0, None), NONE);
        }),
        ("chain", Button::Left, |h, a, _| {
            h.press(Button::Left, s(0.5, 60.0, body(a)), SHIFT);
            h.mv(s(4.0, 65.0, None), SHIFT);
        }),
        ("legato", Button::Right, |h, a, _| {
            h.press(Button::Right, s(0.5, 60.0, body(a)), NONE);
            h.mv(s(1.5, 62.0, None), NONE);
        }),
        ("marquee", Button::Right, |h, _, _| {
            h.press(Button::Right, s(0.0, 70.0, None), NONE);
            h.mv(s(1.5, 50.0, None), NONE);
        }),
        ("draw", Button::Left, |h, _, _| {
            h.send(Input::Spectro(SpectroEvent::DrawBegan { midi: 60, exact_midi_float: 60.0, time_sec: 3.0 }, NONE));
            h.send(Input::Spectro(SpectroEvent::DrawMoved { midi: 60, exact_midi_float: 60.0, time_sec: 4.0 }, NONE));
        }),
    ];
    for (name, button, start) in starts {
        for use_escape in [true, false] {
            let (mut h, a, b) = two_notes();
            h.ed.selection.set(vec![b]);
            let before = h.p.notes.clone();
            start(&mut h, a, b);
            assert!(h.ed.is_busy(), "{name} starts");
            let other = if button == Button::Left { Button::Right } else { Button::Left };
            let mut effects = if use_escape { h.send(Input::Escape) } else { h.press(other, s(5.0, 60.0, None), NONE) };
            assert!(!h.ed.is_busy(), "{name} cancelled");
            effects.extend(h.send(Input::Spectro(SpectroEvent::DrawEnded { midi: 60, exact_midi_float: 60.0, time_sec: 4.0 }, NONE)));
            effects.extend(h.rel(button, s(5.0, 60.0, body(a)), NONE));
            if !use_escape {
                effects.extend(h.rel(other, s(5.0, 60.0, None), NONE));
            }
            assert_eq!(edits(&effects), 0, "{name} swallowed");
            assert_eq!(h.p.notes, before, "{name} untouched");
            assert_eq!(h.ed.selection.ids, if name == "move" { vec![a] } else { vec![b] }.into_iter().collect::<Vec<_>>().as_slice().to_vec().into_iter().collect::<Vec<_>>(), "{name} selection");
            h.click(Button::Left, s(0.5, 60.0, body(a)), NONE);
            assert_eq!(h.ed.selection.ids, [a], "{name}: the next click works again");
        }
    }
}

#[test]
fn escape_with_nothing_in_progress_clears_the_selection() {
    let (mut h, a, _) = two_notes();
    h.ed.selection.set(vec![a]);
    h.send(Input::Escape);
    assert!(h.ed.selection.ids.is_empty());
}

#[test]
fn shift_drag_from_the_last_note_extends_the_chain_and_moves_nothing_else() {
    let (mut h, a, b) = two_notes();
    let before = h.p.notes.clone();
    let out = h.drag(Button::Left, s(0.5, 60.0, body(a)), s(3.0, 64.0, None), SHIFT);
    assert_eq!(edits(&out), 1);
    assert_eq!(h.p.notes.len(), 3);
    let shape = |n: &fretscribe_core::Note| (n.id, n.midi, n.start, n.end);
    assert_eq!(shape(&h.p.notes[1]), shape(&before[1]));
    assert_eq!((h.p.notes[0].midi, h.p.notes[0].start), (before[0].midi, before[0].start));
    let new = h.p.notes[2].id;
    assert_eq!(h.p.notes[0].end, h.p.notes[2].start, "the source ends where the new note starts");
    assert_eq!(h.p.outgoing(a), Some((new, LinkType::Slide)));
    assert_eq!(h.p.outgoing(b), None);
    assert_eq!(h.ed.selection.ids, [new]);
}

#[test]
fn shift_drag_from_a_note_that_already_starts_a_link_moves_it_instead() {
    let (mut h, a, b) = two_notes();
    h.p.link_notes(a, b, LinkType::Slide).unwrap();
    h.drag(Button::Left, s(0.5, 60.0, body(a)), s(1.5, 60.0, None), SHIFT);
    assert_eq!(h.p.notes.len(), 2);
    assert_eq!(h.p.note(a).unwrap().start, at(1.0));
}

#[test]
fn right_click_cycles_the_outgoing_link_and_coalesces_within_a_second() {
    let (mut h, a, b) = two_notes();
    assert!(h.click(Button::Right, s(0.5, 60.0, body(a)), NONE).is_empty(), "no link, nothing happens");
    h.p.link_notes(a, b, LinkType::Slide).unwrap();
    let first = h.click(Button::Right, s(0.5, 60.0, body(a)), NONE);
    assert_eq!(message(&first), Some("Bend"));
    assert!(matches!(first[0], Effect::Edit { merge: false, .. }));
    h.now = 0.6;
    let second = h.click(Button::Right, s(0.5, 60.0, body(a)), NONE);
    assert_eq!(message(&second), Some("Legato"), "linking tied the notes: legato is valid");
    assert!(matches!(second[0], Effect::Edit { merge: true, .. }));
    h.now = 3.0;
    let third = h.click(Button::Right, s(0.5, 60.0, body(a)), NONE);
    assert!(matches!(third[0], Effect::Edit { merge: false, .. }));
    assert_eq!(message(&third), Some("Slide"));
    assert_eq!(h.p.outgoing(a), Some((b, LinkType::Slide)));
}

#[test]
fn right_click_on_a_link_removes_only_the_link() {
    let (mut h, a, b) = two_notes();
    h.p.link_notes(a, b, LinkType::Slide).unwrap();
    let out = h.click(Button::Right, s(1.5, 61.0, Some((a, HitZone::Line))), NONE);
    assert_eq!(edits(&out), 1);
    assert_eq!((h.p.notes.len(), h.p.outgoing(a)), (2, None));
    let (c, d) = (add(&mut h.p, 60, 5.0, 6.0), add(&mut h.p, 62, 6.0, 7.0));
    h.p.link_notes(c, d, LinkType::Legato).unwrap();
    h.click(Button::Right, s(6.0, 61.0, Some((d, HitZone::Arc))), NONE);
    assert_eq!((h.p.notes.len(), h.p.outgoing(c)), (4, None));
}

#[test]
fn right_click_on_empty_space_clears_the_selection() {
    let (mut h, a, _) = two_notes();
    h.ed.selection.set(vec![a]);
    h.click(Button::Right, s(5.0, 70.0, None), NONE);
    assert!(h.ed.selection.ids.is_empty());
}

#[test]
fn right_drag_onto_any_valid_note_links_without_a_new_note() {
    let mut h = H::new();
    let (a, b, c) = (add(&mut h.p, 60, 0.0, 1.0), add(&mut h.p, 62, 1.05, 2.0), add(&mut h.p, 64, 3.0, 4.0));
    h.press(Button::Right, s(0.5, 60.0, body(a)), NONE);
    h.mv(s(1.5, 62.0, body(b)), NONE);
    assert!(h.ed.ghost.is_some());
    assert_eq!(edits(&h.rel(Button::Right, s(1.5, 62.0, body(b)), NONE)), 1);
    assert_eq!((h.p.notes.len(), h.p.outgoing(a)), (3, Some((b, LinkType::Legato))));
    let out = h.drag(Button::Right, s(1.5, 62.0, body(b)), s(3.5, 64.0, body(c)), NONE);
    assert_eq!((edits(&out), h.p.notes.len(), h.p.outgoing(b)), (1, 3, Some((c, LinkType::Slide))));
}

#[test]
fn right_drag_over_an_invalid_note_does_nothing() {
    let mut h = H::new();
    let (a, b) = (add(&mut h.p, 60, 0.0, 1.0), add(&mut h.p, 60, 2.0, 3.0));
    let out = h.drag(Button::Right, s(0.5, 60.0, body(a)), s(2.5, 60.0, body(b)), NONE);
    assert_eq!((edits(&out), h.p.notes.len()), (0, 2));
}

#[test]
fn a_left_click_cancelling_a_right_drag_does_not_seek() {
    let mut h = H::new();
    let a = add(&mut h.p, 60, 2.0, 3.0);
    h.press(Button::Right, s(2.5, 60.0, body(a)), NONE);
    h.mv(s(4.0, 62.0, None), NONE);
    h.press(Button::Left, s(4.0, 62.0, None), NONE);
    h.rel(Button::Left, s(4.0, 62.0, None), NONE);
    let out = h.send(Input::Spectro(SpectroEvent::SeekRequested(4.0), NONE));
    assert!(out.is_empty() && h.ed.ghost.is_none());
    h.rel(Button::Right, s(4.0, 62.0, None), NONE);
    assert!(h.p.outgoing(a).is_none());
    h.press(Button::Left, s(5.0, 62.0, None), NONE);
    h.rel(Button::Left, s(5.0, 62.0, None), NONE);
    assert!(matches!(h.send(Input::Spectro(SpectroEvent::SeekRequested(5.0), NONE))[..], [Effect::Seek(_)]));
}

#[test]
fn ctrl_right_drag_from_a_note_is_a_marquee_that_adds() {
    let (mut h, a, b) = two_notes();
    h.ed.selection.set(vec![b]);
    let out = h.drag(Button::Right, s(2.5, 62.0, body(b)), s(0.2, 58.0, None), CTRL);
    assert_eq!(edits(&out), 0);
    assert_eq!(h.p.notes.len(), 2);
    assert_eq!(h.ed.selection.ids, [b, a]);
}

#[test]
fn plain_right_drag_from_empty_space_is_a_marquee_that_replaces() {
    let (mut h, a, b) = two_notes();
    h.ed.selection.set(vec![b]);
    h.press(Button::Right, s(0.0, 70.0, None), NONE);
    h.mv(s(0.8, 55.0, None), NONE);
    assert!(h.ed.marquee.is_some());
    h.rel(Button::Right, s(0.8, 55.0, None), NONE);
    assert_eq!(h.ed.selection.ids, [a]);
    assert!(h.ed.marquee.is_none());
}

#[test]
fn clicks_select_toggle_and_extend_in_time_order() {
    let mut h = H::new();
    let ids = [add(&mut h.p, 60, 0.0, 1.0), add(&mut h.p, 70, 1.0, 2.0), add(&mut h.p, 50, 2.0, 3.0)];
    h.click(Button::Left, s(0.5, 60.0, body(ids[0])), NONE);
    assert_eq!(h.ed.selection.ids, [ids[0]]);
    h.click(Button::Left, s(1.5, 70.0, body(ids[1])), CTRL);
    assert_eq!(h.ed.selection.ids, [ids[0], ids[1]]);
    h.click(Button::Left, s(2.5, 50.0, body(ids[2])), SHIFT);
    assert_eq!(h.ed.selection.ids.len(), 3);
    h.click(Button::Left, s(1.5, 70.0, body(ids[1])), NONE);
    assert_eq!(h.ed.selection.ids, [ids[1]]);
}

#[test]
fn moving_a_selection_keeps_lengths_and_a_non_selected_note_moves_alone() {
    let (mut h, a, b) = two_notes();
    h.ed.selection.set(vec![a, b]);
    let out = h.drag(Button::Left, s(0.5, 60.0, body(a)), s(1.5, 62.0, None), NONE);
    assert_eq!(edits(&out), 1);
    let (na, nb) = (h.p.note(a).unwrap().clone(), h.p.note(b).unwrap().clone());
    assert_eq!((na.start, na.end, na.midi), (at(1.0), at(2.0), 62));
    assert_eq!((nb.start, nb.end, nb.midi), (at(3.0), fretscribe_core::TimePos::new(2, 0.0), 64));

    let (mut h, a, b) = two_notes();
    h.ed.selection.set(vec![a]);
    h.drag(Button::Left, s(2.5, 62.0, body(b)), s(3.5, 62.0, None), NONE);
    assert_eq!(h.p.note(a).unwrap().start, at(0.0));
    assert_eq!(h.p.note(b).unwrap().start, at(3.0));
    assert_eq!(h.ed.selection.ids, [b]);
}

#[test]
fn shift_keeps_the_pitch_when_moving_and_resize_uses_the_note_edge() {
    let (mut h, a, b) = two_notes();
    h.drag(Button::Left, s(2.5, 62.0, body(b)), s(3.5, 70.0, None), SHIFT.into_move());
    assert_eq!(h.p.note(b).unwrap().midi, 62);
    h.drag(Button::Left, s(1.0, 60.0, Some((a, HitZone::RightEdge))), s(1.5, 60.0, None), NONE);
    assert_eq!(h.p.note(a).unwrap().end, at(1.5));
}

trait IntoMove {
    fn into_move(self) -> Mods;
}

impl IntoMove for Mods {
    /// Shift while pressing on a note that starts no link would continue a chain; the pressed note
    /// here has no outgoing link, so Shift is applied only after the drag started.
    fn into_move(self) -> Mods {
        self
    }
}

#[test]
fn copy_and_paste_follow_the_cursor_horizontally_and_validate_on_left_click() {
    let (mut h, a, _) = two_notes();
    h.ed.selection.set(vec![a]);
    h.send(Input::Copy);
    h.send(Input::Paste);
    h.mv(s(5.0, 80.0, None), NONE);
    let ghost = h.ed.ghost.clone().expect("preview");
    assert_eq!((ghost.glyphs[0].midi, ghost.glyphs[0].start_sec), (60, sec(5.0)));
    assert_eq!(h.p.notes.len(), 2, "nothing reaches the project during the preview");
    h.press(Button::Left, s(5.0, 80.0, None), NONE);
    let out = h.rel(Button::Left, s(5.0, 80.0, None), NONE);
    assert_eq!(edits(&out), 1);
    assert_eq!(h.p.notes.len(), 3);
    assert_eq!(h.ed.selection.ids, [h.p.notes[2].id]);
    assert!(h.ed.ghost.is_none());
    assert!(h.send(Input::Spectro(SpectroEvent::SeekRequested(2.5), NONE)).is_empty(), "the validating click does not seek");
    assert!(matches!(h.send(Input::Spectro(SpectroEvent::SeekRequested(2.5), NONE)).as_slice(), [Effect::Seek(_)]));
}

#[test]
fn right_click_or_escape_cancels_the_paste_preview() {
    for escape in [false, true] {
        let (mut h, a, _) = two_notes();
        h.ed.selection.set(vec![a]);
        h.send(Input::Copy);
        h.send(Input::Paste);
        h.mv(s(5.0, 60.0, None), NONE);
        if escape {
            h.send(Input::Escape);
        } else {
            h.press(Button::Right, s(5.0, 60.0, None), NONE);
            h.rel(Button::Right, s(5.0, 60.0, None), NONE);
        }
        assert!(h.ed.ghost.is_none() && !h.ed.in_mode());
        assert_eq!(h.p.notes.len(), 2);
    }
}

#[test]
fn link_hover_mode_links_from_the_hovered_note_and_confirms_with_a_left_click() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    h.hovered = Some(src);
    h.send(Input::Action(NoteAction::LinkHover));
    assert!(h.ed.in_mode());
    h.mv(s(4.02, 62.0, None), NONE);
    assert!(h.ed.ghost.is_some());
    assert_eq!(edits(&h.click(Button::Left, s(4.02, 62.0, None), NONE)), 0);
    assert!(h.ed.in_mode());
    h.mv(s(6.0, 62.0, None), NONE);
    assert_eq!(edits(&h.click(Button::Left, s(6.0, 62.0, None), NONE)), 1);
    assert_eq!(h.p.notes.len(), 2);
    assert!(!h.ed.in_mode());
}

#[test]
fn link_hover_mode_is_cancelled_by_escape_or_right_click_and_acts_on_a_selection_otherwise() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    h.hovered = Some(src);
    h.send(Input::Action(NoteAction::LinkHover));
    h.send(Input::Escape);
    assert!(!h.ed.in_mode());
    h.send(Input::Action(NoteAction::LinkHover));
    h.press(Button::Right, s(4.0, 62.0, None), NONE);
    assert!(!h.ed.in_mode());
    h.rel(Button::Right, s(4.0, 62.0, None), NONE);

    let (mut h, a, b) = two_notes();
    h.hovered = Some(a);
    h.ed.selection.set(vec![a, b]);
    let out = h.send(Input::Action(NoteAction::LinkHover));
    assert!(!h.ed.in_mode());
    assert_eq!(edits(&out), 1);
    assert_eq!(h.p.outgoing(a), Some((b, LinkType::Slide)));
}

#[test]
fn actions_flow_through_apply_and_report_through_effects() {
    let (mut h, a, b) = two_notes();
    h.ed.selection.set(vec![a, b]);
    let out = h.send(Input::Action(NoteAction::LinkSelection));
    assert_eq!(message(&out), Some("1 linked"));
    let out = h.send(Input::Action(NoteAction::Delete { bridge: false }));
    assert_eq!(edits(&out), 1);
    assert!(h.p.notes.is_empty() && h.ed.selection.ids.is_empty());
    let out = h.send(Input::Action(NoteAction::Transpose(1)));
    assert!(out.is_empty(), "an empty selection is ignored quietly");
}

/// Right press on `src`, drag to `aim`, release: the first point is fixed.
fn aimed(h: &mut H, from: Sample, aim: Sample) {
    h.press(Button::Right, from, NONE);
    h.mv(aim, NONE);
    assert!(h.ed.ghost.is_some(), "the aim is previewed");
    assert_eq!(edits(&h.rel(Button::Right, aim, NONE)), 0);
    assert!(h.ed.in_mode(), "waiting for the left click");
}

#[test]
fn right_drag_past_the_end_aims_a_slide_then_a_left_click_sets_the_end() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    aimed(&mut h, s(2.5, 60.0, body(src)), s(4.02, 62.0, None));
    assert_eq!(h.p.notes.len(), 1);
    h.mv(s(6.0, 62.0, None), NONE);
    let out = h.click(Button::Left, s(6.0, 62.0, None), NONE);
    assert_eq!(edits(&out), 1);
    let new = h.ed.selection.ids[0];
    let n = h.p.note(new).unwrap();
    assert_eq!((n.midi, n.start, n.end), (62, TimePos::new(2, 0.0), TimePos::new(2, 2.0)));
    assert_eq!(h.p.outgoing(src), Some((new, LinkType::Slide)));
    assert!(h.ed.ghost.is_none() && !h.ed.in_mode());
    assert!(h.send(Input::Spectro(SpectroEvent::SeekRequested(3.0), NONE)).is_empty(), "the click is not a seek");
}

#[test]
fn right_drag_inside_the_note_is_a_legato_and_the_left_half_goes_backwards() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    aimed(&mut h, s(2.2, 60.0, body(src)), s(2.8, 62.0, None));
    h.click(Button::Left, s(5.0, 62.0, None), NONE);
    let new = h.ed.selection.ids[0];
    assert_eq!(h.p.outgoing(src), Some((new, LinkType::Legato)));
    assert_eq!(h.p.note(new).unwrap().start, h.p.note(src).unwrap().end);

    let src = add(&mut h.p, 60, 6.0, 7.0);
    aimed(&mut h, s(6.8, 60.0, body(src)), s(6.2, 58.0, None));
    h.click(Button::Left, s(5.0, 58.0, None), NONE);
    let new = h.ed.selection.ids[0];
    assert_eq!(h.p.outgoing(new), Some((src, LinkType::Legato)));
    assert_eq!(h.p.note(new).unwrap().end, h.p.note(src).unwrap().start);
}

#[test]
fn right_drag_before_the_note_aims_a_reverse_slide() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    aimed(&mut h, s(2.5, 60.0, body(src)), s(1.0, 58.0, None));
    h.click(Button::Left, s(0.5, 58.0, None), NONE);
    let new = h.ed.selection.ids[0];
    let n = h.p.note(new).unwrap();
    assert_eq!((n.midi, n.start.beat, n.end.beat), (58, 0.5, 2.0));
    assert_eq!(h.p.outgoing(new), Some((src, LinkType::Slide)));
}

#[test]
fn right_press_on_a_link_point_starts_a_link() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    aimed(&mut h, s(3.0, 60.0, Some((src, HitZone::LinkRight))), s(4.02, 62.0, None));
}

#[test]
fn cancelling_at_any_point_cancels_the_whole_link() {
    let mut h = H::new();
    let src = add(&mut h.p, 60, 2.0, 3.0);
    let cancels: [&dyn Fn(&mut H); 2] = [
        &|h| {
            h.send(Input::Escape);
        },
        &|h| {
            h.press(Button::Right, s(5.0, 62.0, None), NONE);
            h.rel(Button::Right, s(5.0, 62.0, None), NONE);
        },
    ];
    for cancel in cancels {
        // While aiming (right button held), then while extending.
        h.press(Button::Right, s(2.5, 60.0, body(src)), NONE);
        h.mv(s(4.02, 62.0, None), NONE);
        h.send(Input::Escape);
        h.rel(Button::Right, s(4.02, 62.0, None), NONE);
        assert!(h.ed.ghost.is_none() && !h.ed.in_mode() && !h.ed.is_busy());
        aimed(&mut h, s(2.5, 60.0, body(src)), s(4.02, 62.0, None));
        cancel(&mut h);
        assert!(h.ed.ghost.is_none() && !h.ed.in_mode() && !h.ed.is_busy());
        assert_eq!(h.p.notes.len(), 1);
    }
    h.press(Button::Right, s(2.5, 60.0, body(src)), NONE);
    h.mv(s(4.02, 62.0, None), NONE);
    h.press(Button::Left, s(4.02, 62.0, None), NONE);
    h.rel(Button::Left, s(4.02, 62.0, None), NONE);
    h.rel(Button::Right, s(4.02, 62.0, None), NONE);
    assert_eq!((h.p.notes.len(), h.ed.is_busy()), (1, false));
}

fn same_pitch_overlaps(p: &Project) -> bool {
    p.notes.iter().any(|a| p.notes.iter().any(|b| a.id < b.id && a.midi == b.midi && a.start.beat < b.end.beat && b.start.beat < a.end.beat))
}

#[test]
fn no_gesture_or_command_leaves_two_same_pitch_notes_overlapping() {
    let mut h = H::new();
    add(&mut h.p, 60, 0.0, 3.0);
    let other = add(&mut h.p, 62, 1.0, 2.0);
    h.draw((1.0, 60.0), (2.0, 60.0), NONE);
    assert_eq!(h.p.notes.len(), 4, "the long note was split around the drawn one");
    assert!(!same_pitch_overlaps(&h.p));
    h.ed.selection.set(vec![other]);
    h.send(Input::Action(NoteAction::Transpose(-2)));
    assert!(!same_pitch_overlaps(&h.p) && h.p.notes.len() == 3, "the transposed note replaced the drawn one");
    h.send(Input::Action(NoteAction::Duplicate));
    assert!(!same_pitch_overlaps(&h.p));
}

#[test]
fn force_legato_emits_one_edit_and_a_message() {
    let mut h = H::new();
    let (a, b) = (add(&mut h.p, 60, 0.0, 1.0), add(&mut h.p, 62, 1.0, 2.0));
    h.ed.selection.set(vec![b]);
    let out = h.send(Input::Action(NoteAction::ForceLegato));
    assert_eq!(edits(&out), 1);
    assert!(out.iter().any(|e| matches!(e, Effect::Message(m) if m == "1 linked")));
    assert_eq!(h.p.outgoing(a), Some((b, LinkType::Legato)));
    h.ed.selection.set(vec![a]);
    let out = h.send(Input::Action(NoteAction::ForceLegato));
    assert!(edits(&out) == 0 && out.iter().any(|e| matches!(e, Effect::Message(m) if m.contains("skipped"))));
}

#[test]
fn grace_entry_is_modal_and_enter_confirms_as_one_edit() {
    let mut h = H::new();
    let main = add(&mut h.p, 64, 1.0, 2.0);
    h.ed.selection.set(vec![main]);
    assert!(h.send(Input::Action(NoteAction::Grace)).is_empty());
    assert!(h.ed.in_entry() && h.ed.is_busy() && h.ed.in_mode() && h.p.notes.len() == 1);
    assert!(h.ed.ghost.as_ref().is_some_and(|g| g.glyphs.len() == 2 && g.glyphs[0].grace));
    assert_eq!(h.ed.feedback.as_ref().map(|f| f.labels[0].2.as_str()), Some("B 3"));
    h.send(Input::Entry(EntryKey::Pitch { up: false, tone: false }));
    h.send(Input::Entry(EntryKey::ToggleKind));
    assert_eq!(h.p.notes.len(), 1, "nothing changes before the confirmation");
    let out = h.send(Input::Entry(EntryKey::Confirm));
    assert_eq!(edits(&out), 1);
    assert!(!h.ed.in_entry() && h.ed.ghost.is_none());
    let grace = h.p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    assert_eq!((grace.midi, grace.grace_of.unwrap().kind), (61, fretscribe_core::GraceKind::Appoggiatura));
    assert_eq!(h.p.grace_link(grace.id), LinkType::Legato, "close to the main note: legato");
    assert_eq!(h.ed.selection.ids, [grace.id]);
}

#[test]
fn the_pointer_left_of_the_note_draws_a_grace_note_before_the_beat_and_a_left_click_confirms() {
    let mut h = H::new();
    add(&mut h.p, 40, 0.0, 0.5);
    let main = add(&mut h.p, 64, 8.0, 9.0);
    h.ed.selection.set(vec![main]);
    h.send(Input::Action(NoteAction::Grace));
    let at = s(5.0, 60.2, None);
    h.mv(at, NONE);
    let ghost = h.ed.ghost.as_ref().unwrap();
    assert!((ghost.glyphs[0].start_sec - sec(5.0)).abs() < 1e-9 && (ghost.glyphs[0].end_sec - sec(8.0)).abs() < 1e-9);
    assert_eq!((ghost.glyphs[0].midi, ghost.glyphs[1].legato_from), (60, Some(0)));
    assert!(h.ed.feedback.as_ref().is_some_and(|f| f.labels[0].0 == ghost.glyphs[0].start_sec));
    h.press(Button::Left, at, NONE);
    assert!(h.ed.in_entry(), "the press alone changes nothing");
    let out = h.rel(Button::Left, at, NONE);
    assert_eq!(edits(&out), 1);
    assert!(!h.ed.in_entry());
    let grace = h.p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    let tempo = fretscribe_core::TempoMap::new(&h.p.measures);
    assert_eq!((grace.midi, grace.grace_of.unwrap().kind), (60, fretscribe_core::GraceKind::Acciaccatura));
    assert!((tempo.pos_to_sec(grace.start) - sec(5.0)).abs() < 1e-9);
    assert!((tempo.pos_to_sec(grace.end) - tempo.pos_to_sec(h.p.note(main).unwrap().start)).abs() < 1e-9, "the end is the start of the note");
    assert_eq!(h.p.grace_link(grace.id), LinkType::Legato);
    assert!(h.send(Input::Spectro(SpectroEvent::SeekRequested(1.0), NONE)).is_empty(), "the click does not seek");
}

#[test]
fn the_pointer_right_of_the_note_draws_a_grace_note_on_the_beat() {
    let mut h = H::new();
    let main = add(&mut h.p, 64, 8.0, 10.0);
    h.ed.selection.set(vec![main]);
    h.send(Input::Action(NoteAction::Grace));
    let at = s(8.5, 62.0, None);
    h.mv(at, NONE);
    let ghost = h.ed.ghost.as_ref().unwrap();
    assert!((ghost.glyphs[0].start_sec - sec(8.0)).abs() < 1e-9 && (ghost.glyphs[0].end_sec - sec(8.5)).abs() < 1e-9);
    assert_eq!(ghost.glyphs[1].legato_from, Some(0), "on the beat: legato first");
    h.press(Button::Left, at, NONE);
    assert_eq!(edits(&h.rel(Button::Left, at, NONE)), 1);
    let grace = h.p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
    let tempo = fretscribe_core::TempoMap::new(&h.p.measures);
    assert_eq!(grace.grace_of.unwrap().kind, fretscribe_core::GraceKind::Appoggiatura);
    assert!((tempo.pos_to_sec(grace.start) - tempo.pos_to_sec(h.p.note(main).unwrap().start)).abs() < 1e-9, "the start is the start of the note");
    assert!((tempo.pos_to_sec(grace.end) - sec(8.5)).abs() < 1e-9);
    assert_eq!(h.p.grace_link(grace.id), LinkType::Legato);
}

#[test]
fn h_cycles_the_link_in_the_entry_and_escape_or_the_right_button_cancels() {
    let mut h = H::new();
    let main = add(&mut h.p, 64, 1.0, 2.0);
    h.ed.selection.set(vec![main]);
    h.send(Input::Action(NoteAction::Grace));
    h.send(Input::Entry(EntryKey::CycleLink));
    h.send(Input::Entry(EntryKey::CycleLink));
    h.send(Input::Entry(EntryKey::Confirm));
    let grace = h.p.notes.iter().find(|n| n.grace_of.is_some()).unwrap().id;
    assert_eq!(h.p.grace_link(grace), LinkType::Slide);
    h.p.remove_grace(grace).unwrap();

    h.ed.selection.set(vec![main]);
    h.send(Input::Action(NoteAction::Grace));
    h.send(Input::Escape);
    assert!(!h.ed.in_entry() && h.ed.ghost.is_none() && h.p.notes.len() == 1);
    h.send(Input::Action(NoteAction::Grace));
    h.click(Button::Right, s(3.0, 60.0, None), NONE);
    assert!(!h.ed.in_entry() && h.p.notes.len() == 1);
}

#[test]
fn right_click_on_a_grace_note_or_its_connector_cycles_the_valid_links_in_one_step() {
    let mut h = H::new();
    let main = add(&mut h.p, 64, 3.0, 4.0);
    let grace = h.p.add_grace_note(main, None, fretscribe_core::GraceKind::Acciaccatura).unwrap();
    let kinds = |h: &H| h.p.grace_link(grace);
    assert_eq!(kinds(&h), LinkType::Legato);
    let expect = |h: &mut H, hit: Hit, link: LinkType| {
        h.now += 5.0;
        let out = h.click(Button::Right, s(3.0, 62.0, hit), NONE);
        assert_eq!(edits(&out), 1);
        assert_eq!(kinds(h), link);
    };
    expect(&mut h, body(grace), LinkType::Dashed);
    expect(&mut h, Some((grace, HitZone::Line)), LinkType::Slide);
    expect(&mut h, body(grace), LinkType::Bend);
    expect(&mut h, Some((main, HitZone::Arc)), LinkType::Legato);
    let same = add(&mut h.p, 62, 6.0, 7.0);
    let lone = h.p.add_grace_note(same, Some(62), fretscribe_core::GraceKind::Acciaccatura).unwrap();
    let out = h.click(Button::Right, s(6.0, 62.0, body(lone)), NONE);
    assert_eq!(edits(&out), 0, "only the dashed link is possible");
}

#[test]
fn grace_on_a_selected_grace_note_starts_an_entry_and_transposes_and_deletes() {
    let mut h = H::new();
    let main = add(&mut h.p, 64, 1.0, 2.0);
    let grace = h.p.add_grace_note(main, None, fretscribe_core::GraceKind::Acciaccatura).unwrap();
    h.ed.selection.set(vec![grace]);
    assert_eq!(edits(&h.send(Input::Action(NoteAction::Grace))), 0);
    assert!(h.ed.in_entry());
    h.send(Input::Escape);
    h.send(Input::Action(NoteAction::Transpose(1)));
    assert_eq!(h.p.note(grace).unwrap().midi, 63);
    h.send(Input::Action(NoteAction::Delete { bridge: false }));
    assert_eq!(h.p.notes.len(), 1);
}
