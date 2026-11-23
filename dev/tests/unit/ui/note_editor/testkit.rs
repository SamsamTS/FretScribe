use fretscribe_core::{Project, SnapGrid, TempoMap, TimePos};

use super::EditCtx;

pub(crate) const PX_PER_SEC: f64 = 200.0;

pub(crate) fn project() -> Project {
    fretscribe_core::testkit::project(8.0)
}

pub(crate) use fretscribe_core::testkit::add;

pub(crate) fn at(beat: f64) -> TimePos {
    TimePos::at(1, beat)
}

/// Runs `f` with a context for `p` at 200 px/s (so one beat is 100 px) and a 12 px snap threshold.
pub(crate) fn with_ctx<R>(p: &Project, grid: &[u8], alt: bool, f: impl FnOnce(&EditCtx) -> R) -> R {
    let tempo = TempoMap::new(&p.measures);
    let grid = SnapGrid::new(grid);
    f(&EditCtx { project: p, tempo: &tempo, grid: &grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, alt })
}

/// Seconds of a beat in the first measure at 120 bpm.
pub(crate) fn sec(beat: f64) -> f64 {
    beat * 0.5
}

pub(crate) fn near(pos: TimePos, beat: f64) {
    assert_eq!(pos.measure, 1);
    assert!((pos.beat() - beat).abs() < 0.011, "{pos:?} vs {beat}");
}
