//! Limits for dragging a barline so the edit never fails in `core::Project::move_measure_start`.

use fretscribe_core::{Measure, MAX_BPM, MIN_BPM};

const MARGIN_SEC: f64 = 2e-3;
const TEMPO_MARGIN_SEC: f64 = 1e-6;

/// Allowed range of the new start of measure `index`. The previous measure is refitted to end at
/// the new start, so its tempo must stay valid; an anchored measure further on must not be reached
/// by the measures that follow the dragged one. The range is never inverted.
pub fn drag_limits(measures: &[Measure], index: usize) -> (f64, f64) {
    let Some(current) = measures.get(index) else {
        return (0.0, 0.0);
    };
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);

    if let Some(prev) = index.checked_sub(1).and_then(|p| measures.get(p)) {
        let beats = prev.time_sig.beats();
        lo = prev.start_sec + (beats * 60.0 / MAX_BPM).max(MARGIN_SEC) + TEMPO_MARGIN_SEC;
        hi = prev.start_sec + beats * 60.0 / MIN_BPM - TEMPO_MARGIN_SEC;
    }

    let mut reach = 0.0;
    for (j, next) in measures.iter().enumerate().skip(index + 1) {
        if next.anchored {
            hi = hi.min(next.start_sec - MARGIN_SEC - reach);
            break;
        }
        reach += measures[j - 1].nominal_duration_sec();
    }

    if lo <= hi {
        (lo, hi)
    } else {
        (current.start_sec, current.start_sec)
    }
}

/// Where the barline goes for a pointer at `pointer_sec`, keeping the point that was grabbed.
pub fn dragged_start(pointer_sec: f64, grab_offset_sec: f64, limits: (f64, f64)) -> f64 {
    (pointer_sec - grab_offset_sec).max(limits.0).min(limits.1)
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/drag.rs"]
mod tests;
