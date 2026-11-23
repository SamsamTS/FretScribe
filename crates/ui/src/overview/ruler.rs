//! Time ruler: tick spacing that adapts to the width, and `m:ss` labels.

const STEPS: [f64; 13] = [
    1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0,
];
pub(crate) const MIN_LABEL_SPACING_PX: f64 = 64.0;
pub(crate) const MIN_MINOR_SPACING_PX: f64 = 8.0;

/// The smallest nice step whose ticks are at least `min_spacing_px` apart.
pub fn tick_step(pixels_per_sec: f64, min_spacing_px: f64) -> f64 {
    STEPS
        .iter()
        .copied()
        .find(|step| step * pixels_per_sec >= min_spacing_px)
        .unwrap_or(STEPS[STEPS.len() - 1])
}

/// Multiples of `step` in `[0, end_sec]`.
pub fn ticks(end_sec: f64, step: f64) -> Vec<f64> {
    if !(step > 0.0 && end_sec.is_finite() && end_sec >= 0.0) {
        return Vec::new();
    }
    let count = (end_sec / step).floor() as usize;
    (0..=count).map(|k| k as f64 * step).collect()
}

/// Number of minor intervals per major step, 1 when they would be too close together.
pub(crate) fn minor_divisions(step: f64, pixels_per_sec: f64) -> u32 {
    let divisions = if step >= 60.0 { if step % 300.0 == 0.0 { 5 } else { 2 } } else if step >= 10.0 { 5 } else { 1 };
    if step / f64::from(divisions) * pixels_per_sec >= MIN_MINOR_SPACING_PX {
        divisions
    } else {
        1
    }
}

/// `m:ss`, or `h:mm:ss` from one hour; fractions are rounded to the nearest second.
pub fn format_time(sec: f64) -> String {
    if !sec.is_finite() {
        return "?".to_owned();
    }
    let total = sec.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/overview/ruler.rs"]
mod tests;
