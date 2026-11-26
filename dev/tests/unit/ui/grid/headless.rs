//! Drives the widget through egui frames with synthetic input, without a window.

use egui::{CentralPanel, Event, Modifiers, PointerButton, Pos2, RawInput};
use fretscribe_core::{Project, TimeSignature, Track, TrackRole};

use super::*;
use crate::view::MEASURE_HEADER_H;

struct Harness {
    ctx: Context,
    project: Project,
    view: TimelineView,
    snap: SnapGrid,
    policy: NotePolicy,
    time: f64,
    rect: Rect,
    header: bool,
}

impl Harness {
    fn new() -> Self {
        let project = Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            8.0,
            120.0,
            TimeSignature::COMMON,
        );
        let mut h = Self {
            ctx: Context::default(),
            project,
            view: TimelineView::new(0.0, 100.0),
            snap: SnapGrid::default(),
            policy: NotePolicy::FollowGrid,
            time: 0.0,
            rect: Rect::NOTHING,
            header: false,
        };
        crate::theme::install_fonts(&h.ctx);
        crate::kit::install(&h.ctx);
        h.ctx.style_mut(|s| s.interaction.tooltip_delay = f32::INFINITY);
        h.frame(Vec::new(), Modifiers::NONE);
        h.frame(Vec::new(), Modifiers::NONE);
        h
    }

    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<GridEvent> {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let tempo = TempoMap::new(&self.project.measures);
        let (view, snap, policy) = (self.view, self.snap.clone(), self.policy);
        let mut out = Vec::new();
        let (mut rect, header, fits) = (self.rect, self.header, Default::default());
        let _ = self.ctx.run(raw, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                rect = ui.max_rect();
                let mut grid = GridOverlay::new(&tempo, &view, &snap).note_policy(policy);
                if header {
                    grid = grid.with_header(HeaderData { fits: &fits, region: None, audio_len_sec: 8.0 });
                }
                out = grid.show(ui, rect);
            });
        });
        self.rect = rect;
        out
    }

    fn line_pos(&self, sec: f64) -> Pos2 {
        pos2(self.rect.left() + self.view.sec_to_x(sec), self.rect.center().y)
    }

    fn mouse(&mut self, pos: Pos2, modifiers: Modifiers) -> Vec<GridEvent> {
        self.frame(vec![Event::PointerMoved(pos)], modifiers)
    }

    fn button(&mut self, pos: Pos2, pressed: bool, modifiers: Modifiers) -> Vec<GridEvent> {
        let event = Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers };
        self.frame(vec![event], modifiers)
    }

    fn drag(&mut self, from: Pos2, dx: f32, modifiers: Modifiers) -> Vec<GridEvent> {
        let mut events = Vec::new();
        events.extend(self.mouse(from, modifiers));
        events.extend(self.button(from, true, modifiers));
        let mut pos = from;
        for _ in 0..4 {
            pos.x += dx / 4.0;
            events.extend(self.mouse(pos, modifiers));
        }
        events.extend(self.button(pos, false, modifiers));
        events.extend(self.frame(Vec::new(), modifiers));
        events
    }
}

#[test]
fn dragging_a_barline_reports_the_new_start_without_touching_the_project() {
    let mut h = Harness::new();
    let before = h.project.clone();
    let events = h.drag(h.line_pos(2.0), 60.0, Modifiers::NONE);
    assert_eq!(events.len(), 1, "{events:?}");
    let GridEvent::MeasureMoved { index, new_start_sec, policy } = events[0] else {
        panic!("unexpected {:?}", events[0]);
    };
    assert_eq!(index, 1);
    assert!((new_start_sec - 2.6).abs() < 0.02, "{new_start_sec}");
    assert_eq!(policy, NotePolicy::FollowGrid);
    assert_eq!(h.project, before);
}

#[test]
fn alt_while_dragging_uses_the_other_policy() {
    let mut h = Harness::new();
    let events = h.drag(h.line_pos(4.0), -50.0, Modifiers::ALT);
    assert!(
        matches!(events[..], [GridEvent::MeasureMoved { index: 2, policy: NotePolicy::KeepAudioPosition, .. }]),
        "{events:?}"
    );
    h.policy = NotePolicy::KeepAudioPosition;
    let events = h.drag(h.line_pos(4.0), -50.0, Modifiers::ALT);
    assert!(matches!(events[..], [GridEvent::MeasureMoved { policy: NotePolicy::FollowGrid, .. }]));
}

#[test]
fn a_dragged_barline_cannot_cross_its_neighbour() {
    let mut h = Harness::new();
    h.project.measures[2].anchored = true;
    let events = h.drag(h.line_pos(2.0), 300.0, Modifiers::NONE);
    let [GridEvent::MeasureMoved { new_start_sec, .. }] = events[..] else { panic!("{events:?}") };
    assert!(new_start_sec < 4.0 && new_start_sec > 3.9, "{new_start_sec}");
}

#[test]
fn a_click_without_movement_only_seeks() {
    let mut h = Harness::new();
    let at = h.line_pos(2.0);
    h.mouse(at, Modifiers::NONE);
    h.button(at, true, Modifiers::NONE);
    let mut events = h.button(at, false, Modifiers::NONE);
    events.extend(h.frame(Vec::new(), Modifiers::NONE));
    let [GridEvent::SeekRequested(sec)] = events[..] else { panic!("{events:?}") };
    assert!((sec - 2.0).abs() < 0.05, "{sec}");
}

#[test]
fn escape_or_the_other_button_cancels_a_barline_drag() {
    let button = |pos, button, pressed| Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE };
    let escape = Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
    for escaping in [true, false] {
        let mut h = Harness::new();
        let (from, to) = (h.line_pos(2.0), h.line_pos(2.6));
        h.mouse(from, Modifiers::NONE);
        h.button(from, true, Modifiers::NONE);
        let mut events = h.mouse(to, Modifiers::NONE);
        if escaping {
            events.extend(h.frame(vec![escape.clone()], Modifiers::NONE));
        } else {
            events.extend(h.frame(vec![button(to, PointerButton::Secondary, true)], Modifiers::NONE));
            events.extend(h.frame(vec![button(to, PointerButton::Secondary, false)], Modifiers::NONE));
        }
        events.extend(h.button(to, false, Modifiers::NONE));
        events.extend(h.frame(Vec::new(), Modifiers::NONE));
        assert!(events.is_empty(), "{escaping} {events:?}");
        assert!(!crate::kit::clicks_swallowed(&h.ctx));
        assert_eq!(h.drag(from, 60.0, Modifiers::NONE).len(), 1, "the next drag works");
    }
}

#[test]
fn a_locked_barline_cannot_be_dragged() {
    let mut h = Harness::new();
    h.project.measures[1].locked = true;
    assert!(h.drag(h.line_pos(2.0), 60.0, Modifiers::NONE).is_empty());
    assert!(h.drag(h.line_pos(4.0), 60.0, Modifiers::NONE).is_empty());
}

fn click(h: &mut Harness, at: Pos2, modifiers: Modifiers) -> Vec<GridEvent> {
    let mut events = h.mouse(at, modifiers);
    events.extend(h.button(at, true, modifiers));
    events.extend(h.button(at, false, modifiers));
    h.time += 1.0; // never a double click with the next one
    events
}

/// A point of the header band, `x` px right of the barline at `sec`.
fn header_pos(h: &Harness, sec: f64, x: f32) -> Pos2 {
    pos2(h.line_pos(sec).x + x, h.rect.top() + MEASURE_HEADER_H / 2.0)
}

#[test]
fn the_number_selects_extends_and_loops() {
    let mut h = Harness::new();
    h.header = true;
    let at = header_pos(&h, 2.0, 18.0);
    assert_eq!(click(&mut h, at, Modifiers::NONE), [GridEvent::MeasureSelected { index: 1, extend: false }]);
    assert_eq!(click(&mut h, at, Modifiers::SHIFT), [GridEvent::MeasureSelected { index: 1, extend: true }]);
    click(&mut h, at, Modifiers::NONE);
    h.time -= 1.0;
    assert_eq!(click(&mut h, at, Modifiers::NONE), [GridEvent::MeasureLoopRequested(1)]);
}

#[test]
fn a_toggle_click_reports_the_new_value_and_ctrl_propagates() {
    let mut h = Harness::new();
    h.header = true;
    h.view = TimelineView::new(0.0, 200.0);
    h.frame(Vec::new(), Modifiers::NONE);
    // The lock sits left of the overflow button, which hugs the right edge of the 400 px measure.
    let lock = header_pos(&h, 0.0, 400.0 - 8.0 - 26.0 - 4.0 - 13.0);
    assert_eq!(click(&mut h, lock, Modifiers::NONE), [GridEvent::LockToggled { index: 0, on: true, propagate: false }]);
    assert_eq!(click(&mut h, lock, Modifiers::COMMAND), [GridEvent::LockToggled { index: 0, on: true, propagate: true }]);
}

#[test]
fn the_widget_ignores_the_wheel() {
    let mut h = Harness::new();
    let at = h.line_pos(2.0);
    h.mouse(at, Modifiers::NONE);
    let wheel = Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 3.0),
        modifiers: Modifiers::NONE,
    };
    assert!(h.frame(vec![wheel], Modifiers::NONE).is_empty());
    assert_eq!(h.view, TimelineView::new(0.0, 100.0));
}

#[test]
fn a_view_without_any_line_in_it_produces_nothing() {
    let mut h = Harness::new();
    h.view = TimelineView::new(500.0, 100.0);
    assert!(h.frame(Vec::new(), Modifiers::NONE).is_empty());
}
