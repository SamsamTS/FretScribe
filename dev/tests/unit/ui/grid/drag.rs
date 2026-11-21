use super::*;
use fretscribe_core::{generate_measures, NotePolicy, Project, TimeSignature};

fn project(measures: usize) -> Project {
    let mut p = fretscribe_core::testkit::project(measures as f64 * 2.0);
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
