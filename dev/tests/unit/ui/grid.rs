use super::*;
use fretscribe_core::{generate_measures, Project};

fn project() -> Project {
    fretscribe_core::testkit::project(8.0)
}

#[test]
fn effective_tempo_is_shown_only_when_it_differs() {
    let m = Measure::new(1, 0.0, 56.0, TimeSignature::COMMON);
    assert_eq!(effective_suffix(&m, 55.3).as_deref(), Some(" (55.3)"));
    assert_eq!(effective_suffix(&m, 56.01), None);
}

#[test]
fn time_text() {
    assert_eq!(format_time(0.0), "0:00.000");
    assert_eq!(format_time(65.2504), "1:05.250");
    assert_eq!(format_time(-0.5), "-0:00.500");
    assert_eq!(format_time(f64::NAN), "?");
}

#[test]
fn split_position_is_valid_for_core() {
    for (n, d) in [(4, 4), (3, 4), (6, 8), (7, 8), (2, 2), (5, 4), (12, 8), (2, 4)] {
        let mut p = project();
        p.measures[0].time_sig = TimeSignature::new(n, d);
        let at = default_split_beat(TimeSignature::new(n, d)).unwrap();
        p.split_measure(0, at).unwrap_or_else(|e| panic!("{n}/{d} at {at}: {e}"));
        assert_eq!(p.measures[0].time_sig.numerator, n / 2);
    }
    assert_eq!(default_split_beat(TimeSignature::new(1, 4)), None);
}

#[test]
fn merge_needs_a_previous_measure_with_the_same_denominator() {
    let mut next = 1;
    let mut ms = generate_measures(0.0, 6.0, 120.0, TimeSignature::COMMON, &mut next);
    assert!(!can_merge_with_previous(&ms, 0));
    assert!(can_merge_with_previous(&ms, 1));
    ms[1].time_sig = TimeSignature::new(6, 8);
    assert!(!can_merge_with_previous(&ms, 1));
    assert!(!can_merge_with_previous(&ms, 9));
}
