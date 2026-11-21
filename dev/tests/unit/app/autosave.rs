use super::*;
use fretscribe_core::{load_autosave, Project, TimeSignature, Track, TrackRole};
use std::time::Duration;

fn file() -> ProjectFile {
    ProjectFile::new(Project::new(Track::new(1, "t", TrackRole::Guitar), 10.0, 120.0, TimeSignature::COMMON))
}

#[test]
fn recovery_decision_table() {
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let new = old + Duration::from_secs(50);
    let d = decide_recovery;
    assert_eq!(d(Some(new), Some(old), true, false), Recovery::OfferRestore);
    assert_eq!(d(Some(old), Some(new), true, false), Recovery::None);
    assert_eq!(d(Some(new), None, false, false), Recovery::OfferRestore);
    assert_eq!(d(Some(old), Some(new), false, false), Recovery::OfferRestore);
    assert_eq!(d(Some(new), Some(old), true, true), Recovery::None);
    assert_eq!(d(None, Some(old), true, false), Recovery::None);
    assert_eq!(d(None, None, false, false), Recovery::None);
    assert_eq!(d(Some(new), None, true, false), Recovery::None);
}

#[test]
fn policy_thresholds() {
    let p = AutosavePolicy { enabled: true, interval_secs: 30, every_edits: 20 };
    assert!(!p.should_autosave(false, 999.0, 99));
    assert!(!p.should_autosave(true, 999.0, 0));
    assert!(!p.should_autosave(true, 29.9, 19));
    assert!(p.should_autosave(true, 30.0, 1));
    assert!(p.should_autosave(true, 1.0, 20));
    assert!(!AutosavePolicy { enabled: false, ..p }.should_autosave(true, 999.0, 99));
}

#[test]
fn worker_writes_generations_without_leftovers() {
    let dir = tempfile::tempdir().unwrap();
    let f = file();
    let path = autosave_path(dir.path(), f.project_id);
    assert!(path.ends_with(format!("autosave/{}.autosave", f.project_id)));
    for _ in 0..3 {
        let worker = AutosaveWorker::spawn();
        worker.submit(f.clone(), path.clone());
    }
    assert_eq!(load_autosave(&path).unwrap().map(|l| l.project_id), Some(f.project_id));
    let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.iter().all(|n| !n.ends_with(".tmp")));
    discard(&path).unwrap();
    assert!(!path.exists());
}

#[test]
fn worker_reports_unwritable_path() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, b"x").unwrap();
    let worker = AutosaveWorker::spawn();
    worker.submit(file(), blocker.join("sub/a.autosave"));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let error = loop {
        if let Some(e) = worker.try_recv_error() {
            break e;
        }
        assert!(std::time::Instant::now() < deadline, "no error reported");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(error.contains("autosave"));
}

#[test]
fn session_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(was_clean_exit(dir));
    mark_running(dir).unwrap();
    assert!(!was_clean_exit(dir));
    mark_clean_exit(dir).unwrap();
    assert!(was_clean_exit(dir));
}
