//! Drives the widget through egui frames with synthetic input, without a window.

use egui::{CentralPanel, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

use super::*;

const LEN: f64 = 120.0;
const MAIN_W: f32 = 800.0;

struct Harness {
    ctx: Context,
    view: TimelineView,
    region: Option<RegionSelection>,
    measures: Vec<f64>,
    time: f64,
    strip: Rect,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            ctx: Context::default(),
            view: TimelineView::new(0.0, 100.0),
            region: None,
            measures: (0..60).map(|k| f64::from(k) * 2.0).collect(),
            time: 0.0,
            strip: Rect::NOTHING,
        };
        h.frame(Vec::new(), Modifiers::NONE);
        h.frame(Vec::new(), Modifiers::NONE);
        h
    }

    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<OverviewEvent> {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 200.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let (view, measures) = (self.view, self.measures.clone());
        let mut region = self.region;
        let mut out = Vec::new();
        let mut strip = self.strip;
        let _ = self.ctx.run(raw, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let peaks = |range: Range<f64>, n: usize| {
                    (0..n)
                        .map(|i| {
                            let t = range.start + (range.end - range.start) * i as f64 / n as f64;
                            let a = (t * 0.7).sin().abs() as f32 * 0.8;
                            (-a, a, a * 0.5)
                        })
                        .collect()
                };
                let output = OverviewTimeline::new(LEN, &view, MAIN_W, peaks)
                    .measure_starts(&measures)
                    .playhead(10.0)
                    .show(ui, &mut region);
                out = output.events;
                strip = output.strip;
            });
        });
        self.region = region;
        self.strip = strip;
        for event in &out {
            if let OverviewEvent::ViewportChanged { scroll_sec, pixels_per_sec } = *event {
                self.view = TimelineView::new(scroll_sec, pixels_per_sec);
            }
        }
        out
    }

    fn pos(&self, sec: f64) -> Pos2 {
        let map = OverviewMap { audio_len_sec: LEN, width_px: self.strip.width() };
        Pos2::new(self.strip.left() + map.sec_to_x(sec), self.strip.top() + 30.0)
    }

    fn mouse(&mut self, pos: Pos2, m: Modifiers) -> Vec<OverviewEvent> {
        self.frame(vec![Event::PointerMoved(pos)], m)
    }

    fn button(&mut self, pos: Pos2, pressed: bool, m: Modifiers) -> Vec<OverviewEvent> {
        let event = Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: m };
        self.frame(vec![event], m)
    }

    fn click(&mut self, pos: Pos2) -> Vec<OverviewEvent> {
        let mut events = self.mouse(pos, Modifiers::NONE);
        events.extend(self.button(pos, true, Modifiers::NONE));
        events.extend(self.button(pos, false, Modifiers::NONE));
        events
    }

    fn drag(&mut self, from_sec: f64, to_sec: f64, m: Modifiers) -> Vec<OverviewEvent> {
        let (from, to) = (self.pos(from_sec), self.pos(to_sec));
        let mut events = self.mouse(from, m);
        events.extend(self.button(from, true, m));
        for step in 1..=5 {
            let t = step as f32 / 5.0;
            events.extend(self.mouse(Pos2::new(from.x + (to.x - from.x) * t, from.y), m));
        }
        events.extend(self.button(to, false, m));
        events.extend(self.frame(Vec::new(), m));
        events
    }
}

fn last_region(events: &[OverviewEvent]) -> Option<RegionSelection> {
    events.iter().rev().find_map(|e| match e {
        OverviewEvent::RegionChanged(r) => *r,
        _ => None,
    })
}

#[test]
fn a_plain_click_seeks() {
    let mut h = Harness::new();
    let target = h.pos(60.0);
    let events = h.click(target);
    let [OverviewEvent::SeekRequested(sec)] = events[..] else { panic!("{events:?}") };
    assert!((sec - 60.0).abs() < 0.5, "{sec}");
    assert!(h.region.is_none());
}

#[test]
fn dragging_on_empty_strip_creates_a_region() {
    let mut h = Harness::new();
    let events = h.drag(90.0, 100.0, Modifiers::NONE);
    let r = last_region(&events).expect("region event");
    assert!((r.start_sec - 90.0).abs() < 0.6 && (r.end_sec - 100.0).abs() < 0.6, "{r:?}");
    assert_eq!(h.region, Some(r));
    assert!(!events.iter().any(|e| matches!(e, OverviewEvent::SeekRequested(_))), "{events:?}");
}

#[test]
fn dragging_right_to_left_still_gives_an_ordered_region() {
    let mut h = Harness::new();
    h.drag(100.0, 70.0, Modifiers::NONE);
    let r = h.region.unwrap();
    assert!(r.start_sec < r.end_sec && (r.start_sec - 70.0).abs() < 0.6);
}

#[test]
fn double_click_clears_the_region() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 40.0, end_sec: 50.0 });
    let at = h.pos(100.0);
    h.mouse(at, Modifiers::NONE);
    let mut events = Vec::new();
    for _ in 0..2 {
        events.extend(h.button(at, true, Modifiers::NONE));
        events.extend(h.button(at, false, Modifiers::NONE));
    }
    assert!(h.region.is_none(), "{events:?}");
    assert!(events.contains(&OverviewEvent::RegionChanged(None)));
}

#[test]
fn dragging_the_viewport_pans_the_main_view() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    let events = h.drag(24.0, 34.0, Modifiers::NONE);
    assert!(h.region.is_none());
    assert!((h.view.scroll_sec - 30.0).abs() < 0.8, "{:?}", h.view);
    assert_eq!(h.view.pixels_per_sec, 100.0);
    assert!(events.iter().any(|e| matches!(e, OverviewEvent::ViewportChanged { .. })));
}

#[test]
fn dragging_a_viewport_edge_zooms_the_main_view() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    h.drag(28.0, 48.0, Modifiers::NONE);
    assert!((h.view.scroll_sec - 20.0).abs() < 1e-9);
    assert!((h.view.pixels_per_sec - 800.0 / 28.0).abs() < 1.5, "{:?}", h.view);
    assert!(h.region.is_none());
}

#[test]
fn region_handles_resize_and_the_body_moves() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 60.0, end_sec: 80.0 });
    h.drag(80.0, 90.0, Modifiers::NONE);
    let r = h.region.unwrap();
    assert!((r.start_sec - 60.0).abs() < 1e-9 && (r.end_sec - 90.0).abs() < 0.6, "{r:?}");
    h.drag(70.0, 75.0, Modifiers::NONE);
    let moved = h.region.unwrap();
    assert!((moved.len_sec() - r.len_sec()).abs() < 1e-9 && (moved.start_sec - 65.0).abs() < 0.8, "{moved:?}");
}

#[test]
fn shift_creates_a_region_over_the_viewport() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    h.drag(22.0, 26.0, Modifiers::SHIFT);
    let r = h.region.expect("region");
    assert!((r.start_sec - 22.0).abs() < 0.6 && (r.end_sec - 26.0).abs() < 0.6);
    assert_eq!(h.view.scroll_sec, 20.0);
}

#[test]
fn ctrl_snaps_region_edges_to_measure_lines() {
    let mut h = Harness::new();
    h.drag(91.0, 96.7, Modifiers::COMMAND);
    let r = h.region.unwrap();
    assert_eq!((r.start_sec, r.end_sec), (90.0, 96.0), "{r:?}");
}

#[test]
fn without_ctrl_nothing_snaps() {
    let mut h = Harness::new();
    h.drag(91.0, 96.7, Modifiers::NONE);
    let r = h.region.unwrap();
    assert!(r.start_sec % 2.0 != 0.0 || r.end_sec % 2.0 != 0.0, "{r:?}");
}

#[test]
fn the_widget_ignores_the_wheel() {
    let mut h = Harness::new();
    let at = h.pos(60.0);
    h.mouse(at, Modifiers::NONE);
    let wheel = Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 3.0), modifiers: Modifiers::NONE };
    assert!(h.frame(vec![wheel], Modifiers::NONE).is_empty());
    assert_eq!(h.view, TimelineView::new(0.0, 100.0));
}

#[test]
fn zero_length_audio_draws_and_reports_nothing() {
    let ctx = Context::default();
    let view = TimelineView::default();
    let mut region = None;
    for _ in 0..2 {
        let _ = ctx.run(RawInput::default(), |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let out = OverviewTimeline::new(0.0, &view, MAIN_W, |_, n| vec![(0.0, 0.0, 0.0); n]).show(ui, &mut region);
                assert!(out.events.is_empty());
            });
        });
    }
}
