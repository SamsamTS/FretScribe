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
