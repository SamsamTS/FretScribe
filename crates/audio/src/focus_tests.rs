//! The guitar focus filter measured through the real engine, one block at a time.

use std::f64::consts::TAU;

use fretscribe_core::Tuning;

use crate::engine::Engine;
use crate::testutil::*;
use crate::{FocusBand, Player, SpeedMode};

const AMP: f64 = 0.2;

fn index(sec: f64) -> usize {
    (sec * f64::from(RATE)).round() as usize
}

/// Amplitude of one tone over a whole number of its periods, so there is no leakage.
fn amplitude(x: &[f32], freq: f64) -> f64 {
    let periods = (x.len() as f64 * freq / f64::from(RATE)).floor().max(1.0);
    let n = ((periods * f64::from(RATE) / freq).round() as usize).min(x.len());
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &s) in x[..n].iter().enumerate() {
        let phase = TAU * freq * i as f64 / f64::from(RATE);
        re += f64::from(s) * phase.cos();
        im += f64::from(s) * phase.sin();
    }
    2.0 * re.hypot(im) / n as f64
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

fn tones(parts: &[(f64, f64)], seconds: f64) -> Vec<f32> {
    let frames = (seconds * f64::from(RATE)) as usize;
    (0..frames)
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            parts.iter().map(|&(f, a)| a * (TAU * f * t).sin()).sum::<f64>() as f32
        })
        .collect()
}

fn natural_step(parts: &[(f64, f64)]) -> f32 {
    parts.iter().map(|&(f, a)| a * TAU * f / f64::from(RATE)).sum::<f64>() as f32
}

/// Gain of a steady tone through the engine, in dB, measured once the ramps have settled.
fn gain_db(freq: f64, strength: f32, configure: impl Fn(&Player)) -> f64 {
    let (player, mut engine) = setup(vec![track(1, tones(&[(freq, AMP)], 3.0))]);
    configure(&player);
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(strength);
    player.play();
    let r = render_sec(&player, &mut engine, 1.8);
    db(amplitude(&r.left[index(0.9)..index(0.9) + index(0.8)], freq) / AMP)
}

fn plain_gain_db(freq: f64, strength: f32) -> f64 {
    gain_db(freq, strength, |_| {})
}

fn band() -> FocusBand {
    FocusBand::default()
}

#[test]
fn full_strength_response_matches_the_design() {
    let band = band();
    let (low_corner, high_corner) = (0.6 * f64::from(band.low_hz), 4.0 * f64::from(band.high_hz));

    let lowest = plain_gain_db(f64::from(band.low_hz), 1.0);
    let highest = plain_gain_db(f64::from(band.high_hz), 1.0);
    let middle = plain_gain_db(440.0, 1.0);
    let third_harmonic = plain_gain_db(3.0 * f64::from(band.high_hz), 1.0);
    let below = plain_gain_db(low_corner / 2.0, 1.0);
    let above = plain_gain_db(high_corner * 2.0, 1.0);
    println!("focus response: E2 {lowest:.2}  440 {middle:.2}  E6 {highest:.2}  3xE6 {third_harmonic:.2}  octave below corner {below:.2}  octave above corner {above:.2}");

    assert!(lowest > -1.5, "lowest open string {lowest}");
    assert!(highest > -0.5, "highest fundamental {highest}");
    assert!(middle.abs() < 0.5, "mid band {middle}");
    assert!(third_harmonic > -3.0, "guitar harmonics {third_harmonic}");
    assert!(below <= -20.0, "octave below the low corner {below}");
    assert!(above <= -20.0, "octave above the high corner {above}");
}

#[test]
fn half_strength_cuts_about_half_the_decibels() {
    for freq in [24.7, 10_550.0] {
        let full = -plain_gain_db(freq, 1.0);
        let half = -plain_gain_db(freq, 0.5);
        assert!((half - full / 2.0).abs() < 3.0, "{freq} Hz: {half} vs {full}");
    }
}

#[test]
fn the_cut_grows_monotonically_with_strength() {
    for freq in [30.0, 12_000.0] {
        let cuts: Vec<f64> = [0.0f32, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|&s| -plain_gain_db(freq, s))
            .collect();
        assert!(cuts[0].abs() < 0.01, "{freq} Hz at strength 0: {}", cuts[0]);
        assert!(cuts.windows(2).all(|w| w[1] > w[0] + 1.0), "{freq} Hz: {cuts:?}");
    }
}

#[test]
fn the_band_follows_tuning_and_capo() {
    let standard = plain_gain_db(60.0, 1.0);
    let drop_d = gain_db(60.0, 1.0, |p| {
        p.set_focus_context(
            &Tuning {
                name: "Drop D".to_owned(),
                open_strings: vec![38, 45, 50, 55, 59, 64],
            },
            0,
        );
    });
    assert!(drop_d > standard + 0.5, "drop D {drop_d} vs standard {standard}");

    let lowest = f64::from(band().low_hz);
    let capoed = gain_db(lowest, 1.0, |p| p.set_focus_context(&Tuning::standard(), 5));
    assert!(capoed < plain_gain_db(lowest, 1.0) - 1.0, "capo 5 at the old lowest note: {capoed}");
}

#[test]
fn it_filters_the_sum_of_the_tracks() {
    let (player, mut engine) = setup(vec![
        track(1, tones(&[(440.0, AMP)], 3.0)),
        track(2, tones(&[(30.0, AMP)], 3.0)),
    ]);
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(1.0);
    player.play();
    let r = render_sec(&player, &mut engine, 1.8);
    let window = &r.left[index(0.9)..index(1.7)];
    assert!(db(amplitude(window, 440.0) / AMP).abs() < 0.5);
    assert!(db(amplitude(window, 30.0) / AMP) < -12.0);
}

#[test]
fn the_note_overlay_is_never_filtered() {
    let render_with = |focus: bool| {
        let (player, mut engine) = setup(vec![track(1, dc(3.0, 0.0))]);
        player.set_note_overlay(vec![(40, 0.0, 2.0), (69, 0.2, 2.0)], 1.0);
        player.set_guitar_focus(focus);
        player.set_guitar_focus_strength(1.0);
        player.play();
        render_sec(&player, &mut engine, 1.0).left
    };
    let off = render_with(false);
    assert!(rms(&off[index(0.1)..]) > 0.01);
    assert_eq!(render_with(true), off);
}

fn mixed_tones() -> Vec<(f64, f64)> {
    vec![(110.0, 0.25), (880.0, 0.25), (30.0, 0.1)]
}

fn mixed_setup() -> (Player, Engine) {
    setup(vec![track(1, tones(&mixed_tones(), 8.0))])
}

#[test]
fn off_or_zero_strength_is_bit_exact_and_does_not_move_the_position() {
    let baseline = {
        let (player, mut engine) = mixed_setup();
        player.play();
        render_sec(&player, &mut engine, 1.5)
    };

    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(0.0);
    player.play();
    let zero = render_sec(&player, &mut engine, 1.5);
    assert_eq!(zero.left, baseline.left);
    assert_eq!(zero.positions, baseline.positions);

    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus(false);
    player.play();
    assert_eq!(render_sec(&player, &mut engine, 1.5).left, baseline.left);

    let (player, mut engine) = mixed_setup();
    player.play();
    let mut left = render_sec(&player, &mut engine, 0.3).left;
    player.set_guitar_focus(true);
    left.extend(render_sec(&player, &mut engine, 0.5).left);
    player.set_guitar_focus(false);
    left.extend(render_sec(&player, &mut engine, 0.7).left);
    assert_ne!(left[index(0.5)..index(0.7)], baseline.left[index(0.5)..index(0.7)]);
    assert_eq!(left[index(1.1)..], baseline.left[index(1.1)..]);
}

#[test]
fn clamping_and_non_finite_values() {
    let render_with = |strengths: &[f32]| {
        let (player, mut engine) = mixed_setup();
        player.set_guitar_focus(true);
        for &s in strengths {
            player.set_guitar_focus_strength(s);
        }
        player.play();
        render_sec(&player, &mut engine, 0.6).left
    };
    assert_eq!(render_with(&[5.0]), render_with(&[1.0]));
    assert_eq!(render_with(&[-2.0]), render_with(&[0.0]));
    assert_eq!(render_with(&[f32::NAN, f32::INFINITY]), render_with(&[]));
}

fn bound(parts: &[(f64, f64)], factor: f32) -> f32 {
    natural_step(parts) * factor
}

#[test]
fn toggling_while_playing_is_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus_strength(1.0);
    player.play();
    let mut all = render_sec(&player, &mut engine, 0.2).left;
    for step in 0..10 {
        player.set_guitar_focus(step % 2 == 0);
        all.extend(render_sec(&player, &mut engine, 0.1 + 0.013 * f64::from(step)).left);
    }
    let step = max_step(&all);
    assert!(step < bound(&mixed_tones(), 1.25), "{step} vs {}", natural_step(&mixed_tones()));
}

#[test]
fn strength_sweeps_are_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.play();
    let mut all = render_sec(&player, &mut engine, 0.2).left;
    for i in (0..=60).chain((0..=60).rev()) {
        player.set_guitar_focus_strength(i as f32 / 60.0);
        all.extend(render(&player, &mut engine, 13).left);
    }
    let step = max_step(&all);
    assert!(step < bound(&mixed_tones(), 1.25), "{step}");
}

#[test]
fn context_changes_while_playing_are_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(1.0);
    player.play();
    let mut all = render_sec(&player, &mut engine, 0.3).left;
    let drop_d = Tuning {
        name: "Drop D".to_owned(),
        open_strings: vec![38, 45, 50, 55, 59, 64],
    };
    let seven = Tuning {
        name: "7".to_owned(),
        open_strings: vec![35, 40, 45, 50, 55, 59, 64],
    };
    for (tuning, capo) in [(&drop_d, 0), (&Tuning::standard(), 5), (&seven, 0), (&Tuning::standard(), 0)] {
        player.set_focus_context(tuning, capo);
        all.extend(render_sec(&player, &mut engine, 0.25).left);
    }
    player.set_reference_pitch(fretscribe_core::PitchRef::new(432.0).unwrap());
    all.extend(render_sec(&player, &mut engine, 0.25).left);
    let step = max_step(&all);
    assert!(step < bound(&mixed_tones(), 1.25), "{step}");
}

#[test]
fn speed_and_mode_switches_with_the_filter_on_are_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(1.0);
    player.play();
    let mut all = render_sec(&player, &mut engine, 0.3).left;
    for (speed, mode) in [
        (0.5, SpeedMode::ChangePitch),
        (0.5, SpeedMode::PreservePitch),
        (2.0, SpeedMode::ChangePitch),
        (1.25, SpeedMode::ChangePitch),
        (1.0, SpeedMode::PreservePitch),
    ] {
        player.set_playback_speed(speed);
        player.set_speed_mode(mode);
        all.extend(render_sec(&player, &mut engine, 0.4).left);
    }
    let step = max_step(&all);
    assert!(step < bound(&mixed_tones(), 2.0 * 1.25), "{step}");
}

#[test]
fn seeks_and_loop_wraps_with_the_filter_on_are_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(1.0);
    player.set_region(Some((1.0, 1.2)));
    player.set_loop(true);
    player.play();
    let mut all = render_sec(&player, &mut engine, 1.5).left;
    player.set_region(None);
    player.seek(4.321);
    all.extend(render_sec(&player, &mut engine, 0.5).left);
    let step = max_step(&all);
    assert!(step < bound(&mixed_tones(), 3.0), "{step}");
}

#[test]
fn pausing_and_resuming_with_the_filter_on_is_click_free() {
    let (player, mut engine) = mixed_setup();
    player.set_guitar_focus(true);
    player.set_guitar_focus_strength(1.0);
    player.play();
    render_sec(&player, &mut engine, 0.4137);
    player.pause();
    let paused = render_sec(&player, &mut engine, 0.3);
    assert!(paused.left[index(0.1)..].iter().all(|&s| s == 0.0));
    player.play();
    let resumed = render_sec(&player, &mut engine, 0.5);
    let step = max_step(&resumed.left);
    assert!(step < bound(&mixed_tones(), 1.25), "{step}");
}

#[test]
fn change_pitch_slowdown_keeps_a_guitar_note_inside_the_band() {
    let measure = |mode: SpeedMode, speed: f32, source_hz: f64, heard_hz: f64| {
        let (player, mut engine) = setup(vec![track(1, tones(&[(source_hz, AMP)], 6.0))]);
        player.set_speed_mode(mode);
        player.set_playback_speed(speed);
        player.set_guitar_focus(true);
        player.set_guitar_focus_strength(1.0);
        player.play();
        let r = render_sec(&player, &mut engine, 2.5);
        db(amplitude(&r.left[index(1.2)..index(2.4)], heard_hz) / AMP)
    };

    let unscaled_reference = plain_gain_db(55.0, 1.0);
    assert!(unscaled_reference < -1.5, "55 Hz sits near the fixed corner: {unscaled_reference}");

    let slowed_a2 = measure(SpeedMode::ChangePitch, 0.5, 110.0, 55.0);
    assert!(slowed_a2 > -1.5, "A2 slowed to 55 Hz must stay in band: {slowed_a2}");

    let preserved = measure(SpeedMode::PreservePitch, 0.5, 55.0, 55.0);
    assert!((preserved - unscaled_reference).abs() < 1.5, "{preserved} vs {unscaled_reference}");
}
