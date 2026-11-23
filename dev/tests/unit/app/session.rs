use super::*;
use crate::loader::tests::{clicks, write_wav};

#[test]
fn q_change_recomputes_the_stored_spectrogram() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("a.wav");
    write_wav(&wav, 22_050, &clicks(22_050, 2.0, 0.5));
    let mut store = AudioStore::default();
    let mut job = LoadJob::spawn(vec![(1, wav)], 48_000, 40.0);
    while !job.is_finished() {
        job.poll(&mut store);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let file = ProjectFile::new(fretscribe_core::testkit::project(2.0));
    let mut s = Session::new(file, None, store, &Settings::default(), LutSet::scan(None));
    let (before, revision) = (s.store.tracks[&1].spectrogram.data.clone(), s.store.revision);
    for q in [8.0, 12.0] {
        s.ui.spectro_q = q;
        s.q_changed();
    }
    while s.poll_loads() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_ne!(s.store.tracks[&1].spectrogram.data, before);
    assert!(s.store.revision > revision);
}
