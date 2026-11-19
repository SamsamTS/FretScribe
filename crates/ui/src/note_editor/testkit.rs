use fretscribe_core::{Project, SnapGrid, TempoMap, TimePos};

use super::EditCtx;

pub const PX_PER_SEC: f64 = 200.0;

pub fn project() -> Project {
    fretscribe_core::testkit::project(8.0)
}

pub use fretscribe_core::testkit::add;

pub fn at(beat: f64) -> TimePos {
    TimePos::at(1, beat)
}

/// Runs `f` with a context for `p` at 200 px/s (so one beat is 100 px) and a 12 px snap threshold.
pub fn with_ctx<R>(p: &Project, grid: &[u8], alt: bool, f: impl FnOnce(&EditCtx) -> R) -> R {
    let tempo = TempoMap::new(&p.measures);
    let grid = SnapGrid::new(grid);
    f(&EditCtx { project: p, tempo: &tempo, grid: &grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, onsets: None, alt })
}

/// Seconds of a beat in the first measure at 120 bpm.
pub fn sec(beat: f64) -> f64 {
    beat * 0.5
}

pub fn near(pos: TimePos, beat: f64) {
    assert_eq!(pos.measure, 1);
    assert!((pos.beat() - beat).abs() < 0.011, "{pos:?} vs {beat}");
}
