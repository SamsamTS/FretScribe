//! Measure grid overlay: draws the tempo grid over a timeline rect and turns user edits into
//! `GridEvent`s. Wheel and navigation input belong to the navigation layer, not to this widget.

mod drag;
mod draw;
mod event;
mod header;
mod lines;
mod style;

use egui::{pos2, vec2, Area, Button, Context, CursorIcon, Frame, Id, Order, Rect, Response, Sense, Ui};
use fretscribe_core::{Measure, NotePolicy, SnapGrid, TempoMap, TimeSignature};

use crate::view::TimelineView;

pub use drag::{drag_limits, dragged_start};
pub use draw::{line_rect, Dots, LineBatch};
pub use event::GridEvent;
pub use header::{FitInfo, HeaderData};
pub use lines::{is_triplet, visible_lines, GridLine, LineKind, MIN_MAIN_BEAT_PX, MIN_SUBDIVISION_PX};
pub use style::GridStyle;

const HIT_HALF_WIDTH: f32 = 4.0;
const MIN_MOVE_SEC: f64 = 1e-6;
const DOTS: Dots = Dots { width_px: 1.0, dot_px: 2.0, gap_px: 3.0 };

#[derive(Clone, Copy, Debug, PartialEq)]
struct DragState {
    measure_id: u64,
    grab_offset_sec: f64,
    current_start_sec: f64,
}

pub struct GridOverlay<'a> {
    tempo: &'a TempoMap,
    view: &'a TimelineView,
    snap: &'a SnapGrid,
    policy: NotePolicy,
    style: GridStyle,
    autofit: bool,
    passthrough: Option<Rect>,
    header: Option<header::HeaderData<'a>>,
}

impl<'a> GridOverlay<'a> {
    pub fn new(tempo: &'a TempoMap, view: &'a TimelineView, snap: &'a SnapGrid) -> Self {
        Self {
            tempo,
            view,
            snap,
            policy: NotePolicy::default(),
            style: GridStyle::default(),
            autofit: false,
            passthrough: None,
            header: None,
        }
    }

    /// The application's current policy; holding Alt while dragging uses the other one.
    pub fn note_policy(mut self, policy: NotePolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn style(mut self, style: GridStyle) -> Self {
        self.style = style;
        self
    }

    /// Adds the auto-fit entries (whole grid: index 0, or from the clicked measure) to the menus,
    /// reported as `GridEvent::AutoFitRequested`.
    pub fn autofit(mut self, available: bool) -> Self {
        self.autofit = available;
        self
    }

    /// Adds the measure header band.
    pub fn with_header(mut self, data: HeaderData<'a>) -> Self {
        self.header = Some(data);
        self
    }

    /// An area (the note-drawing spectrogram) where the grid takes no mouse input at all, so notes
    /// can be drawn across measure lines.
    pub fn passthrough(mut self, area: Option<Rect>) -> Self {
        self.passthrough = area;
        self
    }

    pub fn show(self, ui: &mut Ui, rect: Rect) -> Vec<GridEvent> {
        let mut events = Vec::new();
        if self.tempo.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return events;
        }
        let ctx = ui.ctx().clone();
        let id = ui.id().with("measure_grid");
        let lines = visible_lines(self.tempo, self.view, self.snap, rect.width());

        let mut hovered = None;
        for line in lines.iter().filter(|l| l.kind == LineKind::Measure) {
            let Some(measure) = self.tempo.measures().get(line.measure) else { continue };
            let x = self.x_of(rect, line.sec);
            let hit = Rect::from_min_max(
                pos2(x - HIT_HALF_WIDTH, rect.top()),
                pos2(x + HIT_HALF_WIDTH, rect.bottom()),
            )
            .intersect(rect);
            for (k, hit) in self.hit_parts(hit).into_iter().enumerate() {
                let response = ui.interact(hit, id.with(("line", measure.id, k)), Sense::click_and_drag());
                let frozen = barline_locked(self.tempo.measures(), line.measure);
                if response.hovered() || response.dragged() {
                    hovered = Some(line.measure);
                    ctx.set_cursor_icon(if frozen { CursorIcon::NotAllowed } else { CursorIcon::ResizeHorizontal });
                }
                if !frozen {
                    self.track_drag(ui, id, rect, &response, line.measure, &mut events);
                }
                response.context_menu(|ui| self.context_menu(ui, line.measure, &mut events));
            }
        }
        let drag = ctx.data(|d| d.get_temp::<DragState>(id.with("drag")));
        if !ui.input(|i| i.pointer.any_down()) {
            ctx.data_mut(|d| d.remove::<DragState>(id.with("drag")));
        }

        let painter = ui.painter_at(rect);
        painter.add(self.line_shape(&ctx, rect, &lines, hovered, drag));
        if let Some(data) = self.header {
            self.header(ui, id, rect, data, &mut events);
        }
        if let Some(drag) = drag {
            self.drag_tooltip(ui, id, &drag);
        }
        events
    }

    /// `hit` minus the passthrough area: up to two vertical pieces.
    fn hit_parts(&self, hit: Rect) -> Vec<Rect> {
        let parts = match self.passthrough.filter(|p| p.intersects(hit)) {
            None => vec![hit],
            Some(p) => vec![
                Rect::from_min_max(hit.min, pos2(hit.max.x, p.top().max(hit.top()))),
                Rect::from_min_max(pos2(hit.min.x, p.bottom().min(hit.bottom())), hit.max),
            ],
        };
        parts.into_iter().filter(|r| r.width() > 0.0 && r.height() > 0.0).collect()
    }

    /// Only the measure lines, in the grid's own style and thickness, for panels that share the
    /// timeline (sheet, tab) but draw no barlines of their own. Takes no input.
    pub fn paint_measure_lines(painter: &egui::Painter, tempo: &TempoMap, view: &TimelineView, rect: Rect) {
        let style = GridStyle::default();
        let mut batch = LineBatch::new(painter.ctx().pixels_per_point());
        for m in tempo.measures() {
            let x = rect.left() + view.sec_to_x(m.start_sec);
            if x >= rect.left() && x <= rect.right() {
                batch.solid(x, rect.top(), rect.bottom(), 2.0, style.measure);
            }
        }
        painter.add(batch.into_shape());
    }

    fn x_of(&self, rect: Rect, sec: f64) -> f32 {
        rect.left() + self.view.sec_to_x(sec)
    }

    fn edit_policy(&self, ui: &Ui) -> NotePolicy {
        if ui.input(|i| i.modifiers.alt) {
            self.policy.inverted()
        } else {
            self.policy
        }
    }

    fn line_shape(
        &self,
        ctx: &Context,
        rect: Rect,
        lines: &[GridLine],
        hovered: Option<usize>,
        drag: Option<DragState>,
    ) -> egui::Shape {
        let style = &self.style;
        let mut batch = LineBatch::new(ctx.pixels_per_point());
        let (top, bottom) = (rect.top(), rect.bottom());
        let mut ordered = lines.to_vec();
        ordered.sort_by_key(|l| l.kind);
        for line in ordered {
            let x = self.x_of(rect, line.sec);
            match line.kind {
                LineKind::Straight => batch.solid(x, top, bottom, 1.0, style.straight),
                LineKind::Triplet => batch.dotted(x, top, bottom, DOTS, style.triplet),
                LineKind::MainBeat => batch.solid(x, top, bottom, 1.0, style.main_beat),
                LineKind::Measure if hovered == Some(line.measure) => {
                    batch.solid(x, top, bottom, 3.0, style.measure_active);
                }
                LineKind::Measure => batch.solid(x, top, bottom, 2.0, style.measure),
            }
        }
        if let Some(drag) = drag {
            batch.solid(self.x_of(rect, drag.current_start_sec), top, bottom, 2.0, style.ghost);
        }
        batch.into_shape()
    }

    fn track_drag(
        &self,
        ui: &Ui,
        id: Id,
        rect: Rect,
        response: &Response,
        index: usize,
        events: &mut Vec<GridEvent>,
    ) {
        let ctx = ui.ctx();
        let drag_id = id.with("drag");
        let Some(measure) = self.tempo.measures().get(index) else { return };

        if response.drag_started() {
            let origin = ui
                .input(|i| i.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            let Some(origin) = origin else { return };
            let state = DragState {
                measure_id: measure.id,
                grab_offset_sec: self.view.x_to_sec(origin.x - rect.left()) - measure.start_sec,
                current_start_sec: measure.start_sec,
            };
            ctx.data_mut(|d| d.insert_temp(drag_id, state));
        }
        let Some(mut state) = ctx.data(|d| d.get_temp::<DragState>(drag_id)) else { return };
        if state.measure_id != measure.id {
            return;
        }

        if response.dragged() {
            if let Some(pointer) = ui.input(|i| i.pointer.latest_pos()) {
                let pointer_sec = self.view.x_to_sec(pointer.x - rect.left());
                let limits = drag_limits(self.tempo.measures(), index);
                state.current_start_sec = dragged_start(pointer_sec, state.grab_offset_sec, limits);
                ctx.data_mut(|d| d.insert_temp(drag_id, state));
            }
        }
        if response.drag_stopped() {
            ctx.data_mut(|d| d.remove::<DragState>(drag_id));
            if (state.current_start_sec - measure.start_sec).abs() > MIN_MOVE_SEC {
                events.push(GridEvent::MeasureMoved {
                    index,
                    new_start_sec: state.current_start_sec,
                    policy: self.edit_policy(ui),
                });
            }
        }
    }

    fn drag_tooltip(&self, ui: &Ui, id: Id, drag: &DragState) {
        let ctx = ui.ctx();
        let Some(pointer) = ctx.input(|i| i.pointer.latest_pos()) else { return };
        let Some(index) = self.tempo.index_of(drag.measure_id) else { return };
        let mut text = format!("Measure {}: {}", index + 1, format_time(drag.current_start_sec));
        if self.edit_policy(ui) == NotePolicy::KeepAudioPosition {
            text.push_str(" (notes stay with the audio)");
        }
        Area::new(id.with("drag_tooltip"))
            .order(Order::Tooltip)
            .fixed_pos(pointer + vec2(14.0, 18.0))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
            });
    }

    fn context_menu(&self, ui: &mut Ui, index: usize, events: &mut Vec<GridEvent>) {
        if self.structure_menu(ui, index, events) {
            ui.close_menu();
        }
        if self.autofit {
            ui.separator();
            for (label, from) in [("Snap grid to detected beats", 0), ("Auto-fit from this measure", index)] {
                if ui.button(label).clicked() {
                    events.push(GridEvent::AutoFitRequested { index: from });
                    ui.close_menu();
                }
            }
        }
    }

    /// The entries shared by the barline menu and the header's overflow menu. True once one was picked.
    fn structure_menu(&self, ui: &mut Ui, index: usize, events: &mut Vec<GridEvent>) -> bool {
        let measures = self.tempo.measures();
        let Some(measure) = measures.get(index) else { return false };
        let policy = self.policy;
        let split = default_split_beat(measure.time_sig);
        let entries = [
            ("Insert measure here", true, Some(GridEvent::InsertMeasure { index, policy })),
            ("Delete measure", measures.len() > 1 && !measure.locked, Some(GridEvent::DeleteMeasure { index, policy })),
            ("Split measure", !measure.locked, split.map(|at_beat| GridEvent::SplitMeasure { index, at_beat })),
            ("Merge with previous", can_merge_with_previous(measures, index), index.checked_sub(1).map(|previous| GridEvent::MergeMeasures { index: previous })),
        ];
        let mut picked = false;
        for (label, enabled, event) in entries {
            if ui.add_enabled(enabled && event.is_some(), Button::new(label)).clicked() {
                events.extend(event);
                picked = true;
            }
        }
        picked
    }
}

/// The barline starting measure `index` cannot move: it or the measure it ends is locked.
fn barline_locked(measures: &[Measure], index: usize) -> bool {
    measures[index.saturating_sub(1)..=index].iter().any(|m| m.locked)
}

/// Middle of the measure, rounded down to a whole number of the signature's units.
pub fn default_split_beat(sig: TimeSignature) -> Option<f64> {
    let first = sig.numerator / 2;
    (first > 0).then(|| f64::from(first) * 4.0 / f64::from(sig.denominator))
}

pub fn can_merge_with_previous(measures: &[Measure], index: usize) -> bool {
    let previous = index.checked_sub(1).and_then(|p| measures.get(p));
    match (previous, measures.get(index)) {
        (Some(a), Some(b)) => a.time_sig.denominator == b.time_sig.denominator && !a.locked && !b.locked,
        _ => false,
    }
}

pub fn format_bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 0.05 {
        format!("{}", bpm.round() as i64)
    } else {
        format!("{bpm:.1}")
    }
}

/// `m:ss.mmm`, with a leading minus for times before the start.
pub fn format_time(sec: f64) -> String {
    if !sec.is_finite() {
        return "?".to_owned();
    }
    let sign = if sec < 0.0 { "-" } else { "" };
    let total_ms = (sec.abs() * 1000.0).round() as u64;
    format!("{sign}{}:{:02}.{:03}", total_ms / 60_000, (total_ms / 1000) % 60, total_ms % 1000)
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/header.rs"]
mod header_tests;

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid.rs"]
mod tests;

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/headless.rs"]
mod headless;
