use super::*;
use fretscribe_audio::DecodedAudio;


pub fn data(onsets: Vec<f64>, seconds: f64) -> TrackData {
    let rate = 8000;
    let mono: Arc<[f32]> = vec![0.5; (seconds * f64::from(rate)) as usize].into();
    let decoded = DecodedAudio::from_interleaved(rate, 1, mono.to_vec()).unwrap();
    TrackData {
        prepared: PreparedAudio::new(&decoded, 48_000).unwrap(),
        pyramid: Arc::new(WaveformPyramid::build(&mono, rate)),
        mono,
        sample_rate: rate,
        duration_sec: seconds,
        spectrogram: Spectrogram::empty(&fretscribe_analysis::SpectrogramInfo {
            n_frames: 2,
            n_rows: 2,
            hop_sec: 0.01,
            min_midi: 40.0,
            max_midi: 41.0,
            bins_per_semitone: 1,
        }),
        envelope: OnsetEnvelope { values: vec![0.0, 1.0], hop_sec: 0.01 },
        onsets,
    }
}

fn project() -> Project {
    fretscribe_core::testkit::band(4.0)
}

#[test]
fn prepared_and_inputs_follow_project_order_and_skip_missing() {
    let mut p = project();
    let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
    p.tracks[1].offset_sec = 0.25;
    p.tracks[1].muted = true;
    let mut s = AudioStore::default();
    s.tracks.insert(ids[1], data(vec![], 1.0));
    s.tracks.insert(ids[0], data(vec![], 1.0));
    let prepared = s.prepared_tracks(&p);
    assert_eq!(prepared.iter().map(|t| t.id).collect::<Vec<_>>(), ids[..2]);
    assert!(prepared[1].muted && prepared[1].offset_sec == 0.25);
    // Guitar is excluded from beats by default; the drums are in.
    assert_eq!(s.onset_inputs(&p).len(), 1);
    assert_eq!(s.shared_audio_end(&p), 1.0);
    s.remove(ids[1]);
    assert_eq!(s.prepared_tracks(&p).len(), 1);
}

#[test]
fn beat_times_and_snapping() {
    let p = project();
    let mut s = AudioStore::default();
    s.set_beat_times_from(&p);
    assert!(s.beat_times.len() >= 4 && s.beat_times.windows(2).all(|w| w[1] > w[0]));
    s.tracks.insert(1, data(vec![0.1], 4.0));
    s.tracks.insert(2, data(vec![0.2], 4.0));
    s.reference = (1, 0.0);
    let c = s.snap_candidates(2, 0.28, 0.1);
    assert!(c.iter().any(|o| (o - 0.3).abs() < 1e-9), "{c:?}");
    assert_eq!(s.peaks(2, (0.0, 1.0), 10).len(), 10);
    assert_eq!(s.peaks(99, (0.0, 1.0), 10).len(), 10);
    assert_eq!(s.duration_sec(2), 4.0);
}
