//! Sync offset text, nudging and drag snapping (pure).

pub const MAX_OFFSET_SEC: f64 = 60.0;
pub const SNAP_TOLERANCE_PX: f64 = 8.0;
const FINE_DRAG_FACTOR: f64 = 0.1;
const STORED_PRECISION: f64 = 1e4;

/// Seconds from text such as "+120", "-35.5", "120ms", "-0.5 ms" or "1,5". `None` if invalid.
pub fn parse_offset(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix("ms").or_else(|| t.strip_suffix("MS")).or_else(|| t.strip_suffix("Ms")).unwrap_or(t).trim();
    let unsigned = t.strip_prefix('+').unwrap_or(t);
    if unsigned.starts_with(['+', '-']) && t.starts_with('+') {
        return None;
    }
    let ms: f64 = unsigned.replace(',', ".").parse().ok()?;
    ms.is_finite().then_some(ms / 1000.0)
}

/// "+120 ms", "-35.5 ms": explicit sign, at most one decimal.
pub fn format_offset(sec: f64) -> String {
    let tenths = (sec * 10_000.0).round() as i64;
    if tenths % 10 == 0 {
        format!("{:+} ms", tenths / 10)
    } else {
        format!("{:+.1} ms", tenths as f64 / 10.0)
    }
}

pub fn clamp_offset(sec: f64) -> f64 {
    if sec.is_finite() {
        sec.clamp(-MAX_OFFSET_SEC, MAX_OFFSET_SEC)
    } else {
        0.0
    }
}

pub fn nudge_step_ms(shift: bool, ctrl: bool) -> f64 {
    if ctrl {
        0.1
    } else if shift {
        10.0
    } else {
        1.0
    }
}

/// `direction` is +1 or -1; the result is rounded to 0.1 ms so repeated nudges do not drift.
pub fn nudged(offset_sec: f64, direction: i32, shift: bool, ctrl: bool) -> f64 {
    let sec = offset_sec + f64::from(direction) * nudge_step_ms(shift, ctrl) / 1000.0;
    clamp_offset((sec * STORED_PRECISION).round() / STORED_PRECISION)
}

/// Drag movement in seconds for a pointer delta; Shift is 10x slower.
pub fn drag_delta_sec(dx_points: f32, pixels_per_sec: f64, fine: bool) -> f64 {
    f64::from(dx_points) / pixels_per_sec.max(f64::EPSILON) * if fine { FINE_DRAG_FACTOR } else { 1.0 }
}

/// The candidate within `SNAP_TOLERANCE_PX` of `raw` (nearest first), or `raw` itself; Alt disables
/// snapping. The flag tells whether it snapped.
pub fn snap_offset(raw: f64, candidates: &[f64], pixels_per_sec: f64, alt: bool) -> (f64, bool) {
    if alt {
        return (raw, false);
    }
    let reach = SNAP_TOLERANCE_PX / pixels_per_sec.max(f64::EPSILON);
    candidates
        .iter()
        .copied()
        .filter(|c| c.is_finite() && (c - raw).abs() <= reach)
        .min_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()))
        .map_or((raw, false), |c| (c, true))
}

#[cfg(test)]
mod tests {
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
    fn snaps_to_the_nearest_candidate_within_eight_pixels_unless_alt() {
        let pps = 100.0;
        let candidates = [0.30, 0.52, 0.55, 2.0];
        assert_eq!(snap_offset(0.54, &candidates, pps, false), (0.55, true));
        assert_eq!(snap_offset(0.52 + 0.004, &candidates, pps, false), (0.52, true));
        assert_eq!(snap_offset(0.54, &candidates, pps, true), (0.54, false));
        assert_eq!(snap_offset(1.0, &candidates, pps, false), (1.0, false));
        assert_eq!(snap_offset(0.54, &[], pps, false), (0.54, false));
        assert_eq!(snap_offset(0.3 + 0.0799, &candidates, pps, false), (0.3, true));
        let just_out = 0.3 + 0.0801;
        assert_eq!(snap_offset(just_out, &candidates, pps, false), (just_out, false));
    }
}
