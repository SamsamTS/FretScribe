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

/// The candidate within `SNAP_TOLERANCE_PX` of `raw` (nearest first), or `raw` itself (Alt: the
/// caller passes no candidates). The flag tells whether it snapped.
pub fn snap_offset(raw: f64, candidates: &[f64], pixels_per_sec: f64) -> (f64, bool) {
    let reach = SNAP_TOLERANCE_PX / pixels_per_sec.max(f64::EPSILON);
    candidates
        .iter()
        .copied()
        .filter(|c| c.is_finite() && (c - raw).abs() <= reach)
        .min_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()))
        .map_or((raw, false), |c| (c, true))
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/tracks/offset.rs"]
mod tests;
