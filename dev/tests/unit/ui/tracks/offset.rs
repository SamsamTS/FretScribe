use super::*;

fn close(a: Option<f64>, b: f64) {
    assert!(a.is_some_and(|a| (a - b).abs() < 1e-9), "{a:?} != {b}");
}

#[test]
fn parses_signed_decimal_and_suffixed_values() {
    close(parse_offset("+120"), 0.12);
    close(parse_offset("-35.5"), -0.0355);
    close(parse_offset("120ms"), 0.12);
    close(parse_offset("-0.5 ms"), -0.0005);
    close(parse_offset("  +12,5 MS "), 0.0125);
    close(parse_offset("0"), 0.0);
}

#[test]
fn rejects_invalid_text() {
    for bad in ["", "ms", "abc", "+-3", "++3", "12 34", "1e999", "NaN", "inf", "--5", "5 s"] {
        assert_eq!(parse_offset(bad), None, "{bad:?}");
    }
}

#[test]
fn formats_with_explicit_sign() {
    assert_eq!(format_offset(0.12), "+120 ms");
    assert_eq!(format_offset(-0.0355), "-35.5 ms");
    assert_eq!(format_offset(0.0), "+0 ms");
    assert_eq!(format_offset(-0.00001), "+0 ms");
    assert_eq!(format_offset(-0.0005), "-0.5 ms");
    for sec in [0.12, -0.0355, 1.2345] {
        close(parse_offset(&format_offset(sec)), (sec * 1e4_f64).round() / 1e4);
    }
}

#[test]
fn clamps_to_the_limit_and_rejects_non_finite() {
    assert_eq!(clamp_offset(500.0), MAX_OFFSET_SEC);
    assert_eq!(clamp_offset(-500.0), -MAX_OFFSET_SEC);
    assert_eq!(clamp_offset(f64::NAN), 0.0);
    assert_eq!(nudged(MAX_OFFSET_SEC, 1, true, false), MAX_OFFSET_SEC);
}

#[test]
fn nudge_steps_follow_the_modifiers() {
    assert_eq!(nudged(0.0, 1, false, false), 0.001);
    assert_eq!(nudged(0.0, -1, true, false), -0.01);
    assert_eq!(nudged(0.0, 1, false, true), 0.0001);
    assert_eq!(nudged(0.0, 1, true, true), 0.0001);
    let mut x = 0.0;
    for _ in 0..30 {
        x = nudged(x, 1, false, true);
    }
    assert_eq!(x, 0.003);
}

#[test]
fn fine_drag_is_ten_times_slower() {
    assert!((drag_delta_sec(100.0, 200.0, false) - 0.5).abs() < 1e-9);
    assert!((drag_delta_sec(100.0, 200.0, true) - 0.05).abs() < 1e-9);
}

#[test]
fn snaps_to_the_nearest_candidate_within_eight_pixels() {
    let pps = 100.0;
    let candidates = [0.30, 0.52, 0.55, 2.0];
    assert_eq!(snap_offset(0.54, &candidates, pps), (0.55, true));
    assert_eq!(snap_offset(0.52 + 0.004, &candidates, pps), (0.52, true));
    assert_eq!(snap_offset(1.0, &candidates, pps), (1.0, false));
    assert_eq!(snap_offset(0.54, &[], pps), (0.54, false));
    assert_eq!(snap_offset(0.3 + 0.0799, &candidates, pps), (0.3, true));
    let just_out = 0.3 + 0.0801;
    assert_eq!(snap_offset(just_out, &candidates, pps), (just_out, false));
}
