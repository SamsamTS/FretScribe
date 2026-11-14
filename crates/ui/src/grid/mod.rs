//! Measure grid overlay: draws the tempo grid over a timeline rect and turns user edits into
//! `GridEvent`s. Wheel and navigation input belong to the navigation layer, not to this widget.

mod drag;
mod draw;
mod event;
mod lines;
mod popup;
mod style;

use egui::{
    pos2, vec2, Area, Button, Context, CursorIcon, Frame, Id, Order, Rect, Response, Sense,
    TextStyle, Ui,
};
use fretscribe_core::{Measure, NotePolicy, SnapGrid, TempoMap, TimeSignature};

use crate::view::TimelineView;

pub use drag::{drag_limits, dragged_start};
pub use draw::{line_rect, Dots, LineBatch};
pub use event::GridEvent;
pub use lines::{is_triplet, visible_lines, GridLine, LineKind, MIN_MAIN_BEAT_PX, MIN_SUBDIVISION_PX};
pub use popup::{policy_label, MeasureSettings};
pub use style::GridStyle;

use popup::{PopupOutcome, SettingsPopup};

const HIT_HALF_WIDTH: f32 = 4.0;
const MIN_MOVE_SEC: f64 = 1e-6;
const DOTS: Dots = Dots { width_px: 1.0, dot_px: 2.0, gap_px: 3.0 };
const LABEL_PADDING: f32 = 3.0;
const LABEL_MARGIN_PX: f64 = 10.0;

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
                if response.hovered() || response.dragged() {
                    hovered = Some(line.measure);
                    ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                self.track_drag(ui, id, rect, &response, line.measure, &mut events);
                response.context_menu(|ui| self.context_menu(ui, line.measure, &mut events));
            }
        }
        let drag = ctx.data(|d| d.get_temp::<DragState>(id.with("drag")));
        if !ui.input(|i| i.pointer.any_down()) {
            ctx.data_mut(|d| d.remove::<DragState>(id.with("drag")));
        }

        let painter = ui.painter_at(rect);
        painter.add(self.line_shape(&ctx, rect, &lines, hovered, drag));
        self.labels(ui, id, rect, &lines);
        if let Some(drag) = drag {
            self.drag_tooltip(ui, id, &drag);
        }
        self.settings_popup(&ctx, id, &mut events);
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

    fn x_of(&self, rect: Rect, sec: f64) -> f32 {
        rect.left() + self.view.sec_to_x(sec)
    }

    fn drag_policy(&self, ui: &Ui) -> NotePolicy {
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
                    policy: self.drag_policy(ui),
                });
            }
        }
    }

    fn drag_tooltip(&self, ui: &Ui, id: Id, drag: &DragState) {
        let ctx = ui.ctx();
        let Some(pointer) = ctx.input(|i| i.pointer.latest_pos()) else { return };
        let Some(index) = self.tempo.index_of(drag.measure_id) else { return };
        let mut text = format!("Measure {}: {}", index + 1, format_time(drag.current_start_sec));
        if self.drag_policy(ui) == NotePolicy::KeepAudioPosition {
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

    fn labels(&self, ui: &mut Ui, id: Id, rect: Rect, lines: &[GridLine]) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter_at(rect);
        let font = TextStyle::Small.resolve(ui.style());
        let pps = self.view.pixels_per_sec;
        for line in lines.iter().filter(|l| l.kind == LineKind::Measure) {
            let measures = self.tempo.measures();
            let Some(measure) = measures.get(line.measure) else { continue };
            let end = measures.get(line.measure + 1).map_or(
                measure.start_sec + self.tempo.measure_duration_sec(line.measure),
                |next| next.start_sec,
            );
            let effective = self.tempo.effective_bpm(line.measure);
            let galley = painter.layout_job(label_job(measure, effective, font.clone(), self.style.label_text));
            if (end - measure.start_sec) * pps < f64::from(galley.size().x) + LABEL_MARGIN_PX {
                continue;
            }
            let origin = pos2(self.x_of(rect, line.sec) + 4.0, rect.top() + 3.0);
            let label_rect = Rect::from_min_size(origin, galley.size())
                .expand2(vec2(LABEL_PADDING, LABEL_PADDING / 2.0));
            let hit = label_rect.intersect(rect);
            if hit.width() <= 0.0 || hit.height() <= 0.0 {
                continue;
            }
            if self.passthrough.is_some_and(|p| p.contains_rect(hit)) {
                painter.rect_filled(label_rect, 3.0, self.style.label_background);
                painter.galley(origin, galley, self.style.label_text);
                continue;
            }
            let response = ui.interact(hit, id.with(("label", measure.id)), Sense::click());
            let background = if response.hovered() {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
                self.style.label_background_hover
            } else {
                self.style.label_background
            };
            painter.rect_filled(label_rect, 3.0, background);
            painter.galley(origin, galley, self.style.label_text);
            if response.clicked() {
                let popup = SettingsPopup {
                    settings: MeasureSettings::new(line.measure, measure, self.policy),
                    anchor: label_rect.left_bottom() + vec2(0.0, 4.0),
                };
                ctx.data_mut(|d| d.insert_temp(id.with("popup"), popup));
            }
        }
    }

    fn settings_popup(&self, ctx: &Context, id: Id, events: &mut Vec<GridEvent>) {
        let popup_id = id.with("popup");
        let Some(mut popup) = ctx.data(|d| d.get_temp::<SettingsPopup>(popup_id)) else { return };
        let Some(index) = self.tempo.index_of(popup.settings.measure_id) else {
            ctx.data_mut(|d| d.remove::<SettingsPopup>(popup_id));
            return;
        };
        popup.settings.index = index;
        let policy_before = popup.settings.policy;
        let outcome = popup::show(ctx, popup_id, &mut popup);
        if popup.settings.policy != policy_before {
            events.push(GridEvent::NotePolicyChanged(popup.settings.policy));
        }
        match outcome {
            PopupOutcome::Open => ctx.data_mut(|d| d.insert_temp(popup_id, popup)),
            PopupOutcome::Confirm => {
                events.extend(popup.settings.events());
                ctx.data_mut(|d| d.remove::<SettingsPopup>(popup_id));
            }
            PopupOutcome::Cancel => ctx.data_mut(|d| d.remove::<SettingsPopup>(popup_id)),
            PopupOutcome::AutoFit => {
                events.push(GridEvent::AutoFitRequested { index });
                ctx.data_mut(|d| d.remove::<SettingsPopup>(popup_id));
            }
        }
    }

    fn context_menu(&self, ui: &mut Ui, index: usize, events: &mut Vec<GridEvent>) {
        let measures = self.tempo.measures();
        let Some(measure) = measures.get(index) else { return };
        let policy = self.policy;

        if ui.button("Insert measure here").clicked() {
            events.push(GridEvent::InsertMeasure { index, policy });
            ui.close_menu();
        }
        let can_delete = measures.len() > 1;
        if ui.add_enabled(can_delete, Button::new("Delete measure")).clicked() {
            events.push(GridEvent::DeleteMeasure { index, policy });
            ui.close_menu();
        }
        let split = default_split_beat(measure.time_sig);
        if ui.add_enabled(split.is_some(), Button::new("Split measure")).clicked() {
            if let Some(at_beat) = split {
                events.push(GridEvent::SplitMeasure { index, at_beat });
            }
            ui.close_menu();
        }
        let can_merge = can_merge_with_previous(measures, index);
        if ui.add_enabled(can_merge, Button::new("Merge with previous")).clicked() {
            events.push(GridEvent::MergeMeasures { index: index - 1 });
            ui.close_menu();
        }
        if self.autofit {
            ui.separator();
            if ui.button("Snap grid to detected beats").clicked() {
                events.push(GridEvent::AutoFitRequested { index: 0 });
                ui.close_menu();
            }
            if ui.button("Auto-fit from this measure").clicked() {
                events.push(GridEvent::AutoFitRequested { index });
                ui.close_menu();
            }
        }
    }
}

/// Middle of the measure, rounded down to a whole number of the signature's units.
pub fn default_split_beat(sig: TimeSignature) -> Option<f64> {
    let first = sig.numerator / 2;
    (first > 0).then(|| f64::from(first) * 4.0 / f64::from(sig.denominator))
}

pub fn can_merge_with_previous(measures: &[Measure], index: usize) -> bool {
    let previous = index.checked_sub(1).and_then(|p| measures.get(p));
    match (previous, measures.get(index)) {
        (Some(a), Some(b)) => a.time_sig.denominator == b.time_sig.denominator,
        _ => false,
    }
}

pub fn measure_label(measure: &Measure) -> String {
    format!(
        "{}/{} \u{2669}={}",
        measure.time_sig.numerator,
        measure.time_sig.denominator,
        format_bpm(measure.tempo_bpm)
    )
}

/// The realised tempo as " (55.3)", only when it differs visibly from the nominal one.
pub fn effective_suffix(measure: &Measure, effective_bpm: f64) -> Option<String> {
    let shown = format_bpm(effective_bpm);
    (shown != format_bpm(measure.tempo_bpm)).then(|| format!(" ({shown})"))
}

fn label_job(measure: &Measure, effective_bpm: f64, font: egui::FontId, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(&measure_label(measure), 0.0, egui::TextFormat::simple(font.clone(), color));
    if let Some(suffix) = effective_suffix(measure, effective_bpm) {
        job.append(&suffix, 0.0, egui::TextFormat::simple(font, color.gamma_multiply(0.55)));
    }
    job
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

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{generate_measures, Project, Track, TrackRole};

    fn project() -> Project {
        Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            8.0,
            120.0,
            TimeSignature::COMMON,
        )
    }

    #[test]
    fn label_text() {
        let m = Measure::new(1, 0.0, 70.0, TimeSignature::COMMON);
        assert_eq!(measure_label(&m), "4/4 \u{2669}=70");
        let m = Measure::new(1, 0.0, 97.5, TimeSignature::new(6, 8));
        assert_eq!(measure_label(&m), "6/8 \u{2669}=97.5");
        assert_eq!(format_bpm(119.99), "120");
        assert_eq!(format_bpm(120.04), "120");
    }

    #[test]
    fn effective_tempo_is_shown_only_when_it_differs() {
        let m = Measure::new(1, 0.0, 56.0, TimeSignature::COMMON);
        assert_eq!(effective_suffix(&m, 55.3).as_deref(), Some(" (55.3)"));
        assert_eq!(effective_suffix(&m, 56.01), None);
    }

    #[test]
    fn time_text() {
        assert_eq!(format_time(0.0), "0:00.000");
        assert_eq!(format_time(65.2504), "1:05.250");
        assert_eq!(format_time(-0.5), "-0:00.500");
        assert_eq!(format_time(f64::NAN), "?");
    }

    #[test]
    fn split_position_is_valid_for_core() {
        for (n, d) in [(4, 4), (3, 4), (6, 8), (7, 8), (2, 2), (5, 4), (12, 8), (2, 4)] {
            let mut p = project();
            p.measures[0].time_sig = TimeSignature::new(n, d);
            let at = default_split_beat(TimeSignature::new(n, d)).unwrap();
            p.split_measure(0, at).unwrap_or_else(|e| panic!("{n}/{d} at {at}: {e}"));
            assert_eq!(p.measures[0].time_sig.numerator, n / 2);
        }
        assert_eq!(default_split_beat(TimeSignature::new(1, 4)), None);
    }

    #[test]
    fn merge_needs_a_previous_measure_with_the_same_denominator() {
        let mut next = 1;
        let mut ms = generate_measures(0.0, 6.0, 120.0, TimeSignature::COMMON, &mut next);
        assert!(!can_merge_with_previous(&ms, 0));
        assert!(can_merge_with_previous(&ms, 1));
        ms[1].time_sig = TimeSignature::new(6, 8);
        assert!(!can_merge_with_previous(&ms, 1));
        assert!(!can_merge_with_previous(&ms, 9));
    }
}

#[cfg(test)]
mod headless;
