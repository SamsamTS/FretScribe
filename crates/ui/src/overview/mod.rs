//! Overview timeline: the whole audio in a compact strip, with the visible part of the main view,
//! the playhead and the region selection. It only reads the shared `TimelineView`; the app applies
//! the returned events.
//!
//! Interactions on the strip (see `logic::hit_test` for the priority):
//! - click: seek; double-click: clear the selection
//! - drag the viewport rectangle: pan the main view; drag its edges: zoom it
//! - drag elsewhere (or Shift+drag anywhere): create a region; drag its handles to resize it;
//!   drag inside it (Alt+drag when it is under the viewport) to move it
//! - Ctrl (Cmd on macOS) while dragging, or `snap_region(true)`: region edges snap to the nearest
//!   measure line

mod logic;
mod ruler;

#[cfg(test)]
mod headless;

use std::ops::Range;

use egui::{
    pos2, vec2, Align2, Button, Color32, CursorIcon, FontId, Mesh, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

use crate::grid::LineBatch;
use crate::view::{RegionSelection, TimelineView};

pub use logic::{
    drag_update, hit_test, pan, resize_left, resize_right, snap_to, viewport, viewport_x,
    DragContext, DragMode, DragState, DragUpdate, Keys, OverviewMap, HANDLE_PX, MIN_VIEWPORT_PX,
};
pub use ruler::{format_time, tick_step, ticks};

const RULER_HEIGHT: f32 = 16.0;
const DEFAULT_WAVEFORM_HEIGHT: f32 = 44.0;
const MIN_MEASURE_LINE_SPACING_PX: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverviewEvent {
    SeekRequested(f64),
    RegionChanged(Option<RegionSelection>),
    ViewportChanged { scroll_sec: f64, pixels_per_sec: f64 },
    PlayRegionRequested,
    LoopToggled(bool),
}

pub struct OverviewOutput {
    pub events: Vec<OverviewEvent>,
    /// Where the strip (ruler and waveform, without the buttons) was drawn.
    pub strip: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverviewStyle {
    pub background: Color32,
    pub ruler_background: Color32,
    pub ruler_text: Color32,
    pub tick: Color32,
    pub tick_minor: Color32,
    pub envelope: Color32,
    pub body: Color32,
    pub measure_line: Color32,
    pub region_fill: Color32,
    pub region_edge: Color32,
    pub viewport_fill: Color32,
    pub viewport_edge: Color32,
    pub playhead: Color32,
}

impl Default for OverviewStyle {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(16, 20, 28),
            ruler_background: Color32::from_rgb(26, 31, 42),
            ruler_text: Color32::from_rgb(170, 178, 196),
            tick: Color32::from_rgb(120, 130, 152),
            tick_minor: Color32::from_rgb(64, 72, 90),
            envelope: Color32::from_rgb(70, 110, 160),
            body: Color32::from_rgb(120, 175, 235),
            measure_line: Color32::from_rgba_unmultiplied(220, 226, 240, 40),
            region_fill: Color32::from_rgba_unmultiplied(255, 190, 70, 60),
            region_edge: Color32::from_rgb(255, 200, 90),
            viewport_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            viewport_edge: Color32::from_rgb(230, 235, 245),
            playhead: Color32::from_rgb(255, 90, 90),
        }
    }
}

/// `peaks(time_range, pixel_width)` returns exactly `pixel_width` columns of `(min, max, rms)`,
/// amplitudes in -1..1; it is called once per frame for the whole file.
pub struct OverviewTimeline<'a, P>
where
    P: Fn(Range<f64>, usize) -> Vec<(f32, f32, f32)>,
{
    audio_len_sec: f64,
    view: &'a TimelineView,
    main_width_px: f32,
    peaks: P,
    playhead_sec: f64,
    measure_starts: &'a [f64],
    snap_points: Option<&'a [f64]>,
    snap_region: bool,
    looping: bool,
    waveform_height: f32,
    style: OverviewStyle,
}

impl<'a, P> OverviewTimeline<'a, P>
where
    P: Fn(Range<f64>, usize) -> Vec<(f32, f32, f32)>,
{
    /// `main_width_px` is the width of the main view that `view` is shown in; it gives the length
    /// of the visible part.
    pub fn new(audio_len_sec: f64, view: &'a TimelineView, main_width_px: f32, peaks: P) -> Self {
        Self {
            audio_len_sec,
            view,
            main_width_px,
            peaks,
            playhead_sec: 0.0,
            measure_starts: &[],
            snap_points: None,
            snap_region: false,
            looping: false,
            waveform_height: DEFAULT_WAVEFORM_HEIGHT,
            style: OverviewStyle::default(),
        }
    }

    pub fn playhead(mut self, sec: f64) -> Self {
        self.playhead_sec = sec;
        self
    }

    /// Drawn as faint lines, and the snap targets unless `snap_points` is given.
    pub fn measure_starts(mut self, starts: &'a [f64]) -> Self {
        self.measure_starts = starts;
        self
    }

    /// Snap targets other than the measure starts (for instance measures and beats).
    pub fn snap_points(mut self, points: &'a [f64]) -> Self {
        self.snap_points = Some(points);
        self
    }

    /// Always snap region edges, instead of only while Ctrl is held.
    pub fn snap_region(mut self, always: bool) -> Self {
        self.snap_region = always;
        self
    }

    /// Current state of the loop button.
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    pub fn waveform_height(mut self, height: f32) -> Self {
        self.waveform_height = height.max(8.0);
        self
    }

    pub fn style(mut self, style: OverviewStyle) -> Self {
        self.style = style;
        self
    }

    pub fn show(self, ui: &mut Ui, region: &mut Option<RegionSelection>) -> OverviewOutput {
        let mut events = Vec::new();
        let mut strip = Rect::NOTHING;
        ui.horizontal(|ui| {
            ui.vertical(|ui| self.buttons(ui, region.is_some(), &mut events));
            let size = vec2(ui.available_width().max(1.0), RULER_HEIGHT + self.waveform_height);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            strip = rect;
            if self.audio_len_sec > 0.0 && self.audio_len_sec.is_finite() {
                self.interact(ui, rect, region, &mut events);
                self.paint(ui, rect, *region);
            }
        });
        OverviewOutput { events, strip }
    }

    fn buttons(&self, ui: &mut Ui, has_region: bool, events: &mut Vec<OverviewEvent>) {
        let loop_button = ui
            .add(Button::new("Loop").selected(self.looping))
            .on_hover_text("Repeat the selection (or the whole file when nothing is selected)");
        if loop_button.clicked() {
            events.push(OverviewEvent::LoopToggled(!self.looping));
        }
        let play = ui
            .add_enabled(has_region, Button::new("Play selection"))
            .on_hover_text("Play the selected region from its start")
            .on_disabled_hover_text("Select a region first: drag on the overview");
        if play.clicked() {
            events.push(OverviewEvent::PlayRegionRequested);
        }
    }

    fn snap_targets(&self, always: bool, ctrl: bool) -> Option<&'a [f64]> {
        let points = self.snap_points.unwrap_or(self.measure_starts);
        ((always || ctrl) && !points.is_empty()).then_some(points)
    }

    fn interact(
        &self,
        ui: &mut Ui,
        rect: Rect,
        region: &mut Option<RegionSelection>,
        events: &mut Vec<OverviewEvent>,
    ) {
        let ctx = ui.ctx().clone();
        let id = ui.id().with("overview_strip");
        let drag_id = id.with("drag");
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let map = OverviewMap { audio_len_sec: self.audio_len_sec, width_px: rect.width() };
        let vp = viewport(self.view, self.main_width_px, self.audio_len_sec);
        let vp_x = viewport_x(&map, vp);
        let (keys, ctrl) = ui.input(|i| (Keys { shift: i.modifiers.shift, alt: i.modifiers.alt }, i.modifiers.command));
        let sec_at = |p: egui::Pos2| map.x_to_sec(p.x - rect.left());

        let mut state = ctx.data(|d| d.get_temp::<DragState>(drag_id));
        if response.drag_started() {
            let origin = ui.input(|i| i.pointer.press_origin()).or_else(|| response.interact_pointer_pos());
            if let Some(origin) = origin {
                let mode = hit_test(origin.x - rect.left(), &map, *region, vp_x, keys);
                state = Some(DragState { mode, origin_sec: sec_at(origin), region: *region, view: *self.view });
            }
        }
        if let (Some(drag), true) = (state, response.dragged()) {
            if let Some(pointer) = ui.input(|i| i.pointer.latest_pos()) {
                let context = DragContext {
                    audio_len_sec: self.audio_len_sec,
                    main_width_px: self.main_width_px,
                    snap: self.snap_targets(self.snap_region, ctrl),
                };
                match drag_update(&drag, sec_at(pointer), &context) {
                    DragUpdate::Region(new) if *region != Some(new) => {
                        *region = Some(new);
                        events.push(OverviewEvent::RegionChanged(Some(new)));
                    }
                    DragUpdate::Viewport(new) if new != *self.view => {
                        events.push(OverviewEvent::ViewportChanged {
                            scroll_sec: new.scroll_sec,
                            pixels_per_sec: new.pixels_per_sec,
                        });
                    }
                    _ => {}
                }
            }
        }
        match state {
            Some(_) if response.drag_stopped() => ctx.data_mut(|d| d.remove::<DragState>(drag_id)),
            Some(drag) => ctx.data_mut(|d| d.insert_temp(drag_id, drag)),
            None => {}
        }

        if response.double_clicked() {
            if region.take().is_some() {
                events.push(OverviewEvent::RegionChanged(None));
            }
        } else if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                events.push(OverviewEvent::SeekRequested(sec_at(pos)));
            }
        }

        let hover_mode = state.map(|d| d.mode).or_else(|| {
            response
                .hover_pos()
                .map(|p| hit_test(p.x - rect.left(), &map, *region, vp_x, keys))
        });
        if let Some(mode) = hover_mode {
            ctx.set_cursor_icon(match mode {
                DragMode::CreateRegion => CursorIcon::Crosshair,
                DragMode::RegionStart | DragMode::RegionEnd | DragMode::ViewportLeft | DragMode::ViewportRight => {
                    CursorIcon::ResizeHorizontal
                }
                DragMode::MoveRegion | DragMode::PanViewport => CursorIcon::Grab,
            });
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, region: Option<RegionSelection>) {
        let style = &self.style;
        let ppp = ui.ctx().pixels_per_point();
        let painter = ui.painter_at(rect);
        let map = OverviewMap { audio_len_sec: self.audio_len_sec, width_px: rect.width() };
        let x_of = |sec: f64| rect.left() + map.sec_to_x(sec);
        let ruler = Rect::from_min_size(rect.min, vec2(rect.width(), RULER_HEIGHT));
        let wave = Rect::from_min_max(pos2(rect.left(), ruler.bottom()), rect.max);

        painter.rect_filled(rect, 0.0, style.background);
        painter.rect_filled(ruler, 0.0, style.ruler_background);
        painter.add(self.waveform(wave, ppp));

        let mut lines = LineBatch::new(ppp);
        let mut last_x = f32::NEG_INFINITY;
        for &start in self.measure_starts {
            let x = x_of(start);
            if x - last_x >= MIN_MEASURE_LINE_SPACING_PX && x >= rect.left() && x <= rect.right() {
                lines.solid(x, wave.top(), wave.bottom(), 1.0, style.measure_line);
                last_x = x;
            }
        }
        painter.add(lines.into_shape());

        self.ruler(&painter, ruler, &map, ppp);

        if let Some(region) = region {
            let (left, right) = (x_of(region.start_sec), x_of(region.end_sec));
            painter.rect_filled(
                Rect::from_min_max(pos2(left, rect.top()), pos2(right.max(left + 1.0), rect.bottom())),
                0.0,
                style.region_fill,
            );
            let mut edges = LineBatch::new(ppp);
            for x in [left, right] {
                edges.solid(x, rect.top(), rect.bottom(), 2.0, style.region_edge);
            }
            painter.add(edges.into_shape());
            for x in [left, right] {
                let grip = Rect::from_center_size(pos2(x, ruler.center().y), vec2(6.0, RULER_HEIGHT - 4.0));
                painter.rect_filled(grip, 2.0, style.region_edge);
            }
        }

        let vp = viewport(self.view, self.main_width_px, self.audio_len_sec);
        let (vl, vr) = viewport_x(&map, vp);
        let vp_rect = Rect::from_min_max(pos2(rect.left() + vl, rect.top()), pos2(rect.left() + vr, rect.bottom()));
        painter.rect_filled(vp_rect, 0.0, style.viewport_fill);
        painter.rect_stroke(vp_rect, 0.0, Stroke::new(1.0_f32, style.viewport_edge), StrokeKind::Inside);
        let mut handles = LineBatch::new(ppp);
        for x in [vp_rect.left() + 1.0, vp_rect.right() - 1.0] {
            handles.solid(x, rect.top(), rect.bottom(), 3.0, style.viewport_edge);
        }
        painter.add(handles.into_shape());

        let mut playhead = LineBatch::new(ppp);
        playhead.solid(x_of(self.playhead_sec.clamp(0.0, self.audio_len_sec)), rect.top(), rect.bottom(), 2.0, style.playhead);
        painter.add(playhead.into_shape());
    }

    fn waveform(&self, wave: Rect, ppp: f32) -> Shape {
        let mut mesh = Mesh::default();
        let columns = (wave.width() * ppp).round().max(1.0) as usize;
        let peaks = (self.peaks)(0.0..self.audio_len_sec, columns);
        let (mid, half) = (wave.center().y, wave.height() / 2.0 - 1.0);
        let one_px = 1.0 / ppp;
        let clamp = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        for (i, &(min, max, rms)) in peaks.iter().take(columns).enumerate() {
            let x = wave.left() + i as f32 * one_px;
            let (min, max, rms) = (clamp(min), clamp(max), clamp(rms).abs());
            let top = mid - max.max(0.0) * half;
            let bottom = mid - min.min(0.0) * half;
            mesh.add_colored_rect(
                Rect::from_min_max(pos2(x, top.min(mid - one_px / 2.0)), pos2(x + one_px, bottom.max(mid + one_px / 2.0))),
                self.style.envelope,
            );
            if rms > 0.0 {
                mesh.add_colored_rect(
                    Rect::from_min_max(pos2(x, mid - rms * half), pos2(x + one_px, mid + rms * half)),
                    self.style.body,
                );
            }
        }
        Shape::mesh(mesh)
    }

    fn ruler(&self, painter: &egui::Painter, ruler: Rect, map: &OverviewMap, ppp: f32) {
        let style = &self.style;
        let pps = f64::from(map.width_px) / self.audio_len_sec;
        let step = tick_step(pps, ruler::MIN_LABEL_SPACING_PX);
        let minor = ruler::minor_divisions(step, pps);
        let font = FontId::proportional(10.0);
        let (mut major_ticks, mut minor_ticks) = (LineBatch::new(ppp), LineBatch::new(ppp));
        for tick in ticks(self.audio_len_sec, step) {
            let x = ruler.left() + map.sec_to_x(tick);
            major_ticks.solid(x, ruler.bottom() - 7.0, ruler.bottom(), 1.0, style.tick);
            painter.text(pos2(x + 3.0, ruler.top() + 2.0), Align2::LEFT_TOP, format_time(tick), font.clone(), style.ruler_text);
            for k in 1..minor {
                let t = tick + step * f64::from(k) / f64::from(minor);
                if t < self.audio_len_sec {
                    let x = ruler.left() + map.sec_to_x(t);
                    minor_ticks.solid(x, ruler.bottom() - 3.0, ruler.bottom(), 1.0, style.tick_minor);
                }
            }
        }
        painter.add(minor_ticks.into_shape());
        painter.add(major_ticks.into_shape());
    }
}
