use super::*;
use crate::loader::tests::{clicks, write_wav};
use std::time::{Duration, Instant};

fn wait<T: Send + 'static>(job: &mut Job<T>) -> Option<T> {
    let start = Instant::now();
    while job.is_running() && start.elapsed() < Duration::from_secs(60) {
        if let Some(v) = job.poll() {
            return Some(v);
        }
        thread::sleep(Duration::from_millis(5));
    }
    None
}

#[test]
fn needs_sync_uses_the_20ms_rule() {
    assert!(!needs_sync(10.0, 10.015));
    assert!(needs_sync(10.0, 10.05));
}

#[test]
fn thumbnail_is_bounded_and_normalised() {
    let mono: Vec<f32> = (0..1000).map(|i| if i == 500 { -0.5 } else { 0.1 }).collect();
    let t = mono_peaks_thumbnail(&mono, 500);
    assert_eq!(t.len(), THUMBNAIL_MAX);
    assert_eq!(t.iter().max(), Some(&255));
    assert_eq!(mono_peaks_thumbnail(&mono, 10).len(), 10);
    assert!(mono_peaks_thumbnail(&[0.0; 100], 8).iter().all(|&b| b == 0));
    assert!(mono_peaks_thumbnail(&[], 8).is_empty());
}

#[test]
fn job_delivers_once() {
    let mut job = Job::spawn(|| 7);
    assert_eq!(wait(&mut job), Some(7));
    assert!(!job.is_running() && job.poll().is_none());
    let mut panicking: Job<u8> = Job::spawn(|| panic!("boom"));
    assert_eq!(wait(&mut panicking), None);
    assert!(!panicking.is_running());
}

#[test]
fn resampling_keeps_duration() {
    let x = vec![0.5; 4000];
    assert_eq!(resample_linear(&x, 8000, 4000).len(), 2000);
    assert_eq!(resample_linear(&x, 8000, 8000).len(), 4000);
}

#[test]
fn sync_finds_a_shift_across_sample_rates() {
    let a = clicks(22_050, 12.0, 0.5);
    let b: Vec<f32> = [vec![0.0; 11_025 / 2], clicks(11_025, 12.0, 0.5)].concat();
    let mut job = sync((a.into(), 22_050), (b.into(), 11_025), SyncOptions::default());
    let c = wait(&mut job).unwrap();
    assert!(!c.is_empty());
}

#[test]
fn auto_jobs_run_on_a_click_track() {
    let env = onset_envelope(&clicks(22_050, 12.0, 0.5), 22_050);
    let inputs = vec![EnvelopeInput { env, offset_sec: 0.0, weight: 1.0 }];
    let measures = fretscribe_analysis::beatfit::synth::rough_grid(110.0, 12.0);
    let mut fit = auto_fit(inputs.clone(), measures.clone(), 0);
    assert_eq!(wait(&mut fit).unwrap().len(), measures.len());
    let mut tempo = auto_tempo(inputs);
    assert!(!wait(&mut tempo).unwrap().is_empty());
}

#[test]
fn form_estimates_find_the_bpm_and_finish() {
    let dir = tempfile::tempdir().unwrap();
    let (guitar, drums) = (dir.path().join("g.wav"), dir.path().join("d.wav"));
    write_wav(&guitar, 22_050, &clicks(22_050, 16.0, 0.5));
    write_wav(&drums, 22_050, &clicks(22_050, 16.0, 0.5));
    let files = vec![(guitar.clone(), TrackRole::Guitar), (drums, TrackRole::Drums), (dir.path().join("x.wav"), TrackRole::Bass)];
    let mut est = FormEstimates::spawn(vec![(guitar.clone(), TrackRole::Other)]);
    est.restart(files);
    let (mut all, start) = (Vec::new(), Instant::now());
    while !all.contains(&EstimateUpdate::Done) && start.elapsed() < Duration::from_secs(60) {
        all.extend(est.poll());
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(all.last(), Some(&EstimateUpdate::Done));
    let bpms = all.iter().find_map(|u| if let EstimateUpdate::Bpm(b) = u { Some(b) } else { None }).unwrap();
    assert!(bpms.iter().any(|b| (b.0 - 120.0).abs() < 3.6), "{bpms:?}");
    assert!(all.iter().any(|u| matches!(u, EstimateUpdate::Reference(_))));
    assert!(all.iter().any(|u| matches!(u, EstimateUpdate::Duration { path, .. } if *path == guitar)));
    assert!(all.iter().any(|u| matches!(u, EstimateUpdate::Failed(_))));
}
