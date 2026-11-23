use super::*;
use crate::measures::tests::{close, grid};
use crate::model::TimeSignature;

fn map() -> TempoMap {
    TempoMap::new(&grid(&[120.0, 120.0]))
}

fn g(divisions: &[u8]) -> SnapGrid {
    SnapGrid::new(divisions)
}

#[test]
fn straight_and_triplet_are_independent_and_may_be_none() {
    let grid = SnapGrid::default();
    assert_eq!((grid.straight(), grid.triplet()), (Some(4), Some(3)));
    let both = grid.with(true, Some(24)).with(false, Some(16));
    assert_eq!(both.divisions, [16, 24]);
    assert_eq!(both.label(), "1/64 + 1/64T");
    assert!(both.with(false, None).with(true, None).divisions.is_empty());
    let m = map();
    // 1/64 and 1/64T coincide 8 times per beat (every 1/8): 16 + 24 - 8 lines per beat.
    assert_eq!(m.grid_lines(0, &both).len(), 4 * (16 + 24 - 8));
    close(m.snap_to_grid(0.0301, &g(&[16])), 0.03125);
}

#[test]
fn new_sanitises() {
    assert_eq!(g(&[4, 4, 5, 3]).divisions, [4, 3]);
    assert_eq!(g(&[0, 7]), SnapGrid::default());
    assert_eq!(g(&[]), SnapGrid::default());
}

#[test]
fn snaps_to_nearest_single_division() {
    let m = map();
    close(m.snap_to_grid(0.12, &g(&[4])), 0.125);
    close(m.snap_to_grid(0.20, &g(&[4])), 0.25);
    close(m.snap_to_grid(0.20, &g(&[2])), 0.25);
    close(m.snap_to_grid(0.40, &g(&[1])), 0.5);
    close(m.snap_to_grid(0.09, &g(&[3])), 0.5 / 3.0);
}

#[test]
fn triplet_positions() {
    let m = map();
    close(m.snap_to_grid(0.17, &g(&[3])), 0.5 / 3.0);
    close(m.snap_to_grid(0.32, &g(&[3])), 1.0 / 3.0);
    close(m.snap_to_grid(0.17, &g(&[6])), 0.5 / 3.0);
    close(m.snap_to_grid(0.09, &g(&[12])), 0.5 / 6.0);
}

#[test]
fn mixed_grid_lets_straight_and_triplet_coexist() {
    let m = map();
    let grid = g(&[4, 3]);
    close(m.snap_to_grid(0.125, &grid), 0.125);
    close(m.snap_to_grid(0.17, &grid), 0.5 / 3.0);
    close(m.snap_to_grid(0.24, &grid), 0.25);
    close(m.snap_to_grid(0.30, &grid), 0.5 / 3.0 * 2.0);
}

#[test]
fn ties_go_to_the_coarser_division() {
    let m = map();
    close(m.snap_to_grid(0.125, &g(&[1, 2])), 0.0);
    close(m.snap_to_grid(0.375, &g(&[2, 1])), 0.5);
    let beat_between = (0.25 + 1.0 / 3.0) / 2.0;
    close(m.snap_to_grid(beat_between * 0.5, &g(&[4, 3])), 0.5 / 3.0);
    close(m.snap_to_grid(beat_between * 0.5, &g(&[3, 4])), 0.5 / 3.0);
}

#[test]
fn snaps_to_next_barline_and_returns_normalised_position() {
    let m = map();
    close(m.snap_to_grid(1.99, &g(&[2])), 2.0);
    let pos = m.snap_to_pos(1.99, &g(&[2]));
    assert_eq!(pos.measure, 2);
    close(pos.beat(), 0.0);
}

#[test]
fn snap_before_first_and_after_last_measure() {
    let m = map();
    close(m.snap_to_grid(-0.3, &g(&[4])), 0.0);
    close(m.snap_to_grid(4.31, &g(&[4])), 4.25);
    let pos = m.snap_to_pos(4.37, &g(&[4]));
    assert_eq!(pos.measure, 2);
    close(pos.beat(), 4.75);
}

#[test]
fn snap_stays_within_irregular_measure() {
    let mut ms = grid(&[120.0, 120.0]);
    ms[0].time_sig = TimeSignature::new(5, 8);
    ms[1].start_sec = 1.25;
    let m = TempoMap::new(&ms);
    close(m.snap_to_grid(1.24, &g(&[1])), 1.25);
    close(m.snap_to_grid(1.0, &g(&[1])), 1.0);
}

#[test]
fn snap_is_idempotent() {
    let m = map();
    let grid = g(&[4, 6]);
    let mut t = 0.0;
    while t < 4.0 {
        let once = m.snap_to_grid(t, &grid);
        close(m.snap_to_grid(once, &grid), once);
        t += 0.0173;
    }
}

#[test]
fn grid_lines_label_each_position_with_the_coarsest_division() {
    let m = map();
    let lines = m.grid_lines(0, &g(&[4, 3]));
    assert_eq!(lines.len(), 4 * 4 + 4 * 3 - 4);
    assert_eq!(lines[0], (0.0, 4));
    let at = |sec: f64| lines.iter().find(|l| (l.0 - sec).abs() < 1e-9).copied();
    assert_eq!(at(0.125).map(|l| l.1), Some(4));
    assert_eq!(at(0.5 / 3.0).map(|l| l.1), Some(3));
    assert_eq!(at(0.25).map(|l| l.1), Some(4));
    assert_eq!(at(0.5).map(|l| l.1), Some(4));
    assert!(lines.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn grid_lines_single_and_mixed() {
    let m = map();
    assert_eq!(m.grid_lines(0, &g(&[1])).len(), 4);
    assert_eq!(m.grid_lines(0, &g(&[2])).len(), 8);
    assert_eq!(m.grid_lines(0, &g(&[3])).len(), 12);
    assert_eq!(m.grid_lines(0, &g(&[12])).len(), 48);
    let lines = m.grid_lines(0, &g(&[2, 3]));
    assert_eq!(lines.len(), 8 + 12 - 4);
    assert_eq!(lines.iter().filter(|l| l.1 == 2).count(), 8);
    let lines = m.grid_lines(0, &g(&[4, 6]));
    assert_eq!(lines.len(), 16 + 24 - 8);
    assert!(lines.iter().all(|l| l.0 < 2.0));
}

#[test]
fn grid_lines_follow_measure_length() {
    let mut ms = grid(&[120.0]);
    ms[0].time_sig = TimeSignature::new(7, 8);
    let m = TempoMap::new(&ms);
    assert_eq!(m.grid_lines(0, &g(&[2])).len(), 7);
    assert_eq!(m.grid_lines(0, &g(&[1])).len(), 4);
}
