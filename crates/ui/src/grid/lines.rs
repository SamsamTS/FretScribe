//! Which grid lines exist for a view, independent of any drawing.

use fretscribe_core::{BeatKind, SnapGrid, TempoMap};

use crate::view::TimelineView;

pub const MIN_SUBDIVISION_PX: f64 = 6.0;
pub const MIN_MAIN_BEAT_PX: f64 = 6.0;
const COINCIDENT_SEC: f64 = 1e-7;
const EDGE_MARGIN_PX: f64 = 2.0;

/// Declared in drawing order: later kinds are painted over earlier ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LineKind {
    Straight,
    Triplet,
    MainBeat,
    Measure,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLine {
    pub sec: f64,
    pub kind: LineKind,
    pub measure: usize,
}

pub fn is_triplet(division: u8) -> bool {
    division.is_multiple_of(3)
}

/// A subdivision level is dropped when its lines would be closer than `MIN_SUBDIVISION_PX`, main
/// beats likewise. Subdivision lines that fall on a main beat or barline are not repeated.
pub fn visible_lines(
    tempo: &TempoMap,
    view: &TimelineView,
    snap: &SnapGrid,
    width_px: f32,
) -> Vec<GridLine> {
    let mut lines = Vec::new();
    if tempo.is_empty() {
        return lines;
    }
    let visible = view.visible_range(width_px);
    let pps = view.pixels_per_sec;
    let margin = EDGE_MARGIN_PX / pps.max(f64::MIN_POSITIVE);
    let in_view = |sec: f64| sec >= visible.start - margin && sec <= visible.end + margin;

    let first = tempo.sec_to_beat_position(visible.start).0;
    for i in first..tempo.len() {
        let start = tempo.measure_start_sec(i);
        if start > visible.end + margin {
            break;
        }
        if in_view(start) {
            lines.push(GridLine { sec: start, kind: LineKind::Measure, measure: i });
        }

        let mains: Vec<f64> = tempo
            .beat_lines(i)
            .iter()
            .filter(|l| l.kind == BeatKind::MainBeat)
            .map(|l| l.time_sec)
            .collect();
        let main_gap_px = mains
            .first()
            .map_or(tempo.measure_duration_sec(i), |first| first - start)
            * pps;
        let show_mains = main_gap_px >= MIN_MAIN_BEAT_PX;
        let mut occupied = vec![start];
        if show_mains {
            occupied.extend(&mains);
            lines.extend(mains.iter().filter(|&&sec| in_view(sec)).map(|&sec| GridLine {
                sec,
                kind: LineKind::MainBeat,
                measure: i,
            }));
        }

        let sec_per_beat = 60.0 / tempo.effective_bpm(i);
        let kept: Vec<u8> = snap
            .divisions
            .iter()
            .copied()
            .filter(|&d| d > 0 && pps * sec_per_beat / f64::from(d) >= MIN_SUBDIVISION_PX)
            .collect();
        if kept.is_empty() {
            continue;
        }
        for (sec, division) in tempo.grid_lines(i, &SnapGrid::new(&kept)) {
            if !in_view(sec) || occupied.iter().any(|o| (o - sec).abs() < COINCIDENT_SEC) {
                continue;
            }
            let kind = if is_triplet(division) { LineKind::Triplet } else { LineKind::Straight };
            lines.push(GridLine { sec, kind, measure: i });
        }
    }
    lines
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/lines.rs"]
mod tests;
