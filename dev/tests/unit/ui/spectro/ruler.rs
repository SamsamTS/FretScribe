use super::*;

#[test]
fn step_keeps_ticks_apart_at_every_zoom() {
    for pps in [0.5, 3.0, 25.0, 120.0, 900.0, 5000.0] {
        let step = ruler_step(pps);
        assert!(step * pps >= MIN_TICK_SPACING_PX || step == 1800.0, "{pps}");
    }
    assert_eq!(ruler_step(100.0), 1.0);
}

#[test]
fn times_are_minutes_and_seconds() {
    assert_eq!(format_time(0.0, 1.0), "0:00");
    assert_eq!(format_time(75.0, 5.0), "1:15");
    assert_eq!(format_time(3.5, 0.5), "0:03.5");
    assert_eq!(format_time(61.25, 0.05), "1:01.25");
}
