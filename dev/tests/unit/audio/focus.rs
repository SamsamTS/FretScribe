use super::*;

fn tuning(open: &[u8]) -> Tuning {
    Tuning {
        name: "test".to_owned(),
        open_strings: open.to_vec(),
    }
}

fn reference(hz: f32) -> PitchRef {
    PitchRef::new(hz).unwrap()
}

fn close(a: f32, b: f32, tolerance: f32) {
    assert!((a - b).abs() <= tolerance, "{a} vs {b}");
}

#[test]
fn standard_tuning_at_440() {
    let band = focus_band(&Tuning::standard(), 0, &PitchRef::default());
    close(band.low_hz, 82.41, 0.01);
    close(band.high_hz, 1318.51, 0.05);
    assert_eq!(band, FocusBand::default());
}

#[test]
fn alternate_tunings_and_string_counts() {
    let drop_d = focus_band(&tuning(&[38, 45, 50, 55, 59, 64]), 0, &PitchRef::default());
    close(drop_d.low_hz, 73.42, 0.01);
    close(drop_d.high_hz, 1318.51, 0.05);
    let seven = focus_band(&tuning(&[35, 40, 45, 50, 55, 59, 64]), 0, &PitchRef::default());
    close(seven.low_hz, 61.74, 0.01);
    let empty = focus_band(&tuning(&[]), 0, &PitchRef::default());
    assert_eq!(empty, FocusBand::default());
}

#[test]
fn capo_shifts_both_edges_up() {
    let plain = focus_band(&Tuning::standard(), 0, &PitchRef::default());
    let capoed = focus_band(&Tuning::standard(), 2, &PitchRef::default());
    let two_semitones = 2f32.powf(2.0 / 12.0);
    close(capoed.low_hz / plain.low_hz, two_semitones, 1e-4);
    close(capoed.high_hz / plain.high_hz, two_semitones, 1e-4);
}

#[test]
fn reference_pitch_scales_every_edge() {
    let plain = focus_band(&Tuning::standard(), 0, &PitchRef::default());
    let tuned = focus_band(&Tuning::standard(), 0, &reference(432.0));
    close(tuned.low_hz / plain.low_hz, 432.0 / 440.0, 1e-4);
    close(tuned.high_hz / plain.high_hz, 432.0 / 440.0, 1e-4);
}

#[test]
fn absurd_capo_stays_finite() {
    let band = focus_band(&Tuning::standard(), 255, &PitchRef::default());
    assert!(band.low_hz.is_finite() && band.high_hz.is_finite());
}

#[test]
fn strength_maps_to_a_monotonic_blend() {
    assert_eq!(blend_amount(0.0), 0.0);
    assert_eq!(blend_amount(-3.0), 0.0);
    let full = blend_amount(1.0);
    assert!((f64::from(1.0 - full) - 10f64.powf(-24.0 / 20.0)).abs() < 1e-6);
    assert_eq!(blend_amount(7.0), full);
    let steps: Vec<f32> = (0..=10).map(|i| blend_amount(i as f32 / 10.0)).collect();
    assert!(steps.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn corners_follow_the_band_and_stay_below_nyquist() {
    let band = FocusBand::default();
    let c = corners(&band, 48_000.0, 1.0);
    assert!((c.low - 0.6 * f64::from(band.low_hz)).abs() < 1e-6);
    assert!((c.high - 4.0 * f64::from(band.high_hz)).abs() < 1e-6);
    let slow = corners(&band, 48_000.0, 0.5);
    assert!((slow.low / c.low - 0.5).abs() < 1e-9 && (slow.high / c.high - 0.5).abs() < 1e-9);
    let low_rate = corners(&band, 8_000.0, 1.0);
    assert!(low_rate.high <= 0.45 * 8_000.0 && low_rate.low <= low_rate.high / 4.0);
    let extreme = corners(&band, 48_000.0, 0.25);
    assert!(extreme.low >= MIN_CORNER_HZ);
}

#[test]
fn inactive_filter_leaves_the_buffer_untouched() {
    let mut filter = FocusFilter::new(48_000, FocusBand::default());
    let original: Vec<f32> = (0..256).map(|i| (i as f32 * 0.37).sin()).collect();
    let mut buffer = original.clone();
    filter.process(&mut buffer);
    assert_eq!(buffer, original);

    filter.set_enabled(true);
    filter.set_strength(0.0);
    filter.process(&mut buffer);
    assert_eq!(buffer, original);
}

#[test]
fn enabling_then_disabling_returns_to_a_bit_exact_bypass() {
    let mut filter = FocusFilter::new(48_000, FocusBand::default());
    let original: Vec<f32> = (0..2 * 4_800).map(|i| (i as f32 * 0.013).sin() * 0.3).collect();
    filter.set_enabled(true);
    let mut buffer = original.clone();
    filter.process(&mut buffer);
    assert_ne!(buffer, original);
    filter.set_enabled(false);
    let mut settle = original.clone();
    filter.process(&mut settle);
    let mut after = original.clone();
    filter.process(&mut after);
    assert_eq!(after, original);
    assert!(filter.current.is_none());
}

#[test]
fn retunes_during_a_fade_keep_only_the_latest_request() {
    let mut filter = FocusFilter::new(48_000, FocusBand::default());
    filter.set_enabled(true);
    filter.set_speed_scale(0.5);
    assert!(filter.incoming.is_some());
    filter.set_speed_scale(0.8);
    filter.set_speed_scale(1.5);
    assert_eq!(filter.pending, Some(corners(&FocusBand::default(), 48_000.0, 1.5)));
    let mut buffer = vec![0.1; 2 * 4_800];
    filter.process(&mut buffer);
    let settled = filter.current.as_ref().map(|c| c.corners);
    assert_eq!(settled, Some(corners(&FocusBand::default(), 48_000.0, 1.5)));
    assert!(filter.incoming.is_none() && filter.pending.is_none());
}

#[test]
fn tiny_retunes_are_ignored() {
    let mut filter = FocusFilter::new(48_000, FocusBand::default());
    filter.set_enabled(true);
    filter.set_speed_scale(1.01);
    assert!(filter.incoming.is_none());
}

