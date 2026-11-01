//! Behavioural tests of the whole engine, rendered offline block by block through `Player::headless`.

use fretscribe_core::PitchRef;

use crate::testutil::*;
use crate::{AudioError, Player, SpeedMode, TrackAudio};

const SKIP_FADE_IN: usize = 480;

fn ramp(seconds: f64, slope: f32) -> Vec<f32> {
    let frames = (seconds * f64::from(RATE)) as usize;
    (0..frames).map(|i| slope * i as f32 / RATE as f32).collect()
}

fn index(sec: f64) -> usize {
    (sec * f64::from(RATE)).round() as usize
}

fn last(r: &Rendered) -> f32 {
    r.left[r.left.len() - 1]
}

fn freq_of(r: &Rendered, from: f64, to: f64, low: f64, high: f64) -> f64 {
    pitch_by_autocorrelation(&r.left[index(from)..index(to)], RATE, low, high)
}

#[test]
fn player_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Player>();
}

mod speed {
    use super::*;

    #[test]
    fn position_follows_the_sample_counter_at_every_speed() {
        for mode in [SpeedMode::PreservePitch, SpeedMode::ChangePitch] {
            for speed in [0.25f32, 0.5, 0.8, 1.0, 1.5, 2.0] {
                let (player, mut engine) = setup(vec![track(1, sine(220.0, RATE, 30.0, 0.3))]);
                player.set_speed_mode(mode);
                player.set_playback_speed(speed);
                player.seek(2.0);
                player.play();
                let r = render_sec(&player, &mut engine, 3.0);
                let expected = 2.0 + 3.0 * f64::from(speed);
                let got = r.positions[r.positions.len() - 1];
                assert!(
                    (got - expected).abs() < 0.004,
                    "{mode:?} x{speed}: {got} vs {expected}"
                );
                assert!(r.positions.windows(2).all(|w| w[1] >= w[0]), "{mode:?} x{speed}");
                assert!(player.is_playing());
            }
        }
    }

    #[test]
    fn position_is_accurate_at_every_point_of_a_run() {
        let (player, mut engine) = setup(vec![track(1, sine(220.0, RATE, 30.0, 0.3))]);
        player.set_playback_speed(0.5);
        player.play();
        let r = render_sec(&player, &mut engine, 4.0);
        for (i, &position) in r.positions.iter().enumerate().skip(40) {
            let expected = (i + 1) as f64 * 64.0 / f64::from(RATE) * 0.5;
            assert!((position - expected).abs() < 0.004, "block {i}: {position} vs {expected}");
        }
    }

    /// The search band is narrow so that sub-harmonic autocorrelation peaks cannot win.
    fn measure(mode: SpeedMode, speed: f32, expected: f64) -> (f64, f32) {
        let (player, mut engine) = setup(vec![track(1, sine(440.0, RATE, 20.0, 0.5))]);
        player.set_speed_mode(mode);
        player.set_playback_speed(speed);
        player.play();
        let r = render_sec(&player, &mut engine, 2.5);
        let window = &r.left[index(0.5)..index(2.0)];
        (freq_of(&r, 0.5, 2.0, expected * 0.8, expected * 1.25), rms(window))
    }

    #[test]
    fn half_speed_keeps_440_hz_when_preserving_pitch() {
        let (freq, level) = measure(SpeedMode::PreservePitch, 0.5, 440.0);
        assert!((freq - 440.0).abs() < 2.0, "{freq}");
        assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.03, "{level}");
    }

    #[test]
    fn half_speed_drops_to_220_hz_when_changing_pitch() {
        let (freq, level) = measure(SpeedMode::ChangePitch, 0.5, 220.0);
        assert!((freq - 220.0).abs() < 1.0, "{freq}");
        assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.01, "{level}");
    }

    #[test]
    fn other_speeds_follow_the_selected_mode() {
        for (mode, speed, expected) in [
            (SpeedMode::PreservePitch, 2.0, 440.0),
            (SpeedMode::PreservePitch, 1.5, 440.0),
            (SpeedMode::PreservePitch, 0.25, 440.0),
            (SpeedMode::ChangePitch, 2.0, 880.0),
            (SpeedMode::ChangePitch, 1.5, 660.0),
            (SpeedMode::ChangePitch, 0.25, 110.0),
        ] {
            let (freq, _) = measure(mode, speed, expected);
            assert!((freq - expected).abs() < expected * 0.007, "{mode:?} x{speed}: {freq}");
        }
    }

    #[test]
    fn unity_speed_bypasses_the_stretcher_bit_exactly() {
        let source = sine(440.0, RATE, 5.0, 0.5);
        let (player, mut engine) = setup(vec![track(1, source.clone())]);
        player.set_speed_mode(SpeedMode::PreservePitch);
        player.play();
        let r = render_sec(&player, &mut engine, 1.0);
        for (i, (heard, original)) in r.left.iter().zip(&source).enumerate().skip(SKIP_FADE_IN) {
            assert_eq!(heard, original, "frame {i}");
        }
    }

    #[test]
    fn speed_is_clamped_to_the_supported_range() {
        let (player, mut engine) = setup(vec![track(1, sine(220.0, RATE, 30.0, 0.3))]);
        player.set_speed_mode(SpeedMode::ChangePitch);
        player.set_playback_speed(10.0);
        player.set_playback_speed(f32::NAN);
        player.play();
        let r = render_sec(&player, &mut engine, 1.0);
        assert!((r.positions[r.positions.len() - 1] - 2.0).abs() < 0.004);
        player.set_playback_speed(0.01);
        let r = render_sec(&player, &mut engine, 1.0);
        let advanced = r.positions[r.positions.len() - 1] - 2.0;
        assert!((advanced - 0.25).abs() < 0.01, "{advanced}");
    }

    #[test]
    fn switching_speed_and_mode_while_playing_is_click_free() {
        let (player, mut engine) = setup(vec![track(1, sine(440.0, RATE, 30.0, 0.5))]);
        player.play();
        let mut all = render_sec(&player, &mut engine, 0.4).left;
        let schedule: [(f32, SpeedMode); 8] = [
            (0.5, SpeedMode::PreservePitch),
            (0.5, SpeedMode::ChangePitch),
            (1.5, SpeedMode::ChangePitch),
            (1.5, SpeedMode::PreservePitch),
            (2.0, SpeedMode::PreservePitch),
            (0.25, SpeedMode::PreservePitch),
            (1.0, SpeedMode::PreservePitch),
            (1.0, SpeedMode::ChangePitch),
        ];
        for (speed, mode) in schedule {
            player.set_playback_speed(speed);
            player.set_speed_mode(mode);
            all.extend(render_sec(&player, &mut engine, 0.4).left);
        }
        let natural = 0.5 * std::f32::consts::TAU * 880.0 / RATE as f32;
        let step = max_step(&all);
        assert!(step < natural * 1.6, "largest step {step}, natural {natural}");
        assert!(all.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn rapid_speed_changes_stay_finite_and_bounded() {
        let (player, mut engine) = setup(vec![track(1, sine(330.0, RATE, 30.0, 0.5))]);
        player.play();
        let mut all = Vec::new();
        for i in 0..60 {
            player.set_playback_speed(0.25 + (i % 7) as f32 * 0.25);
            if i % 3 == 0 {
                player.set_speed_mode(SpeedMode::ChangePitch);
            } else {
                player.set_speed_mode(SpeedMode::PreservePitch);
            }
            all.extend(render(&player, &mut engine, 17).left);
        }
        assert!(all.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(max_step(&all) < 0.2, "{}", max_step(&all));
    }

    #[test]
    fn seeking_while_playing_crossfades_without_a_click() {
        let (player, mut engine) = setup(vec![track(1, sine(440.0, RATE, 30.0, 0.5))]);
        player.play();
        let mut all = render_sec(&player, &mut engine, 0.3).left;
        player.seek(10.37);
        let r = render_sec(&player, &mut engine, 0.3);
        assert!(r.positions[r.positions.len() - 1] > 10.6);
        all.extend(r.left);
        assert!(max_step(&all) < 0.5 * std::f32::consts::TAU * 440.0 / RATE as f32 * 3.0);
    }
}

mod mixer {
    use super::*;

    #[test]
    fn offsets_place_each_track_in_project_time() {
        for offset in [0.5f64, -0.5, 0.0, 1.0] {
            let mut t = track(1, ramp(2.0, 0.1));
            t.offset_sec = offset;
            let (player, mut engine) = setup(vec![t]);
            assert!((player.duration_sec() - (2.0 + offset)).abs() < 1e-6);
            player.play();
            let r = render_sec(&player, &mut engine, 4.0);
            for k in 0..380 {
                let time = 0.015 + k as f64 * 0.01;
                let audio_time = time - offset;
                let expected = if (0.003..1.99).contains(&audio_time) {
                    0.1 * audio_time as f32
                } else if !(-0.003..=2.003).contains(&audio_time) {
                    0.0
                } else {
                    continue;
                };
                let got = r.left[index(time)];
                assert!((got - expected).abs() < 2e-4, "offset {offset} t={time}: {got} vs {expected}");
            }
            assert!(!player.is_playing(), "offset {offset}");
            assert!((player.position_sec() - (2.0 + offset)).abs() < 0.003);
        }
    }

    #[test]
    fn a_negative_offset_skips_the_start_of_the_file() {
        let mut t = track(1, ramp(2.0, 0.1));
        t.offset_sec = -0.5;
        let (player, mut engine) = setup(vec![t]);
        player.play();
        let r = render_sec(&player, &mut engine, 0.1);
        assert!((r.left[index(0.05)] - 0.1 * 0.55).abs() < 2e-4);
    }

    #[test]
    fn tracks_with_different_offsets_are_summed_in_project_time() {
        let mut a = track(1, ramp(1.0, 0.2));
        a.offset_sec = 0.2;
        let mut b = track(2, ramp(1.0, 0.1));
        b.offset_sec = -0.1;
        let (player, mut engine) = setup(vec![a, b]);
        assert!((player.duration_sec() - 1.2).abs() < 1e-6);
        player.play();
        let r = render_sec(&player, &mut engine, 1.3);
        for time in [0.05f64, 0.1, 0.3, 0.6, 0.9, 1.05] {
            let from_a = if time >= 0.2 { 0.2 * (time - 0.2) } else { 0.0 };
            let from_b = if time + 0.1 < 1.0 { 0.1 * (time + 0.1) } else { 0.0 };
            let got = r.left[index(time)];
            assert!((got - (from_a + from_b) as f32).abs() < 3e-4, "t={time}: {got}");
        }
    }

    #[test]
    fn different_sample_rates_are_resampled_and_stay_aligned() {
        let slope = 0.1;
        let source: Vec<f32> = (0..88_200).map(|i| slope * i as f32 / 44_100.0).collect();
        let t = TrackAudio {
            id: 7,
            audio: mono_audio(44_100, source),
            offset_sec: 0.25,
            volume: 1.0,
            muted: false,
            solo: false,
        };
        let (player, mut engine) = setup(vec![t]);
        assert!((player.duration_sec() - 2.25).abs() < 1e-3);
        player.play();
        let r = render_sec(&player, &mut engine, 2.4);
        for time in [0.5f64, 1.0, 1.5, 2.0] {
            let expected = slope * (time - 0.25) as f32;
            assert!((r.left[index(time)] - expected).abs() < 2e-3, "t={time}");
        }
    }

    #[test]
    fn a_resampled_sine_keeps_its_pitch() {
        let t = TrackAudio {
            id: 1,
            audio: mono_audio(44_100, sine(440.0, 44_100, 3.0, 0.5)),
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
        };
        let (player, mut engine) = setup(vec![t]);
        player.play();
        let r = render_sec(&player, &mut engine, 2.0);
        assert!((freq_of(&r, 0.3, 1.8, 300.0, 600.0) - 440.0).abs() < 0.3);
    }

    fn dc_mix() -> (Player, crate::engine::Engine) {
        let (player, engine) = setup(vec![
            track(1, dc(5.0, 0.1)),
            track(2, dc(5.0, 0.2)),
            track(3, dc(5.0, 0.4)),
        ]);
        player.play();
        (player, engine)
    }

    fn settle(player: &Player, engine: &mut crate::engine::Engine) -> f32 {
        last(&render_sec(player, engine, 0.06))
    }

    #[test]
    fn solo_beats_mute_and_mute_applies_otherwise() {
        let (player, mut engine) = dc_mix();
        assert!((settle(&player, &mut engine) - 0.7).abs() < 1e-5);

        player.set_track_mute(2, true);
        assert!((settle(&player, &mut engine) - 0.5).abs() < 1e-5);

        player.set_track_solo(3, true);
        assert!((settle(&player, &mut engine) - 0.4).abs() < 1e-5);

        player.set_track_mute(3, true);
        assert!((settle(&player, &mut engine) - 0.4).abs() < 1e-5, "a muted soloed track is heard");

        player.set_track_solo(2, true);
        assert!((settle(&player, &mut engine) - 0.6).abs() < 1e-5);

        player.set_track_solo(2, false);
        player.set_track_solo(3, false);
        assert!((settle(&player, &mut engine) - 0.1).abs() < 1e-5, "only track 1 is left unmuted");

        player.set_track_mute(2, false);
        player.set_track_mute(3, false);
        assert!((settle(&player, &mut engine) - 0.7).abs() < 1e-5);
    }

    #[test]
    fn track_volume_scales_only_that_track() {
        let (player, mut engine) = dc_mix();
        player.set_track_volume(2, 0.5);
        assert!((settle(&player, &mut engine) - 0.6).abs() < 1e-5);
        player.set_track_volume(2, 0.0);
        assert!((settle(&player, &mut engine) - 0.5).abs() < 1e-5);
        player.set_track_volume(99, 0.0);
        assert!((settle(&player, &mut engine) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn initial_flags_from_set_tracks_apply_from_the_first_sample() {
        let mut muted = track(1, dc(2.0, 0.3));
        muted.muted = true;
        let mut soloed = track(2, dc(2.0, 0.2));
        soloed.solo = true;
        let (player, mut engine) = setup(vec![muted, soloed, track(3, dc(2.0, 0.1))]);
        player.play();
        let r = render_sec(&player, &mut engine, 0.2);
        assert!((last(&r) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn gain_changes_ramp_over_about_ten_milliseconds() {
        let (player, mut engine) = setup(vec![track(1, dc(5.0, 0.5))]);
        player.play();
        render_sec(&player, &mut engine, 0.1);
        player.set_track_mute(1, true);
        let r = render_sec(&player, &mut engine, 0.1);
        let begins = r.left.iter().position(|&s| s < 0.5 - 1e-6).unwrap();
        let ends = r.left.iter().position(|&s| s == 0.0).unwrap();
        let ramp_frames = ends - begins;
        assert!((470..=490).contains(&ramp_frames), "{ramp_frames} frames");
        assert!(max_step(&r.left) < 0.5 / 400.0, "{}", max_step(&r.left));

        player.set_track_mute(1, false);
        let r = render_sec(&player, &mut engine, 0.1);
        assert!(max_step(&r.left) < 0.5 / 400.0);
        assert_eq!(last(&r), 0.5);
    }

    #[test]
    fn mute_solo_and_volume_changes_on_a_sine_are_click_free() {
        let (player, mut engine) = setup(vec![
            track(1, sine(440.0, RATE, 10.0, 0.4)),
            track(2, sine(660.0, RATE, 10.0, 0.4)),
        ]);
        player.play();
        let mut all = render_sec(&player, &mut engine, 0.1).left;
        let natural = (0.4 * 440.0 + 0.4 * 660.0) * std::f32::consts::TAU / RATE as f32;
        let edits: [&dyn Fn(&Player); 6] = [
            &|p| p.set_track_mute(1, true),
            &|p| p.set_track_solo(2, true),
            &|p| p.set_track_volume(2, 0.3),
            &|p| p.set_track_solo(2, false),
            &|p| p.set_track_mute(1, false),
            &|p| p.set_track_volume(2, 1.0),
        ];
        for edit in edits {
            edit(&player);
            all.extend(render(&player, &mut engine, 41).left);
        }
        assert!(max_step(&all) < natural * 1.1, "{} vs {natural}", max_step(&all));
    }

    #[test]
    fn changing_an_offset_while_playing_dips_instead_of_clicking() {
        let (player, mut engine) = setup(vec![track(1, ramp(3.0, 0.1))]);
        player.play();
        let mut all = render_sec(&player, &mut engine, 0.2).left;
        player.set_track_offset(1, 0.3);
        let r = render_sec(&player, &mut engine, 0.3);
        assert!(max_step(&r.left) < 1e-3);
        let time = 0.45;
        assert!((r.left[index(time - 0.2)] - 0.1 * (time - 0.3) as f32).abs() < 3e-4);
        all.extend(r.left);
        assert!((player.duration_sec() - 3.3).abs() < 1e-6);
        assert!(max_step(&all) < 1e-3);
    }

    #[test]
    fn mixing_two_sines_is_their_sum() {
        let (player, mut engine) = setup(vec![
            track(1, sine(440.0, RATE, 3.0, 0.3)),
            track(2, sine(660.0, RATE, 3.0, 0.3)),
        ]);
        player.play();
        let r = render_sec(&player, &mut engine, 1.0);
        for i in SKIP_FADE_IN..r.left.len() {
            let t = i as f64 / f64::from(RATE);
            let expected = 0.3 * (std::f64::consts::TAU * 440.0 * t).sin() + 0.3 * (std::f64::consts::TAU * 660.0 * t).sin();
            assert!((f64::from(r.left[i]) - expected).abs() < 1e-4, "frame {i}");
        }
    }

    #[test]
    fn stereo_tracks_keep_their_channels() {
        let frames = 48_000;
        let interleaved: Vec<f32> = (0..frames).flat_map(|_| [0.2, -0.3]).collect();
        let audio = crate::DecodedAudio::from_interleaved(RATE, 2, interleaved).unwrap();
        let t = TrackAudio {
            id: 1,
            audio: std::sync::Arc::new(audio),
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
        };
        let (player, mut engine) = setup(vec![t]);
        player.play();
        let mut buffer = vec![0.0; 128];
        for _ in 0..40 {
            engine.render_block(&mut buffer);
        }
        assert!((buffer[100] - 0.2).abs() < 1e-6 && (buffer[101] + 0.3).abs() < 1e-6);
    }

    #[test]
    fn summing_loud_tracks_never_clips() {
        let tracks = (1..=4).map(|id| track(id, sine(100.0 + 37.0 * id as f64, RATE, 3.0, 0.9))).collect();
        let (player, mut engine) = setup(tracks);
        player.play();
        let r = render_sec(&player, &mut engine, 2.0);
        assert!(r.left.iter().all(|s| s.abs() <= 1.0));
        assert!(r.left.iter().any(|s| s.abs() > 0.8));

        let (player, mut engine) = setup(vec![track(1, dc(2.0, 0.9)), track(2, dc(2.0, 0.9)), track(3, dc(2.0, 0.9))]);
        player.play();
        let r = render_sec(&player, &mut engine, 0.5);
        let level = last(&r);
        assert!(level > 0.9 && level <= 1.0, "{level}");
    }

    #[test]
    fn peak_levels_report_audible_output_per_track() {
        let (player, mut engine) = dc_mix();
        render_sec(&player, &mut engine, 0.1);
        let peaks = player.track_peak_levels();
        assert_eq!(peaks.iter().map(|p| p.0).collect::<Vec<_>>(), [1, 2, 3]);
        for (peak, expected) in peaks.iter().zip([0.1, 0.2, 0.4]) {
            assert!((peak.1 - expected).abs() < 1e-4, "{peaks:?}");
        }
        player.set_track_mute(3, true);
        render_sec(&player, &mut engine, 2.0);
        let peaks = player.track_peak_levels();
        assert!(peaks[2].1 < 0.01, "{peaks:?}");
        assert!((peaks[1].1 - 0.2).abs() < 1e-4);
    }

    #[test]
    fn set_tracks_rejects_duplicate_ids_and_load_replaces_the_mix() {
        let (player, _engine) = setup(vec![track(1, dc(1.0, 0.1))]);
        let err = player.set_tracks(vec![track(5, dc(1.0, 0.1)), track(5, dc(1.0, 0.1))]);
        assert!(matches!(err, Err(AudioError::DuplicateTrack(5))));
        assert!((player.duration_sec() - 1.0).abs() < 1e-6);
        player.load(mono_audio(RATE, dc(2.5, 0.1))).unwrap();
        assert!((player.duration_sec() - 2.5).abs() < 1e-6);
        player.set_tracks(Vec::new()).unwrap();
        assert_eq!(player.duration_sec(), 0.0);
    }

    #[test]
    fn playing_without_audio_does_nothing() {
        let (player, mut engine) = setup(Vec::new());
        player.play();
        assert!(!player.is_playing());
        let r = render_sec(&player, &mut engine, 0.1);
        assert!(r.left.iter().all(|&s| s == 0.0));
    }
}

mod transport {
    use super::*;

    fn time_coded() -> (Player, crate::engine::Engine) {
        setup(vec![track(1, ramp(4.0, 0.1))])
    }

    fn value_at_time(t: f64) -> f32 {
        0.1 * t as f32
    }

    #[test]
    fn region_without_loop_plays_once_and_parks_at_its_end() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((1.0, 1.5)));
        player.play();
        let r = render_sec(&player, &mut engine, 1.0);
        assert!(!player.is_playing());
        assert!((player.position_sec() - 1.5).abs() < 0.002, "{}", player.position_sec());
        assert!((r.left[index(0.1)] - value_at_time(1.1)).abs() < 3e-4);
        assert!(r.left[index(0.6)..].iter().all(|&s| s == 0.0));
        assert!(max_step(&r.left) < 2e-3, "{}", max_step(&r.left));

        player.play();
        let r = render_sec(&player, &mut engine, 0.1);
        assert!(r.positions[0] < 1.1 && r.positions[0] >= 1.0 - 1e-9, "restarts from the region start");
    }

    #[test]
    fn region_loop_wraps_gaplessly_with_a_short_crossfade() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((1.0, 1.5)));
        player.set_loop(true);
        player.play();
        let r = render_sec(&player, &mut engine, 2.6);
        assert!(player.is_playing());
        assert!(r.left[SKIP_FADE_IN..].iter().all(|&s| (0.0999..=0.1501).contains(&s)));
        let wrap_step = max_step(&r.left[SKIP_FADE_IN..]);
        assert!(wrap_step < 0.05 / 150.0, "{wrap_step}");
        assert!(r.positions.iter().all(|&p| (0.999..=1.501).contains(&p)));

        let wraps: Vec<usize> = (1..r.positions.len())
            .filter(|&i| r.positions[i] < r.positions[i - 1])
            .collect();
        assert!(wraps.len() >= 4, "{wraps:?}");
        let period = f64::from(RATE) * 0.5 / 64.0;
        for pair in wraps.windows(2) {
            assert!(((pair[1] - pair[0]) as f64 - period).abs() < 2.5, "{wraps:?}");
        }
    }

    #[test]
    fn wrap_keeps_the_signal_phase_continuous_for_a_sine() {
        let (player, mut engine) = setup(vec![track(1, sine(440.0, RATE, 5.0, 0.5))]);
        player.set_region(Some((1.0, 1.2)));
        player.set_loop(true);
        player.play();
        let r = render_sec(&player, &mut engine, 2.0);
        let natural = 0.5 * std::f32::consts::TAU * 440.0 / RATE as f32;
        assert!(max_step(&r.left) < natural * 3.0, "{}", max_step(&r.left));
        assert!(rms(&r.left[SKIP_FADE_IN..]) > 0.25);
    }

    #[test]
    fn play_resumes_inside_the_region_and_restarts_outside_it() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((1.0, 2.0)));
        player.seek(1.4);
        player.play();
        let r = render_sec(&player, &mut engine, 0.05);
        assert!((r.left[index(0.04)] - value_at_time(1.44)).abs() < 3e-4);
        player.pause();
        render_sec(&player, &mut engine, 0.05);

        player.seek(3.0);
        player.play();
        let r = render_sec(&player, &mut engine, 0.05);
        assert!((r.left[index(0.04)] - value_at_time(1.04)).abs() < 3e-4);
    }

    #[test]
    fn clearing_the_region_plays_the_whole_file() {
        let (player, mut engine) = setup(vec![track(1, ramp(1.0, 0.1))]);
        player.set_region(Some((0.2, 0.4)));
        player.set_region(None);
        player.play();
        let r = render_sec(&player, &mut engine, 1.5);
        assert!((r.left[index(0.7)] - 0.07).abs() < 3e-4);
        assert!(!player.is_playing());
        assert!((player.position_sec() - 1.0).abs() < 0.002);
    }

    #[test]
    fn changing_the_region_while_playing_applies_at_once_without_clicks() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((0.0, 10.0)));
        player.set_loop(true);
        player.play();
        let mut all = render_sec(&player, &mut engine, 0.3).left;

        player.set_region(Some((0.2, 0.4)));
        player.set_loop(false);
        let r = render_sec(&player, &mut engine, 0.4);
        assert!(!player.is_playing());
        assert!((player.position_sec() - 0.4).abs() < 0.002, "{}", player.position_sec());
        assert!(r.left[index(0.2)..].iter().all(|&s| s == 0.0));
        all.extend(r.left);
        assert!(max_step(&all) < 2e-3, "{}", max_step(&all));
    }

    #[test]
    fn toggling_loop_while_playing_takes_effect_at_the_region_end() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((0.2, 0.4)));
        player.seek(0.3);
        player.play();
        render_sec(&player, &mut engine, 0.02);
        player.set_loop(true);
        let r = render_sec(&player, &mut engine, 0.5);
        assert!(player.is_playing());
        assert!(r.positions.iter().all(|&p| (0.199..=0.401).contains(&p)));

        player.set_loop(false);
        render_sec(&player, &mut engine, 0.5);
        assert!(!player.is_playing());
        assert!((player.position_sec() - 0.4).abs() < 0.002);
    }

    #[test]
    fn pause_holds_the_position_and_play_continues_from_it() {
        let (player, mut engine) = time_coded();
        player.play();
        render_sec(&player, &mut engine, 0.3);
        player.pause();
        assert!(!player.is_playing());
        let r = render_sec(&player, &mut engine, 0.2);
        let held = r.positions[r.positions.len() - 1];
        assert!((held - 0.3).abs() < 0.01, "{held}");
        assert!(r.positions[20..].iter().all(|&p| p == held));
        assert!(r.left[index(0.05)..].iter().all(|&s| s == 0.0));
        assert!(max_step(&r.left) < 2e-3);

        player.play();
        let r = render_sec(&player, &mut engine, 0.2);
        assert!((r.positions[0] - held).abs() < 0.003);
        assert!((r.positions[r.positions.len() - 1] - held - 0.2).abs() < 0.004);
        assert!(max_step(&r.left) < 2e-3);
    }

    #[test]
    fn stop_returns_to_the_region_start_or_the_beginning() {
        let (player, mut engine) = time_coded();
        player.play();
        render_sec(&player, &mut engine, 0.3);
        player.stop();
        render_sec(&player, &mut engine, 0.05);
        assert!(!player.is_playing());
        assert!(player.position_sec() < 1e-6);

        player.set_region(Some((1.0, 2.0)));
        player.seek(1.5);
        player.play();
        render_sec(&player, &mut engine, 0.1);
        player.stop();
        render_sec(&player, &mut engine, 0.05);
        assert!((player.position_sec() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn seek_while_stopped_moves_the_position_and_is_clamped() {
        let (player, mut engine) = time_coded();
        player.seek(2.5);
        render_sec(&player, &mut engine, 0.01);
        assert!((player.position_sec() - 2.5).abs() < 1e-6);
        player.seek(99.0);
        render_sec(&player, &mut engine, 0.01);
        assert!((player.position_sec() - 4.0).abs() < 1e-6);
        player.seek(-3.0);
        player.seek(f64::NAN);
        render_sec(&player, &mut engine, 0.01);
        assert_eq!(player.position_sec(), 0.0);
    }

    #[test]
    fn region_is_normalised_and_tiny_regions_still_play() {
        let (player, mut engine) = time_coded();
        player.set_region(Some((1.5, 1.0)));
        player.play();
        let r = render_sec(&player, &mut engine, 1.0);
        assert!((r.left[index(0.1)] - value_at_time(1.1)).abs() < 3e-4);
        assert!((player.position_sec() - 1.5).abs() < 0.002);

        player.set_region(Some((2.0, 2.0)));
        player.set_loop(true);
        player.play();
        let r = render_sec(&player, &mut engine, 0.5);
        assert!(player.is_playing());
        assert!(r.left.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn master_volume_scales_the_output() {
        let (player, mut engine) = setup(vec![track(1, dc(3.0, 0.4))]);
        player.set_volume(0.5);
        player.play();
        let r = render_sec(&player, &mut engine, 0.1);
        assert!((last(&r) - 0.2).abs() < 1e-6);
        player.set_volume(1.0);
        let r = render_sec(&player, &mut engine, 0.1);
        assert!((last(&r) - 0.4).abs() < 1e-6);
        assert!(max_step(&r.left) < 0.2 / 200.0);
    }
}

mod overlay {
    use super::*;

    fn silent_with_overlay(notes: Vec<(u8, f64, f64)>, mix: f32) -> (Player, crate::engine::Engine) {
        let (player, engine) = setup(vec![track(1, dc(6.0, 0.0))]);
        player.set_note_overlay(notes, mix);
        (player, engine)
    }

    #[test]
    fn overlay_plays_the_notated_pitch_and_follows_the_reference() {
        let (player, mut engine) = silent_with_overlay(vec![(69, 0.0, 2.0)], 1.0);
        player.play();
        let r = render_sec(&player, &mut engine, 0.6);
        let freq = freq_of(&r, 0.05, 0.55, 300.0, 700.0);
        assert!(cents(freq, 440.0).abs() < 5.0, "{freq}");

        let (player, mut engine) = silent_with_overlay(vec![(69, 0.0, 2.0)], 1.0);
        player.set_reference_pitch(PitchRef::new(432.0).unwrap());
        player.play();
        let r = render_sec(&player, &mut engine, 0.6);
        let freq = freq_of(&r, 0.05, 0.55, 300.0, 700.0);
        assert!(cents(freq, 432.0).abs() < 5.0, "{freq}");
    }

    #[test]
    fn overlay_ignores_track_mute_and_solo_and_has_its_own_level() {
        let (player, mut engine) = setup(vec![track(1, sine(220.0, RATE, 6.0, 0.4)), track(2, dc(6.0, 0.0))]);
        player.set_note_overlay(vec![(69, 0.0, 2.0)], 1.0);
        player.set_track_mute(1, true);
        player.set_track_solo(2, true);
        player.play();
        let r = render_sec(&player, &mut engine, 0.6);
        let freq = freq_of(&r, 0.05, 0.55, 300.0, 700.0);
        assert!(cents(freq, 440.0).abs() < 5.0, "{freq}");

        player.set_note_overlay(vec![(69, 0.0, 2.0)], 0.0);
        let r = render_sec(&player, &mut engine, 0.3);
        assert!(r.left[index(0.1)..].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn overlay_is_added_to_the_audio_without_altering_either() {
        let render_with = |audio_level: f32, mix: f32| {
            let (player, mut engine) = setup(vec![track(1, sine(220.0, RATE, 6.0, audio_level))]);
            player.set_note_overlay(vec![(57, 0.0, 2.0), (64, 0.2, 2.0)], mix);
            player.play();
            render_sec(&player, &mut engine, 0.5).left
        };
        let audio_only = render_with(0.05, 0.0);
        let overlay_only = render_with(0.0, 0.5);
        let both = render_with(0.05, 0.5);
        assert!(rms(&overlay_only[SKIP_FADE_IN..]) > 0.01);
        for i in 0..both.len() {
            assert!((both[i] - audio_only[i] - overlay_only[i]).abs() < 1e-5, "frame {i}");
        }
    }

    #[test]
    fn overlay_notes_start_on_project_time_and_keep_their_pitch_when_slowed() {
        for mode in [SpeedMode::PreservePitch, SpeedMode::ChangePitch] {
            let (player, mut engine) = silent_with_overlay(vec![(69, 0.5, 3.0)], 1.0);
            player.set_speed_mode(mode);
            player.set_playback_speed(0.5);
            player.play();
            let r = render_sec(&player, &mut engine, 2.0);
            let onset = r.left.iter().position(|s| s.abs() > 1e-4).unwrap();
            assert!((onset as f64 / f64::from(RATE) - 1.0).abs() < 0.015, "{mode:?} {onset}");
            let freq = freq_of(&r, 1.1, 1.8, 300.0, 700.0);
            assert!(cents(freq, 440.0).abs() < 5.0, "{mode:?} {freq}");
        }
    }

    #[test]
    fn overlay_follows_seeks_and_loops() {
        let (player, mut engine) = silent_with_overlay(vec![(69, 0.2, 0.4), (72, 1.0, 1.2)], 1.0);
        player.seek(0.9);
        player.play();
        let r = render_sec(&player, &mut engine, 0.4);
        let onset = r.left.iter().position(|s| s.abs() > 1e-4).unwrap();
        assert!((onset as f64 / f64::from(RATE) - 0.1).abs() < 0.015, "{onset}");

        let (player, mut engine) = silent_with_overlay(vec![(69, 0.1, 0.2)], 1.0);
        player.set_region(Some((0.0, 0.5)));
        player.set_loop(true);
        player.play();
        let r = render_sec(&player, &mut engine, 1.7);
        let mut onsets = 0;
        let mut quiet = true;
        for window in r.left.chunks(240) {
            let loud = window.iter().any(|s| s.abs() > 1e-3);
            if loud && quiet {
                onsets += 1;
            }
            quiet = !loud;
        }
        assert!(onsets >= 3, "{onsets}");
    }
}
