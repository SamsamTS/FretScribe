use fretscribe_core::{Measure, NotePolicy, Project, Track, TrackRole};

use super::synth::*;
use super::*;

const BPM: f64 = 120.0;
const BEATS: usize = 64;
const LEN_SEC: f64 = 33.0;

fn click_env(beats: &[f64]) -> OnsetEnvelope {
    let hits: Vec<(f64, f64)> = beats.iter().map(|&t| (t, 1000.0)).collect();
    onset_envelope(&render_hits(&hits, LEN_SEC), SAMPLE_RATE)
}

fn fitted(env: &OnsetEnvelope, grid: &[Measure]) -> Vec<Measure> {
    let fits = fit_measures(env, grid, 0.25);
    let mut out = grid.to_vec();
    for (m, f) in out.iter_mut().zip(fitted_grid(grid, &fits)) {
        (m.start_sec, m.tempo_bpm) = (f.start_sec, f.tempo_bpm);
    }
    out
}

#[test]
fn onsets_land_on_the_clicks() {
    let beats = drifting_beats(16, BPM, 0.0, 0.0, 1);
    let onsets = detect_onsets(&click_env(&beats));
    assert_eq!(onsets.len(), beats.len());
    for (o, b) in onsets.iter().zip(&beats) {
        assert!((o - b).abs() < 0.02, "{o} vs {b}");
    }
}

#[test]
fn degenerate_inputs_are_inert() {
    let empty = onset_envelope(&[], SAMPLE_RATE);
    assert!(empty.values.is_empty() && detect_onsets(&empty).is_empty());
    assert!(estimate_tempo(&empty, (60.0, 200.0)).is_empty());
    assert!(combine_envelopes(&[]).values.is_empty());
    let grid = rough_grid(BPM, 8.0);
    let fits = fit_measures(&empty, &grid, 0.25);
    assert!(fits.iter().zip(&grid).all(|(f, m)| f.new_start_sec == m.start_sec && f.confidence == 0.0));
    let silent = onset_envelope(&vec![0.0; 44_100 * 8], SAMPLE_RATE);
    assert!(fit_measures(&silent, &grid, 0.25).iter().all(|f| f.confidence == 0.0));
}

#[test]
fn fit_reduces_the_beat_error_on_drifting_clicks() {
    for seed in 1..=4 {
        let beats = drifting_beats(BEATS, BPM, 0.05, 0.02, seed);
        let grid = rough_grid(BPM, LEN_SEC);
        let before = mean_beat_error(&grid, &beats);
        let after = mean_beat_error(&fitted(&click_env(&beats), &grid), &beats);
        assert!(after < before * 0.6, "seed {seed}: {before:.4} -> {after:.4}");
    }
}

#[test]
fn fit_never_squashes_or_reorders_measures() {
    let beats = drifting_beats(BEATS, BPM, 0.05, 0.02, 7);
    let grid = fretscribe_core::generate_measures(0.0, LEN_SEC, BPM, fretscribe_core::TimeSignature::new(1, 4), &mut 1);
    let fits = fit_measures(&click_env(&beats), &grid, 0.5);
    for (i, w) in fits.windows(2).enumerate() {
        let (old, new) = (grid[i + 1].start_sec - grid[i].start_sec, w[1].new_start_sec - w[0].new_start_sec);
        assert!(new >= 0.5 * old - 1e-9, "measure {i}: {old} -> {new}");
    }
}

#[test]
fn combine_shifts_normalises_and_weights() {
    let unit = |values: Vec<f32>| OnsetEnvelope { values, hop_sec: 0.1 };
    let combined = combine_envelopes(&[
        EnvelopeInput { env: unit(vec![0.0, 4.0, 0.0, 0.0]), offset_sec: 0.0, weight: 1.0 },
        EnvelopeInput { env: unit(vec![0.0, 2.0, 0.0, 0.0]), offset_sec: 0.2, weight: 0.5 },
    ]);
    assert_eq!(combined.values.len(), 6);
    let expected = [0.0, 1.0, 0.0, 0.5, 0.0, 0.0];
    assert!(combined.values.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-6), "{:?}", combined.values);

    let track = Track::new(1, "d", TrackRole::Drums);
    assert!(envelope_input(&track, unit(vec![1.0])).is_some_and(|i| i.weight == 1.0));
    let mut off = Track::new(2, "g", TrackRole::Guitar);
    assert!(envelope_input(&off, unit(vec![1.0])).is_none());
    off.use_for_beats = true;
    assert_eq!(envelope_input(&off, unit(vec![1.0])).map(|i| i.weight), Some(0.5));
}

#[test]
fn tempo_estimate_finds_the_click_rate() {
    let beats = drifting_beats(BEATS, BPM, 0.0, 0.005, 3);
    let top = estimate_tempo(&click_env(&beats), (60.0, 240.0));
    assert!(top.len() <= 3 && (top[0].0 / BPM - 1.0).abs() < 0.02, "{top:?}");
    assert_eq!(octave_ratio(120.0, 60.0), Some(0.5));
    assert_eq!(octave_ratio(120.0, 240.0), Some(2.0));
    assert_eq!(octave_ratio(120.0, 90.0), None);
}

#[test]
fn helper_drum_track_beats_a_noisy_guitar_alone() {
    let (mut gain, mut tempo_gain) = (0, 0);
    for seed in 1..=6 {
        let beats = drifting_beats(BEATS, BPM, 0.05, 0.02, seed);
        let drums = onset_envelope(&render_hits(&drum_hits(&beats), LEN_SEC), SAMPLE_RATE);
        let guitar = noisy_guitar_envelope(&beats, drums.hop_sec, LEN_SEC, seed);
        let both = combine_envelopes(&[
            EnvelopeInput { env: drums, offset_sec: 0.0, weight: 1.0 },
            EnvelopeInput { env: guitar.clone(), offset_sec: 0.0, weight: 0.5 },
        ]);
        let grid = rough_grid(BPM, LEN_SEC);
        let alone = mean_beat_error(&fitted(&guitar, &grid), &beats);
        let helped = mean_beat_error(&fitted(&both, &grid), &beats);
        gain += usize::from(helped < alone * 0.8);

        let miss = |env: &OnsetEnvelope| {
            let top = estimate_tempo(env, (60.0, 240.0)).first().map_or(f64::MAX, |c| c.0);
            [0.5, 1.0, 2.0].map(|m| (top / (BPM * m) - 1.0).abs()).into_iter().fold(f64::MAX, f64::min)
        };
        tempo_gain += usize::from(miss(&both) < 0.03 && miss(&guitar) > miss(&both));
    }
    assert!(gain >= 5, "grid fit improved on {gain}/6 seeds");
    assert!(tempo_gain >= 5, "tempo (up to octave) improved on {tempo_gain}/6 seeds");
}

#[test]
fn fitted_grid_is_accepted_by_auto_fit() {
    let beats = drifting_beats(BEATS, BPM, 0.05, 0.02, 2);
    let grid = rough_grid(BPM, LEN_SEC);
    let fits = fit_measures(&click_env(&beats), &grid, 0.25);
    let mut project = Project::new(Track::new(1, "g", TrackRole::Guitar), LEN_SEC, BPM, grid[0].time_sig);
    project.auto_fit(&fitted_grid(&grid, &fits), NotePolicy::KeepAudioPosition).unwrap();
    project.validate().unwrap();
}
