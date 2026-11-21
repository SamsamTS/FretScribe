use std::time::{Duration, Instant};

use super::*;
use crate::testutil::*;

#[test]
fn installing_prepared_tracks_is_fast_for_long_audio() {
    let (player, _engine) = Player::headless(RATE);
    let audio = mono_audio(RATE, dc(380.0, 0.1));
    let tracks = vec![track_of(1, Arc::clone(&audio), 0.0), track_of(2, Arc::clone(&audio), 0.0)];
    let best = (0..3)
        .map(|_| {
            let started = Instant::now();
            player.set_prepared_tracks(tracks.clone()).unwrap();
            started.elapsed()
        })
        .min()
        .unwrap();
    assert!(best < Duration::from_millis(50), "{best:?}");
    assert!((player.duration_sec() - 380.0).abs() < 1e-6);
}

#[test]
fn focus_context_and_reference_pitch_feed_the_band() {
    let (player, _engine) = Player::headless(RATE);
    assert_eq!(player.state().focus.band(), FocusBand::default());
    let drop_d = Tuning {
        name: "Drop D".to_owned(),
        open_strings: vec![38, 45, 50, 55, 59, 64],
    };
    let tuned = PitchRef::new(432.0).unwrap();
    player.set_focus_context(&drop_d, 2);
    player.set_reference_pitch(tuned);
    assert_eq!(player.state().focus.band(), focus_band(&drop_d, 2, &tuned));
}

#[test]
fn prepared_tracks_are_validated() {
    let (player, _engine) = Player::headless(RATE);
    let audio = mono_audio(RATE, dc(1.0, 0.1));
    let duplicate = player.set_prepared_tracks(vec![track_of(3, Arc::clone(&audio), 0.0), track_of(3, Arc::clone(&audio), 0.0)]);
    assert!(matches!(duplicate, Err(AudioError::DuplicateTrack(3))));

    let other_rate = mono_audio(44_100, dc(1.0, 0.1));
    let wrong = PreparedTrack {
        audio: PreparedAudio::new(&other_rate, 44_100).unwrap(),
        ..track_of(1, Arc::clone(&audio), 0.0)
    };
    assert!(matches!(
        player.set_prepared_tracks(vec![wrong]),
        Err(AudioError::RateMismatch { expected: RATE, found: 44_100 })
    ));
    assert_eq!(player.duration_sec(), 0.0);
}
