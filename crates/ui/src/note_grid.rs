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
