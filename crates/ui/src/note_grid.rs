//! Grid lines of the notation lanes and where notes sit between them. Sheet and tab share one `NoteGrid`,
//! so a note has the same x in both: it is kept inside its cell (between two visible lines) and clear of them.

use crate::view::TimelineView;
use fretscribe_core::{BeatKind, TempoMap};

/// Width every glyph (notehead, accidental, two-digit fret) is fitted with, in both views.
const GLYPH_W: f32 = 16.0;
const MARGIN: f32 = 3.0;
/// Narrowest cell kept; coarser lines are dropped (halving the count) below this.
const MIN_CELL: f32 = 36.0;
const EPS: f64 = 1e-4;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteGrid {
    lines: Vec<(f64, BeatKind)>,
}

impl NoteGrid {
    /// The lines in view (plus one measure either side) at the level of detail the zoom allows.
    pub fn new(tempo: &TempoMap, view: &TimelineView, width: f32) -> Self {
        let range = view.visible_range(width);
        let ms = tempo.measures();
        let first = ms.partition_point(|m| m.start_sec <= range.start).saturating_sub(2);
        let mut lines = Vec::new();
        for (i, m) in ms.iter().enumerate().skip(first) {
            if m.start_sec > range.end {
                break;
            }
            let all = tempo.beat_lines(i);
            let spacing = match all.as_slice() {
                [a, b, ..] => b.time_sec - a.time_sec,
                _ => f64::INFINITY,
            };
            let mut step = 1;
            while step < all.len() && spacing * view.pixels_per_sec * step as f64 <= f64::from(MIN_CELL) {
                step *= 2;
            }
            lines.extend(all.iter().enumerate().filter(|(k, _)| k % step == 0).map(|(_, l)| (l.time_sec, l.kind)));
        }
        Self { lines }
    }

    pub fn lines(&self) -> &[(f64, BeatKind)] {
        &self.lines
    }

    /// Drawn x (relative to the timeline origin) of something at `sec`.
    pub fn x(&self, view: &TimelineView, sec: f64) -> f32 {
        let x = view.sec_to_x(sec);
        let Some(i) = self.lines.partition_point(|l| l.0 <= sec + EPS).checked_sub(1) else { return x };
        let lo = view.sec_to_x(self.lines[i].0) + MARGIN + GLYPH_W / 2.0;
        match self.lines.get(i + 1) {
            Some(next) => {
                let hi = view.sec_to_x(next.0) - MARGIN - GLYPH_W / 2.0;
                if hi < lo { (lo + hi) / 2.0 } else { x.clamp(lo, hi) }
            }
            None => x.max(lo),
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/ui/note_grid.rs"]
mod tests;

/// The small text above a harmonic (tab and staff).
pub fn harmonic_tag(technique: Option<fretscribe_core::Technique>) -> Option<&'static str> {
    match technique {
        Some(fretscribe_core::Technique::Harmonic) => Some("N.H."),
        Some(fretscribe_core::Technique::PinchHarmonic) => Some("P.H."),
        _ => None,
    }
}

/// Palm mute brackets ("P.M." and a dashed line to the last note of the run) and vibrato waves of `layout`,
/// drawn on the line `y`; `x_of` maps seconds to screen x and `unit` is the line spacing.
pub fn paint_technique_marks(p: &egui::Painter, layout: &fretscribe_notation::staff::StaffLayout, x_of: impl Fn(f64) -> f32, y: f32, unit: f32, color: egui::Color32) {
    use fretscribe_core::Technique;
    let (clip, stroke) = (p.clip_rect(), egui::Stroke::new(1.0_f32, color));
    for run in fretscribe_notation::staff::technique_runs(layout, Technique::PalmMute) {
        let (x0, x1) = (x_of(run.first_sec), x_of(run.last_sec));
        if x1 < clip.left() || x0 > clip.right() {
            continue;
        }
        p.text(egui::pos2(x0, y - 1.0), egui::Align2::LEFT_BOTTOM, "P.M.", egui::FontId::proportional(unit * 0.9), color);
        let end = egui::pos2(x1, y);
        p.extend(egui::Shape::dashed_line(&[egui::pos2(x0, y), end], stroke, 3.0, 2.0));
        p.line_segment([end, end + egui::vec2(0.0, unit * 0.5)], stroke);
    }
    for run in fretscribe_notation::staff::technique_runs(layout, Technique::Vibrato) {
        let (x0, x1) = (x_of(run.first_sec), x_of(run.end_sec).max(x_of(run.last_sec) + unit));
        if x1 >= clip.left() && x0 <= clip.right() {
            paint_wave(p, x0, x1, y - unit * 0.6, unit * 0.3, unit * 0.8, color.gamma_multiply(0.5));
        }
    }
}

/// Dashed "let ring" lines over runs of consecutive flagged measures (text at the start, a tick at the end).
pub fn paint_let_ring(p: &egui::Painter, layout: &fretscribe_notation::staff::StaffLayout, x_of: impl Fn(f64) -> f32, y: f32, unit: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.0_f32, color);
    let mut runs: Vec<(f64, f64)> = Vec::new();
    for (i, m) in layout.measures.iter().enumerate() {
        match runs.last_mut().filter(|_| m.let_ring && i > 0 && layout.measures[i - 1].let_ring) {
            Some(r) => r.1 = m.end_sec,
            None if m.let_ring => runs.push((m.start_sec, m.end_sec)),
            None => {}
        }
    }
    for (x0, x1) in runs.into_iter().map(|(a, b)| (x_of(a) + 2.0, x_of(b) - 2.0)) {
        if x1 < p.clip_rect().left() || x0 > p.clip_rect().right() {
            continue;
        }
        p.text(egui::pos2(x0, y - 1.0), egui::Align2::LEFT_BOTTOM, "let ring", egui::FontId::proportional(unit * 0.9), color);
        p.extend(egui::Shape::dashed_line(&[egui::pos2(x0, y), egui::pos2(x1, y)], stroke, 4.0, 3.0));
        p.line_segment([egui::pos2(x1, y), egui::pos2(x1, y + unit * 0.5)], stroke);
    }
}

/// A calligraphic wave from `x0` to `x1` around `y`, one period per `period` pixels, thicker on the downward strokes.
pub fn paint_wave(p: &egui::Painter, x0: f32, x1: f32, y: f32, amp: f32, period: f32, color: egui::Color32) {
    let steps = (((x1 - x0) / period.max(1.0)) * 12.0).ceil().max(2.0) as usize;
    let (thin, thick) = (0.5_f32, (amp * 0.9).clamp(1.2, 2.6));
    let mut mesh = egui::Mesh::default();
    for i in 0..=steps {
        let x = x0 + (x1 - x0) * i as f32 / steps as f32;
        let phase = (x - x0) / period * std::f32::consts::TAU;
        let w = 0.5 * (thin + (thick - thin) * phase.cos().max(0.0));
        let cy = y + amp * phase.sin();
        mesh.colored_vertex(egui::pos2(x, cy - w), color);
        mesh.colored_vertex(egui::pos2(x, cy + w), color);
        if i > 0 {
            let k = 2 * i as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
