use super::*;
use fretscribe_ui::LaneSource;
use std::path::Path;

pub fn write_wav(path: &Path, rate: u32, samples: &[f32]) {
    std::fs::write(path, fretscribe_audio::wav::encode_mono_f32(rate, samples)).unwrap();
}

/// Decaying noise-free clicks (short sine bursts) every `period` seconds.
pub fn clicks(rate: u32, seconds: f64, period: f64) -> Vec<f32> {
    (0..(seconds * f64::from(rate)) as usize)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            let since = t % period;
            ((-since * 60.0).exp() * (std::f64::consts::TAU * 1000.0 * t).sin() * 0.8) as f32
        })
        .collect()
}

fn run(job: &mut LoadJob, store: &mut AudioStore) -> Vec<LoadEvent> {
    let mut all = Vec::new();
    let start = Instant::now();
    while !job.is_finished() && start.elapsed() < Duration::from_secs(60) {
        all.extend(job.poll(store));
        thread::sleep(Duration::from_millis(5));
    }
    all
}

#[test]
fn loads_two_tracks_and_reports_failures() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    write_wav(&a, 22_050, &clicks(22_050, 3.0, 0.5));
    write_wav(&b, 16_000, &clicks(16_000, 2.0, 0.5));
    let mut store = AudioStore::default();
    let mut job = LoadJob::spawn(
        vec![(1, a), (2, b), (3, dir.path().join("missing.wav"))],
        48_000,
        40.0,
    );
    let events = run(&mut job, &mut store);
    assert!(job.is_finished() && events.last() == Some(&LoadEvent::Finished));
    assert!(events.iter().any(|e| matches!(e, LoadEvent::TrackFailed { id: 3, .. })));
    assert!(events.iter().any(|e| matches!(e, LoadEvent::TrackReady { id: 2, .. })));
    assert!(events.iter().any(|e| matches!(e, LoadEvent::SpectrogramDone { id: 1 })));
    assert_eq!(store.tracks.len(), 2);
    assert!(store.revision > 0 && store.peaks(1, (0.0, 3.0), 77).len() == 77);
    assert!((store.duration_sec(1) - 3.0).abs() < 0.01);
    assert!(store.tracks[&1].spectrogram.data.iter().any(|&v| v > 0.0));
    assert!(store.onsets(1).len() >= 4, "{:?}", store.onsets(1));
    assert_eq!(store.tracks[&1].prepared.sample_rate(), 48_000);
    let fractions: Vec<f32> = events.iter().filter_map(|e| if let LoadEvent::Progress { fraction, .. } = e { Some(*fraction) } else { None }).collect();
    assert!(fractions.windows(2).all(|w| w[1] >= w[0]) && fractions.last() == Some(&1.0), "{fractions:?}");
    assert!(job.poll(&mut store).is_empty());
}

#[test]
fn requantise_rewrites_the_stored_spectrogram() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    write_wav(&a, 22_050, &clicks(22_050, 2.0, 0.5));
    let mut store = AudioStore::default();
    run(&mut LoadJob::spawn(vec![(1, a)], 48_000, 40.0), &mut store);
    let (before, revision) = (store.tracks[&1].spectrogram.data.clone(), store.revision);
    let t = &store.tracks[&1];
    let mut job = LoadJob::requantise(vec![(1, t.mono.clone(), t.sample_rate)], 8.0);
    run(&mut job, &mut store);
    assert_ne!(store.tracks[&1].spectrogram.data, before);
    assert!(store.revision > revision);
}

#[test]
fn empty_job_finishes() {
    let mut job = LoadJob::spawn(vec![], 48_000, 40.0);
    let events = job.poll(&mut AudioStore::default());
    assert_eq!(events.last(), Some(&LoadEvent::Finished));
}
