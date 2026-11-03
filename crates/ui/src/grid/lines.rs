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

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{generate_measures, TimeSignature};

    fn map(len_sec: f64) -> TempoMap {
        let mut next = 1;
        TempoMap::new(&generate_measures(0.0, len_sec, 120.0, TimeSignature::COMMON, &mut next))
    }

    fn lines(map: &TempoMap, view: TimelineView, divisions: &[u8], width: f32) -> Vec<GridLine> {
        visible_lines(map, &view, &SnapGrid::new(divisions), width)
    }

    fn count(lines: &[GridLine], kind: LineKind) -> usize {
        lines.iter().filter(|l| l.kind == kind).count()
    }

    #[test]
    fn measure_and_main_beat_lines() {
        let m = map(4.0);
        let l = lines(&m, TimelineView::new(0.0, 100.0), &[1], 400.0);
        assert_eq!(count(&l, LineKind::Measure), 2);
        assert_eq!(count(&l, LineKind::MainBeat), 6);
        assert_eq!(count(&l, LineKind::Straight) + count(&l, LineKind::Triplet), 0);
    }

    #[test]
    fn subdivisions_on_beats_are_not_repeated() {
        let m = map(4.0);
        let l = lines(&m, TimelineView::new(0.0, 100.0), &[2], 400.0);
        assert_eq!(count(&l, LineKind::Straight), 8);
        let l = lines(&m, TimelineView::new(0.0, 100.0), &[3], 400.0);
        assert_eq!(count(&l, LineKind::Triplet), 16);
        assert_eq!(count(&l, LineKind::Straight), 0);
    }

    #[test]
    fn mixed_grid_has_both_kinds() {
        let m = map(4.0);
        let l = lines(&m, TimelineView::new(0.0, 100.0), &[4, 3], 400.0);
        assert!(count(&l, LineKind::Straight) > 0 && count(&l, LineKind::Triplet) > 0);
        assert!(l.iter().filter(|l| l.kind == LineKind::Triplet).all(|l| {
            let beat = (l.sec / 0.5).fract();
            beat > 1e-6 && (beat * 3.0 - (beat * 3.0).round()).abs() < 1e-6
        }));
    }

    #[test]
    fn finest_level_is_skipped_when_too_dense() {
        let m = map(4.0);
        let l = lines(&m, TimelineView::new(0.0, 100.0), &[4, 3], 400.0);
        assert!(count(&l, LineKind::Straight) > 0);
        let l = lines(&m, TimelineView::new(0.0, 36.0), &[4, 3], 144.0);
        assert_eq!(count(&l, LineKind::Straight), 0, "1/16 is 4.5 px apart");
        assert!(count(&l, LineKind::Triplet) > 0, "1/8T is 6 px apart");
    }

    #[test]
    fn zooming_out_hides_beats_then_keeps_only_barlines() {
        let m = map(8.0);
        let l = lines(&m, TimelineView::new(0.0, 20.0), &[4], 160.0);
        assert_eq!(count(&l, LineKind::Straight), 0);
        assert!(count(&l, LineKind::MainBeat) > 0);
        let l = lines(&m, TimelineView::new(0.0, 10.0), &[4], 80.0);
        assert_eq!(count(&l, LineKind::MainBeat), 0);
        assert_eq!(count(&l, LineKind::Measure), 4);
    }

    #[test]
    fn only_visible_lines_are_produced() {
        let m = map(40.0);
        let view = TimelineView::new(10.0, 100.0);
        let l = lines(&m, view, &[4, 3], 200.0);
        assert!(!l.is_empty());
        assert!(l.iter().all(|l| l.sec >= 9.97 && l.sec <= 12.03), "{l:?}");
        assert!(l.iter().all(|l| l.measure >= 4 && l.measure <= 6));
    }

    #[test]
    fn view_before_the_grid_and_empty_map() {
        let m = map(4.0);
        let l = lines(&m, TimelineView::new(-1.0, 100.0), &[1], 400.0);
        assert!(l.iter().all(|l| l.sec >= -0.03));
        assert_eq!(count(&l, LineKind::Measure), 2);
        assert!(lines(&TempoMap::new(&[]), TimelineView::default(), &[1], 100.0).is_empty());
    }

    #[test]
    fn kinds_sort_in_drawing_order() {
        assert!(LineKind::Straight < LineKind::Triplet);
        assert!(LineKind::Triplet < LineKind::MainBeat);
        assert!(LineKind::MainBeat < LineKind::Measure);
        assert!(is_triplet(3) && is_triplet(6) && is_triplet(12));
        assert!(!is_triplet(1) && !is_triplet(2) && !is_triplet(4) && !is_triplet(8));
    }
}
