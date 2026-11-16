//! Drives the widget through egui frames with synthetic input, without a window.

use egui::{CentralPanel, Event, Modifiers, PointerButton, Pos2, RawInput};
use fretscribe_core::{Project, TimeSignature, Track, TrackRole};

use super::*;

struct Harness {
    ctx: Context,
    project: Project,
    view: TimelineView,
    snap: SnapGrid,
    policy: NotePolicy,
    time: f64,
    rect: Rect,
    grid_id: Id,
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
            grid_id: Id::NULL,
        };
        h.frame(Vec::new(), Modifiers::NONE);
        h.frame(Vec::new(), Modifiers::NONE);
        h
    }

    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<GridEvent> {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 300.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let tempo = TempoMap::new(&self.project.measures);
        let (view, snap, policy) = (self.view, self.snap.clone(), self.policy);
        let mut out = Vec::new();
        let (mut rect, mut grid_id) = (self.rect, self.grid_id);
        let _ = self.ctx.run(raw, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                rect = ui.max_rect();
                grid_id = ui.id().with("measure_grid");
                out = GridOverlay::new(&tempo, &view, &snap)
                    .note_policy(policy)
                    .show(ui, rect);
            });
        });
        self.rect = rect;
        self.grid_id = grid_id;
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
fn a_click_without_movement_changes_nothing() {
    let mut h = Harness::new();
    let at = h.line_pos(2.0);
    h.mouse(at, Modifiers::NONE);
    h.button(at, true, Modifiers::NONE);
    let mut events = h.button(at, false, Modifiers::NONE);
    events.extend(h.frame(Vec::new(), Modifiers::NONE));
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn clicking_a_label_opens_the_settings_popup() {
    let mut h = Harness::new();
    let at = pos2(h.line_pos(2.0).x + 20.0, h.rect.top() + 9.0);
    h.mouse(at, Modifiers::NONE);
    h.button(at, true, Modifiers::NONE);
    h.button(at, false, Modifiers::NONE);
    h.frame(Vec::new(), Modifiers::NONE);
    let popup = h.ctx.data(|d| d.get_temp::<SettingsPopup>(h.grid_id.with("popup")));
    let popup = popup.expect("popup should be open");
    assert_eq!((popup.settings.index, popup.settings.bpm), (1, 120.0));
    assert!(popup.settings.apply_to_following);
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
