use super::*;

fn recent(path: &str) -> RecentProject {
    RecentProject { path: path.into(), name: path.into(), ..Default::default() }
}

#[test]
fn recent_is_deduped_capped_and_ordered() {
    let mut s = Settings::default();
    for i in 0..15 {
        s.push_recent(recent(&format!("/p{i}")));
    }
    assert_eq!(s.recent.len(), MAX_RECENT);
    s.push_recent(recent("/p10"));
    assert_eq!(s.recent[0].path, Path::new("/p10"));
    assert_eq!(s.recent.iter().filter(|r| r.path == Path::new("/p10")).count(), 1);
    s.remove_recent(Path::new("/p10"));
    assert_eq!(s.recent.len(), MAX_RECENT - 1);
    s.push_recent(RecentProject { thumbnail: Some(vec![1; 200]), ..recent("/t") });
    assert_eq!(s.recent[0].thumbnail.as_ref().map(Vec::len), Some(MAX_THUMBNAIL_BYTES));
}

#[test]
fn save_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/settings.json");
    let mut s = Settings { accent: Some([1, 2, 3]), autosave_secs: 99, ..Default::default() };
    s.push_recent(recent("/a"));
    s.save(&path).unwrap();
    assert_eq!(Settings::load(&path), s);
}

#[test]
fn missing_corrupt_and_partial_files_give_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.json");
    assert_eq!(Settings::load(&path), Settings::default());
    std::fs::write(&path, b"{ not json").unwrap();
    assert_eq!(Settings::load(&path), Settings::default());
    std::fs::write(&path, br#"{"keep_pitch": false}"#).unwrap();
    let s = Settings::load(&path);
    assert!(!s.keep_pitch && s.autosave_enabled && s.autosave_secs == 30);
}

#[test]
fn display_defaults_are_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.json");
    std::fs::write(&path, br#"{"spectro_black": 0.9, "spectro_white": 0.1, "spectro_gamma": 99.0}"#).unwrap();
    let s = Settings::load(&path);
    assert!(s.spectro_white - s.spectro_black >= fretscribe_core::persist::LEVELS_MIN_GAP - 1e-6);
    assert_eq!(s.spectro_gamma, *fretscribe_core::persist::GAMMA_RANGE.end());
}
