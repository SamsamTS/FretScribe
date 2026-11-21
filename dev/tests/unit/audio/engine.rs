use super::*;

#[test]
fn ramp_reaches_target_in_the_requested_frames() {
    let mut ramp = Ramp::new(0.0);
    ramp.set(1.0, 100.0);
    let values: Vec<f32> = (0..120).map(|_| ramp.next()).collect();
    assert!((values[49] - 0.5).abs() < 1e-5);
    assert_eq!(values[99], 1.0);
    assert_eq!(values[119], 1.0);
    ramp.set(0.25, 50.0);
    let first = ramp.next();
    assert!((first - (1.0 - 0.75 / 50.0)).abs() < 1e-6);
    let steps = std::iter::repeat_with(|| ramp.next()).take(60).collect::<Vec<_>>();
    assert!(steps.windows(2).all(|w| w[1] <= w[0]));
    assert_eq!(steps[48], 0.25);
    assert_eq!(*steps.last().unwrap(), 0.25);
}

#[test]
fn soft_limiter_is_transparent_bounded_and_monotonic() {
    for x in [-0.8f32, -0.3, 0.0, 0.5, 0.8] {
        assert_eq!(soft_limit(x), x);
    }
    let mut previous = soft_limit(0.8);
    for i in 0..400 {
        let x = 0.8 + i as f32 * 0.05;
        let y = soft_limit(x);
        assert!(y >= previous && y <= 1.0, "{x} {y}");
        assert_eq!(soft_limit(-x), -y);
        previous = y;
    }
}

#[test]
fn direct_voice_counts_samples_exactly() {
    let mut voice = Voice::new(1_000.0, 0.5, SpeedMode::ChangePitch, 0, (2_880.0, 2_000.0));
    assert!(!voice.stretching);
    for _ in 0..1_000 {
        assert_eq!(voice.plan(64), 0);
        voice.advance(64, 0);
    }
    assert_eq!(voice.pos, 1_000.0 + 64_000.0 * 0.5);
}

#[test]
fn stretched_voice_consumes_the_right_number_of_input_frames() {
    for speed in [0.25, 0.5, 0.8, 1.5, 2.0] {
        let mut voice = Voice::new(5_000.0, speed, SpeedMode::PreservePitch, 1, (2_880.0, 2_000.0));
        assert!(voice.stretching);
        let start = voice.pos;
        assert!((start - 5_000.0).abs() <= 0.5);
        let mut total = 0usize;
        let mut frames = 0usize;
        for n in [64, 7, 64, 13, 4, 64, 64] {
            let m = voice.plan(n);
            assert!(m >= 1, "speed {speed} chunk {n}");
            voice.advance(n, m);
            total += m;
            frames += n;
        }
        assert!((total as f64 - frames as f64 * speed).abs() < 1.0);
        assert!((voice.pos - start - frames as f64 * speed).abs() < 1.0);
    }
}

#[test]
fn unity_speed_bypasses_the_stretcher_in_both_modes() {
    assert!(!stretches(SpeedMode::PreservePitch, 1.0));
    assert!(!stretches(SpeedMode::ChangePitch, 0.5));
    assert!(stretches(SpeedMode::PreservePitch, 0.5));
}
