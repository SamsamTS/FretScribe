//! Drives the widget through egui frames with synthetic input, without a window.

use egui::epaint::ClippedShape;
use egui::{CentralPanel, Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};

use super::*;

const LEN: f64 = 120.0;
const MAIN_W: f32 = 800.0;
const LEFT: PointerButton = PointerButton::Primary;
const RIGHT: PointerButton = PointerButton::Secondary;

struct Harness {
    ctx: Context,
    view: TimelineView,
    region: Option<RegionSelection>,
    playhead: f64,
    measures: Vec<f64>,
    palette: Option<WaveformPalette>,
    time: f64,
    strip: Rect,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            ctx: Context::default(),
            view: TimelineView::new(0.0, 100.0),
            region: None,
            playhead: 10.0,
            measures: (0..60).map(|k| f64::from(k) * 2.0).collect(),
            palette: None,
            time: 0.0,
            strip: Rect::NOTHING,
        };
        h.frame(Vec::new(), Modifiers::NONE);
        h.frame(Vec::new(), Modifiers::NONE);
        h
    }

    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<OverviewEvent> {
        self.frame_shapes(events, modifiers).0
    }

    fn frame_shapes(&mut self, events: Vec<Event>, modifiers: Modifiers) -> (Vec<OverviewEvent>, Vec<ClippedShape>) {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 200.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let (view, measures, playhead) = (self.view, self.measures.clone(), self.playhead);
        let mut region = self.region;
        let mut out = Vec::new();
        let mut strip = self.strip;
        let palette = self.palette.clone();
        let full = self.ctx.run(raw, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let peaks = |range: Range<f64>, n: usize| {
                    (0..n)
                        .map(|i| {
                            let t = range.start + (range.end - range.start) * i as f64 / n as f64;
                            let a = 0.2 + (t * 0.7).sin().abs() as f32 * 0.6;
                            (-a, a, a * 0.5)
                        })
                        .collect()
                };
                let mut widget = OverviewTimeline::new(LEN, &view, MAIN_W, peaks)
                    .measure_starts(&measures)
                    .playhead(playhead);
                if let Some(palette) = palette.clone() {
                    widget = widget.palette(palette);
                }
                let output = widget.show(ui, &mut region);
                out = output.events;
                strip = output.strip;
            });
        });
        self.region = region;
        self.strip = strip;
        for event in &out {
            match *event {
                OverviewEvent::ViewportChanged { scroll_sec, pixels_per_sec } => {
                    self.view = TimelineView::new(scroll_sec, pixels_per_sec);
                }
                OverviewEvent::SeekRequested(sec) => self.playhead = sec,
                _ => {}
            }
        }
        (out, full.shapes)
    }

    fn pos(&self, sec: f64) -> Pos2 {
        let map = OverviewMap { audio_len_sec: LEN, width_px: self.strip.width() };
        Pos2::new(self.strip.left() + map.sec_to_x(sec), self.strip.top() + 30.0)
    }

    fn ruler_pos(&self, sec: f64) -> Pos2 {
        Pos2::new(self.pos(sec).x, self.strip.top() + 8.0)
    }

    fn mouse(&mut self, pos: Pos2, m: Modifiers) -> Vec<OverviewEvent> {
        self.frame(vec![Event::PointerMoved(pos)], m)
    }

    fn button(&mut self, pos: Pos2, button: PointerButton, pressed: bool, m: Modifiers) -> Vec<OverviewEvent> {
        self.frame(vec![Event::PointerButton { pos, button, pressed, modifiers: m }], m)
    }

    fn click(&mut self, pos: Pos2, button: PointerButton) -> Vec<OverviewEvent> {
        let mut events = self.mouse(pos, Modifiers::NONE);
        events.extend(self.button(pos, button, true, Modifiers::NONE));
        events.extend(self.button(pos, button, false, Modifiers::NONE));
        events.extend(self.frame(Vec::new(), Modifiers::NONE));
        events
    }

    fn drag_between(&mut self, from: Pos2, to: Pos2, button: PointerButton, m: Modifiers) -> Vec<OverviewEvent> {
        let mut events = self.mouse(from, m);
        events.extend(self.button(from, button, true, m));
        for step in 1..=5 {
            let t = step as f32 / 5.0;
            events.extend(self.mouse(Pos2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t), m));
        }
        events.extend(self.button(to, button, false, m));
        events.extend(self.frame(Vec::new(), m));
        events
    }

    fn drag(&mut self, from_sec: f64, to_sec: f64, button: PointerButton, m: Modifiers) -> Vec<OverviewEvent> {
        let (from, to) = (self.pos(from_sec), self.pos(to_sec));
        self.drag_between(from, to, button, m)
    }

    fn ruler_bottom(&self) -> f32 {
        self.strip.top() + RULER_HEIGHT
    }
}

fn last_region(events: &[OverviewEvent]) -> Option<RegionSelection> {
    events.iter().rev().find_map(|e| match e {
        OverviewEvent::RegionChanged(r) => *r,
        _ => None,
    })
}

fn seeks(events: &[OverviewEvent]) -> Vec<f64> {
    events
        .iter()
        .filter_map(|e| match e {
            OverviewEvent::SeekRequested(sec) => Some(*sec),
            _ => None,
        })
        .collect()
}

#[test]
fn left_click_moves_the_seek_point() {
    let mut h = Harness::new();
    let target = h.pos(60.0);
    let events = h.click(target, LEFT);
    let [OverviewEvent::SeekRequested(sec)] = events[..] else { panic!("{events:?}") };
    assert!((sec - 60.0).abs() < 0.5, "{sec}");
    assert!(h.region.is_none());
}

#[test]
fn left_drag_scrubs_the_seek_point() {
    let mut h = Harness::new();
    let events = h.drag(70.0, 100.0, LEFT, Modifiers::NONE);
    let moves = seeks(&events);
    assert!(moves.len() >= 3, "{events:?}");
    assert!(moves.windows(2).all(|w| w[1] >= w[0]), "{moves:?}");
    assert!((moves[moves.len() - 1] - 100.0).abs() < 0.6, "{moves:?}");
    assert!(h.region.is_none() && last_region(&events).is_none());
    assert_eq!(h.view, TimelineView::new(0.0, 100.0));
}

#[test]
fn left_drag_past_the_strip_end_clamps_to_the_audio() {
    let mut h = Harness::new();
    let from = h.pos(100.0);
    let to = Pos2::new(h.strip.right() + 200.0, from.y);
    let events = h.drag_between(from, to, LEFT, Modifiers::NONE);
    assert_eq!(*seeks(&events).last().unwrap(), LEN);
}

#[test]
fn left_drag_on_the_viewport_box_pans_the_main_view() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    let events = h.drag(24.0, 34.0, LEFT, Modifiers::NONE);
    assert!(h.region.is_none());
    assert!((h.view.scroll_sec - 30.0).abs() < 0.8, "{:?}", h.view);
    assert_eq!(h.view.pixels_per_sec, 100.0);
    assert!(events.iter().any(|e| matches!(e, OverviewEvent::ViewportChanged { .. })));
    assert!(seeks(&events).is_empty(), "{events:?}");
}

#[test]
fn left_drag_on_a_viewport_edge_zooms_the_main_view() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    h.drag(28.0, 48.0, LEFT, Modifiers::NONE);
    assert!((h.view.scroll_sec - 20.0).abs() < 1e-9);
    assert!((h.view.pixels_per_sec - 800.0 / 28.0).abs() < 1.5, "{:?}", h.view);
}

#[test]
fn a_plain_left_click_on_the_viewport_box_still_seeks() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    let target = h.pos(24.0);
    let events = h.click(target, LEFT);
    assert!(matches!(events[..], [OverviewEvent::SeekRequested(_)]), "{events:?}");
}

#[test]
fn the_ruler_band_seeks_even_above_the_viewport_box() {
    let mut h = Harness::new();
    h.view = TimelineView::new(20.0, 100.0);
    let events = h.drag_between(h.ruler_pos(22.0), h.ruler_pos(30.0), LEFT, Modifiers::NONE);
    assert!(!seeks(&events).is_empty() && h.view == TimelineView::new(20.0, 100.0), "{events:?}");
    assert!((*seeks(&events).last().unwrap() - 30.0).abs() < 0.6);
}

#[test]
fn right_drag_creates_a_region() {
    let mut h = Harness::new();
    let events = h.drag(90.0, 100.0, RIGHT, Modifiers::NONE);
    let r = last_region(&events).expect("region event");
    assert!((r.start_sec - 90.0).abs() < 0.6 && (r.end_sec - 100.0).abs() < 0.6, "{r:?}");
    assert_eq!(h.region, Some(r));
    assert!(seeks(&events).is_empty(), "{events:?}");
}

#[test]
fn right_drag_works_from_the_ruler_band_too() {
    let mut h = Harness::new();
    h.drag_between(h.ruler_pos(40.0), h.ruler_pos(50.0), RIGHT, Modifiers::NONE);
    let r = h.region.expect("region");
    assert!((r.start_sec - 40.0).abs() < 0.6 && (r.end_sec - 50.0).abs() < 0.6);
}

#[test]
fn right_drag_right_to_left_gives_an_ordered_region() {
    let mut h = Harness::new();
    h.drag(100.0, 70.0, RIGHT, Modifiers::NONE);
    let r = h.region.unwrap();
    assert!(r.start_sec < r.end_sec && (r.start_sec - 70.0).abs() < 0.6);
}

#[test]
fn a_single_right_click_clears_the_selection() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 40.0, end_sec: 50.0 });
    let at = h.pos(100.0);
    let events = h.click(at, RIGHT);
    assert!(h.region.is_none(), "{events:?}");
    assert_eq!(events, [OverviewEvent::RegionChanged(None)]);
    assert!(h.click(at, RIGHT).is_empty(), "nothing to clear the second time");
}

#[test]
fn left_double_click_does_not_clear_the_selection() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 40.0, end_sec: 50.0 });
    let at = h.pos(100.0);
    h.mouse(at, Modifiers::NONE);
    for _ in 0..2 {
        h.button(at, LEFT, true, Modifiers::NONE);
        h.button(at, LEFT, false, Modifiers::NONE);
    }
    assert!(h.region.is_some());
}

#[test]
fn right_handles_resize_and_the_body_moves() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 60.0, end_sec: 80.0 });
    h.drag(80.0, 90.0, RIGHT, Modifiers::NONE);
    let r = h.region.unwrap();
    assert!((r.start_sec - 60.0).abs() < 1e-9 && (r.end_sec - 90.0).abs() < 0.6, "{r:?}");
    h.drag(70.0, 75.0, RIGHT, Modifiers::NONE);
    let moved = h.region.unwrap();
    assert!((moved.len_sec() - r.len_sec()).abs() < 1e-9 && (moved.start_sec - 65.0).abs() < 0.8, "{moved:?}");
}

#[test]
fn left_drag_never_edits_the_region() {
    let mut h = Harness::new();
    let region = RegionSelection { start_sec: 60.0, end_sec: 80.0 };
    h.region = Some(region);
    h.drag(80.0, 90.0, LEFT, Modifiers::NONE);
    h.drag(70.0, 75.0, LEFT, Modifiers::NONE);
    assert_eq!(h.region, Some(region));
}

#[test]
fn shift_right_drag_starts_a_new_region_over_the_old_one() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 60.0, end_sec: 100.0 });
    h.drag(70.0, 75.0, RIGHT, Modifiers::SHIFT);
    let r = h.region.unwrap();
    assert!((r.start_sec - 70.0).abs() < 0.6 && (r.end_sec - 75.0).abs() < 0.6, "{r:?}");
}

#[test]
fn ctrl_snaps_region_edges_to_measure_lines() {
    let mut h = Harness::new();
    h.drag(91.0, 96.7, RIGHT, Modifiers::COMMAND);
    let r = h.region.unwrap();
    assert_eq!((r.start_sec, r.end_sec), (90.0, 96.0), "{r:?}");
}

#[test]
fn without_ctrl_nothing_snaps() {
    let mut h = Harness::new();
    h.drag(91.0, 96.7, RIGHT, Modifiers::NONE);
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

fn mesh_color(shape: &Shape) -> Option<(Color32, Rect)> {
    match shape {
        Shape::Mesh(mesh) if !mesh.vertices.is_empty() => Some((mesh.vertices[0].color, mesh.calc_bounds())),
        _ => None,
    }
}

#[test]
fn the_boxes_never_cover_the_ruler() {
    let mut h = Harness::new();
    h.region = Some(RegionSelection { start_sec: 30.0, end_sec: 50.0 });
    h.view = TimelineView::new(20.0, 100.0);
    let style = OverviewStyle::default();
    let (_, shapes) = h.frame_shapes(Vec::new(), Modifiers::NONE);
    let wave_top = h.ruler_bottom();
    let (mut region_parts, mut viewport_parts) = (0, 0);
    for clipped in &shapes {
        let (colors, top) = match &clipped.shape {
            Shape::Rect(r) => (vec![r.fill, r.stroke.color], r.rect.top()),
            other => match mesh_color(other) {
                Some((color, bounds)) => (vec![color], bounds.top()),
                None => continue,
            },
        };
        let is_region = colors.contains(&style.region_fill) || colors.contains(&style.region_edge);
        let is_viewport = colors.contains(&style.viewport_fill) || colors.contains(&style.viewport_edge);
        region_parts += usize::from(is_region);
        viewport_parts += usize::from(is_viewport);
        if is_region || is_viewport {
            assert!(top >= wave_top - 1e-3, "box part starts at {top}, ruler ends at {wave_top}");
        }
    }
    assert!(region_parts >= 3 && viewport_parts >= 3, "{region_parts} {viewport_parts}");
}

#[test]
fn the_waveform_uses_the_palette_and_sits_in_front_of_the_measure_lines() {
    let mut h = Harness::new();
    let wave_color = Color32::from_rgb(10, 200, 30);
    h.palette = Some(WaveformPalette::from_rgba(&[[10, 200, 30, 255]]));
    let style = OverviewStyle::default();
    let (_, shapes) = h.frame_shapes(Vec::new(), Modifiers::NONE);

    assert!(
        shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == wave_color)),
        "the area behind the waveform is the palette's lowest colour"
    );
    let waveform = shapes
        .iter()
        .position(|s| matches!(&s.shape, Shape::Mesh(m) if m.vertices.iter().all(|v| v.color == wave_color) && m.vertices.len() > 8))
        .expect("waveform mesh in the palette colour");
    let lines = shapes
        .iter()
        .position(|s| mesh_color(&s.shape).is_some_and(|(c, _)| c == style.measure_line))
        .expect("measure line mesh");
    assert!(lines < waveform, "measure lines ({lines}) are painted before the waveform ({waveform})");
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
