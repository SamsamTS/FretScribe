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

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{generate_measures, NotePolicy, Project, TimeSignature, Track, TrackRole};

    fn project(measures: usize) -> Project {
        let mut p = Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            measures as f64 * 2.0,
            120.0,
            TimeSignature::COMMON,
        );
        let mut next = 1;
        p.measures = generate_measures(0.0, measures as f64 * 2.0, 120.0, TimeSignature::COMMON, &mut next);
        p
    }

    #[test]
    fn first_measure_is_only_bounded_by_what_follows() {
        let p = project(3);
        assert_eq!(drag_limits(&p.measures, 0), (f64::NEG_INFINITY, f64::INFINITY));
    }

    #[test]
    fn previous_tempo_stays_valid() {
        let p = project(3);
        let (lo, hi) = drag_limits(&p.measures, 1);
        assert!((lo - 0.48).abs() < 1e-3, "4 beats at 500 BPM is 0.48 s: {lo}");
        assert!((hi - 24.0).abs() < 1e-3, "4 beats at 10 BPM is 24 s: {hi}");
    }

    #[test]
    fn anchored_neighbour_bounds_the_range() {
        let mut p = project(5);
        p.measures[2].anchored = true;
        assert!((drag_limits(&p.measures, 1).1 - (4.0 - MARGIN_SEC)).abs() < 1e-9);
        p.measures[4].anchored = true;
        let (_, hi) = drag_limits(&p.measures, 2);
        assert!((hi - (8.0 - MARGIN_SEC - 2.0)).abs() < 1e-9, "measure 3 sits 2 s after measure 2: {hi}");
    }

    #[test]
    fn limits_are_accepted_by_core() {
        for anchored_later in [false, true] {
            for index in 0..4 {
                let mut p = project(5);
                if anchored_later {
                    p.measures[4].anchored = true;
                }
                let (lo, hi) = drag_limits(&p.measures, index);
                let start = p.measures[index].start_sec;
                for target in [lo.max(start - 50.0), hi.min(start + 50.0)] {
                    let mut q = p.clone();
                    q.move_measure_start(index, target, NotePolicy::FollowGrid)
                        .unwrap_or_else(|e| panic!("index {index} target {target}: {e}"));
                    q.validate().unwrap();
                }
            }
        }
    }

    #[test]
    fn inverted_range_pins_the_measure() {
        let mut p = project(4);
        p.measures[1].start_sec = 1.0;
        p.measures[2].anchored = true;
        p.measures[2].start_sec = 0.3;
        assert_eq!(drag_limits(&p.measures, 1), (1.0, 1.0));
        assert_eq!(drag_limits(&p.measures, 9), (0.0, 0.0));
    }

    #[test]
    fn drag_keeps_the_grabbed_point_and_clamps() {
        assert_eq!(dragged_start(5.0, 0.25, (0.0, 10.0)), 4.75);
        assert_eq!(dragged_start(-5.0, 0.0, (0.5, 10.0)), 0.5);
        assert_eq!(dragged_start(50.0, 0.0, (0.5, 10.0)), 10.0);
        assert_eq!(dragged_start(3.0, 0.0, (1.0, 1.0)), 1.0);
    }
}
