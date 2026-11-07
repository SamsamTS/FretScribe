use egui::{CentralPanel, Context, Event, PointerButton, Pos2, RawInput, Rect};
use fretscribe_analysis::{Levels, Lut, Spectrogram};
use fretscribe_core::{NoteTimes, Project, TimeSignature, Track, TrackRole};
use std::collections::HashSet;
use std::sync::Arc;

use super::*;
use crate::spectro::SpectroStyle;
use crate::view::{PitchView, TimelineView};

fn project() -> Project {
    let mut p = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 20.0, 120.0, TimeSignature::COMMON);
    p.add_track(Track::new(0, "Drums", TrackRole::Drums));
    p.add_track(Track::new(0, "Bass", TrackRole::Bass));
    p
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
}

impl Harness {
    fn new() -> Self {
        let mut h = Self { ctx: Context::default(), project: project(), state: TrackLanesState::default(), time: 0.0 };
        h.frame(Vec::new());
        h
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<TrackEvent> {
        self.time += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0))), time: Some(self.time), events, ..Default::default() };
        let style = SpectroStyle { levels: Levels::default(), gamma: 1.0, lut: Arc::new(Lut::builtin()) };
        let times = NoteTimes::default();
        let mut out = Vec::new();
        let _ = self.ctx.run(raw, |ctx| {
            CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                let lanes = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0));
                let o = TrackLanes { project: &self.project, source: &Silent, style: &style, times: &times, selected: &[], ghost: None, playhead_sec: 0.0, reference_track: 1 }
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
    let mute = Pos2::new(4.0 + 3.0 + 11.0, top + 2.0 + (STRIP_PITCH_PX as f32 - 4.0) / 3.0 * 1.5);
    assert_eq!(h.click(mute), vec![TrackEvent::MuteToggled(2)]);
    for t in &mut h.project.tracks {
        t.spectrogram_collapsed = true;
    }
    for _ in 0..30 {
        h.frame(Vec::new());
    }
    let (_, content) = h.state.strips(&h.ctx, &h.project);
    let plus = Pos2::new(100.0, (content - STRIP_PITCH_PX) as f32 + 10.0);
    assert!(h.click(plus).contains(&TrackEvent::AddTrackRequested));
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
    assert_eq!(after[1].top_px, STRIP_PITCH_PX);
    assert!(after.iter().all(|s| s.top_px % STRIP_PITCH_PX == 0) && content % STRIP_PITCH_PX == 0);
}

#[test]
fn one_alt_wheel_notch_resizes_the_lane_by_a_row_with_animation() {
    let mut h = Harness::new();
    let pos = Pos2::new(500.0, 100.0);
    h.frame(vec![Event::PointerMoved(pos)]);
    let alt = egui::Modifiers { alt: true, ..Default::default() };
    h.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 1.0), modifiers: alt }]);
    assert_eq!(h.state.configured_rows(1), SPECTRO_ROWS + 1);
    let mut seen = vec![h.state.strips(&h.ctx, &h.project).0[0].rows];
    for _ in 0..30 {
        h.frame(Vec::new());
        seen.push(h.state.strips(&h.ctx, &h.project).0[0].rows);
    }
    assert!(seen.windows(2).all(|w| w[0] <= w[1]) && *seen.last().unwrap() == SPECTRO_ROWS + 1, "{seen:?}");
    h.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, -1.0), modifiers: alt }]);
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
