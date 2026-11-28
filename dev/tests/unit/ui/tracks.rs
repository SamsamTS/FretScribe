use egui::{CentralPanel, Context, Event, PointerButton, Pos2, RawInput, Rect};
use fretscribe_analysis::{Levels, Lut, Spectrogram};
use fretscribe_core::{NoteTimes, Project};
use std::collections::HashSet;
use std::sync::Arc;

use super::*;
use crate::note_editor::{header_tools, ActionButton};
use crate::spectro::SpectroStyle;
use crate::view::{PitchView, TimelineView, LANE_HEADER_WIDTH};

const PITCH: u32 = 67;

fn project() -> Project {
    fretscribe_core::testkit::band(20.0)
}

#[test]
fn display_follows_core_audibility() {
    let mut p = project();
    assert_eq!(audible_set(&p), HashSet::from([1, 2, 3]));
    p.tracks[1].muted = true;
    assert_eq!(audible_set(&p), HashSet::from([1, 3]));
    p.tracks[2].solo = true;
    assert_eq!(audible_set(&p), HashSet::from([3]));
    p.tracks[1].muted = false;
    p.tracks[1].solo = true;
    p.tracks[2].muted = true;
    assert_eq!(audible_set(&p), HashSet::from([2, 3]), "solo wins over mute");
    assert!(opacity_for(true) > opacity_for(false));
}

struct Silent;

impl LaneSource for Silent {
    fn spectrogram(&self, _: u64) -> Option<&Spectrogram> {
        None
    }
    fn peaks(&self, _: u64, _: (f64, f64), columns: usize) -> Vec<crate::spectro::Peak> {
        vec![(-0.5, 0.5, 0.2); columns]
    }
    fn duration_sec(&self, _: u64) -> f64 {
        20.0
    }
    fn onsets(&self, _: u64) -> &[f64] {
        &[]
    }
    fn snap_candidates(&self, _: u64, _: f64, _: f64) -> Vec<f64> {
        Vec::new()
    }
}

struct Harness {
    ctx: Context,
    project: Project,
    state: TrackLanesState,
    time: f64,
    rows: f64,
    bottom: f32,
    tools: Option<Vec<ActionButton>>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self { ctx: Context::default(), project: project(), state: TrackLanesState::default(), time: 0.0, rows: 0.0, bottom: 700.0, tools: None };
        h.frame(Vec::new());
        h
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<TrackEvent> {
        self.time += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0))), time: Some(self.time), events, ..Default::default() };
        let style = SpectroStyle { levels: Levels::default(), gamma: 1.0, lut: Arc::new(Lut::builtin()) };
        let times = NoteTimes::default();
        let (rows, bottom) = (std::mem::take(&mut self.rows), self.bottom);
        let mut out = Vec::new();
        let _ = self.ctx.run(raw, |ctx| {
            CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                let lanes = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, bottom));
                let o = TrackLanes { project: &self.project, source: &Silent, style: &style, times: &times, selected: &[], ghost: None, reference_track: 1, focus: None, tools: self.tools.as_deref(), region: None, region_snap: &[], region_snapping: false, audio_len_sec: 20.0, wheel_rows: rows }
                    .show(ui, lanes, &TimelineView::new(0.0, 100.0), &PitchView::default(), 0.0, &mut self.state);
                out = o.events;
            });
        });
        out
    }

    fn click(&mut self, pos: Pos2) -> Vec<TrackEvent> {
        let button = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)])
    }
}

fn lane_top(h: &Harness, index: usize) -> f32 {
    let (strips, _) = h.state.strips(&h.ctx, &h.project);
    strips[index].top_px as f32 / h.ctx.pixels_per_point()
}

#[test]
fn mute_button_and_add_row_emit_events() {
    let mut h = Harness::new();
    let top = lane_top(&h, 1);
    let mute = Pos2::new(4.0 + 3.0 + 18.0 + 13.0, top + 2.0 + 30.0 + 15.0);
    assert_eq!(h.click(mute), vec![TrackEvent::MuteToggled(2)]);
    for t in &mut h.project.tracks {
        t.spectrogram_collapsed = true;
    }
    for _ in 0..30 {
        h.frame(Vec::new());
    }
    let (_, content) = h.state.strips(&h.ctx, &h.project);
    // The button sits in the middle of the last row, inside the header column.
    let row_middle = (content - PITCH) as f32 + PITCH as f32 / 2.0;
    assert!(h.click(Pos2::new(100.0, row_middle)).contains(&TrackEvent::AddTrackRequested));
    assert!(!h.click(Pos2::new(LANE_HEADER_WIDTH + 100.0, row_middle)).contains(&TrackEvent::AddTrackRequested), "only the button adds a track, not the whole row");
}

#[test]
fn collapsing_shrinks_the_lane_in_whole_rows() {
    let mut h = Harness::new();
    let (before, _) = h.state.strips(&h.ctx, &h.project);
    assert_eq!(before[0].rows, SPECTRO_ROWS);
    h.project.tracks[0].spectrogram_collapsed = true;
    let mut seen = Vec::new();
    for _ in 0..30 {
        h.frame(Vec::new());
        seen.push(h.state.strips(&h.ctx, &h.project).0[0].rows);
    }
    assert!(seen.windows(2).all(|w| w[0] >= w[1]));
    assert_eq!(*seen.last().unwrap(), 0);
    let (after, content) = h.state.strips(&h.ctx, &h.project);
    assert_eq!(after[1].top_px, PITCH);
    assert!(after.iter().all(|s| s.top_px % PITCH == 0) && content % PITCH == 0);
}

#[test]
fn one_alt_wheel_notch_from_the_nav_resizes_the_lane_by_a_row_with_animation() {
    let mut h = Harness::new();
    let pos = Pos2::new(500.0, 100.0);
    h.frame(vec![Event::PointerMoved(pos)]);
    h.rows = 1.0;
    h.frame(Vec::new());
    assert_eq!(h.state.configured_rows(1), SPECTRO_ROWS + 1);
    let mut seen = vec![h.state.strips(&h.ctx, &h.project).0[0].rows];
    for _ in 0..30 {
        h.frame(Vec::new());
        seen.push(h.state.strips(&h.ctx, &h.project).0[0].rows);
    }
    assert!(seen.windows(2).all(|w| w[0] <= w[1]) && *seen.last().unwrap() == SPECTRO_ROWS + 1, "{seen:?}");
    h.rows = -1.0;
    h.frame(Vec::new());
    assert_eq!(h.state.configured_rows(1), SPECTRO_ROWS);
}

#[test]
fn reordering_slides_lanes_to_their_new_place() {
    let mut h = Harness::new();
    let first_top = h.state.strips(&h.ctx, &h.project).0[0].top_px as f32;
    let track = h.project.tracks.remove(0);
    h.project.tracks.push(track);
    h.frame(Vec::new());
    assert!(h.state.slide_px.get(&1).is_some_and(|&off| off < 0.0), "{:?}", h.state.slide_px);
    for _ in 0..60 {
        h.frame(Vec::new());
    }
    assert!(h.state.slide_px.is_empty());
    assert!(first_top < h.state.strips(&h.ctx, &h.project).0[2].top_px as f32);
}

#[test]
fn what_is_scrolled_out_of_the_lanes_ignores_the_pointer() {
    for (bottom_margin, reacts) in [(100.0, true), (-30.0, false)] {
        let mut h = Harness::new();
        // The middle of the first lane's waveform, below or above the bottom of the lanes.
        let y = lane_top(&h, 1) - PITCH as f32 / 2.0;
        h.bottom = y + bottom_margin;
        let (from, to) = (Pos2::new(500.0, y), Pos2::new(600.0, y));
        let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Default::default() };
        h.frame(vec![Event::PointerMoved(from)]);
        let mut events = h.frame(vec![button(from, true)]);
        events.extend(h.frame(vec![Event::PointerMoved(to)]));
        events.extend(h.frame(vec![button(to, false)]));
        assert_eq!(events.iter().any(|e| matches!(e, TrackEvent::RegionChanged(_))), reacts, "{events:?}");
    }
}

/// Interactive widgets of the header column, as laid out by the last frame.
fn header_widgets(h: &Harness) -> Vec<Rect> {
    h.ctx.viewport(|v| {
        v.prev_pass.widgets.layers().flat_map(|(_, widgets)| widgets).filter(|w| w.sense.interactive() && w.rect.right() <= LANE_HEADER_WIDTH).map(|w| w.rect).collect()
    })
}

#[test]
fn header_controls_fit_their_strip_at_every_display_scale() {
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        let mut h = Harness::new();
        h.ctx.set_pixels_per_point(ppp);
        h.tools = Some(header_tools(&h.project, &Default::default(), &Default::default(), false));
        let pitch = strip_pitch_px(ppp) as f32 / ppp;
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        // The source lane, expanded: the tool grid starts below the first strip's controls.
        let widgets = header_widgets(&h);
        let (tools, controls): (Vec<Rect>, Vec<Rect>) = widgets.iter().filter(|r| r.top() < lane_top(&h, 1) && r.height() > 8.0).partition(|r| r.height() == crate::kit::BUTTON && r.width() == crate::kit::BUTTON);
        assert!(!tools.is_empty() && controls.len() >= 8, "{ppp}: {widgets:?}");
        for c in &controls {
            assert!(c.bottom() <= pitch && tools.iter().all(|t| !t.intersects(*c)), "{ppp}: {c:?}");
        }
        h.project.tracks.iter_mut().for_each(|t| t.spectrogram_collapsed = true);
        for _ in 0..30 {
            h.frame(Vec::new());
        }
        let tracks = h.project.tracks.len() as f32;
        let in_lanes: Vec<Rect> = header_widgets(&h).into_iter().filter(|r| r.top() < tracks * pitch).collect();
        assert!(in_lanes.len() >= 8 * tracks as usize, "{ppp}: {in_lanes:?}");
        for r in in_lanes {
            let top = (r.center().y / pitch).floor() * pitch;
            assert!(r.top() >= top && r.bottom() <= top + pitch - 1.0 / ppp, "{ppp}: {r:?} leaves its lane");
        }
        let (strips, content) = h.state.strips(&h.ctx, &h.project);
        assert!(strips.iter().all(|s| s.top_px % strip_pitch_px(ppp) == 0) && content % strip_pitch_px(ppp) == 0);
    }
}
