//! Time ruler: tick spacing that adapts to the width, and `m:ss` labels.

const STEPS: [f64; 13] = [
    1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0,
];
pub const MIN_LABEL_SPACING_PX: f64 = 64.0;
pub const MIN_MINOR_SPACING_PX: f64 = 8.0;

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
pub fn minor_divisions(step: f64, pixels_per_sec: f64) -> u32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.4), "1:05");
        assert_eq!(format_time(599.6), "10:00");
        assert_eq!(format_time(3600.0), "1:00:00");
        assert_eq!(format_time(3725.0), "1:02:05");
        assert_eq!(format_time(-4.0), "0:00");
        assert_eq!(format_time(f64::NAN), "?");
    }

    #[test]
    fn step_adapts_to_the_width() {
        // 380 s over 1000 px
        assert_eq!(tick_step(1000.0 / 380.0, MIN_LABEL_SPACING_PX), 30.0);
        // the same audio on a narrow strip
        assert_eq!(tick_step(300.0 / 380.0, MIN_LABEL_SPACING_PX), 120.0);
        assert_eq!(tick_step(100.0, MIN_LABEL_SPACING_PX), 1.0);
        assert_eq!(tick_step(0.0001, MIN_LABEL_SPACING_PX), 3600.0);
    }

    #[test]
    fn chosen_step_keeps_labels_apart() {
        for width in [200.0, 640.0, 1000.0, 1920.0, 3840.0] {
            for len in [5.0, 60.0, 380.0, 1800.0, 7200.0] {
                let pps = width / len;
                let step = tick_step(pps, MIN_LABEL_SPACING_PX);
                assert!(step * pps >= MIN_LABEL_SPACING_PX || step == 3600.0, "{width} {len}");
            }
        }
    }

    #[test]
    fn ticks_are_multiples_inside_the_audio() {
        assert_eq!(ticks(65.0, 30.0), [0.0, 30.0, 60.0]);
        assert_eq!(ticks(60.0, 30.0), [0.0, 30.0, 60.0]);
        assert_eq!(ticks(0.0, 5.0), [0.0]);
        assert!(ticks(10.0, 0.0).is_empty() && ticks(-1.0, 5.0).is_empty() && ticks(f64::NAN, 5.0).is_empty());
    }

    #[test]
    fn minor_ticks_only_when_there_is_room() {
        assert_eq!(minor_divisions(30.0, 3.0), 5);
        assert_eq!(minor_divisions(30.0, 0.5), 1);
        assert_eq!(minor_divisions(1.0, 100.0), 1);
        assert_eq!(minor_divisions(60.0, 1.0), 2);
        assert_eq!(minor_divisions(300.0, 1.0), 5);
    }
}
