use super::*;


fn rel(base: &str, target: &str) -> Option<String> {
    relative_to(Path::new(base), Path::new(target)).map(|p| p.to_string_lossy().replace('\\', "/"))
}

#[test]
fn relative_paths() {
    assert_eq!(rel("/a/b", "/a/b/c/d.wav").as_deref(), Some("c/d.wav"));
    assert_eq!(rel("/a/b/x", "/a/c/d.wav").as_deref(), Some("../../c/d.wav"));
    assert_eq!(rel("/a/b", "/a/b").as_deref(), Some(""));
    assert_eq!(rel("/a/./b", "/a/b/d.wav").as_deref(), Some("d.wav"));
    assert_eq!(rel("/a/../b", "/b/d.wav"), None);
    assert_eq!(rel("a", "/a/d.wav"), None);
}

#[cfg(windows)]
#[test]
fn different_drives_have_no_relative_path() {
    assert_eq!(rel("C:/a", "D:/a/d.wav"), None);
}

fn project(audio: Option<PathBuf>) -> Project {
    let mut p = fretscribe_core::testkit::project(10.0);
    p.tracks[0].audio_path = audio;
    p
}

#[test]
fn fill_and_resolve_prefer_relative() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("audio/a.wav");
    std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
    std::fs::write(&audio, b"x").unwrap();
    let file = dir.path().join("p.fretscribe");
    let mut p = project(Some(audio.clone()));
    fill_relative_paths(&mut p, &file);
    assert_eq!(p.tracks[0].audio_rel_path.as_deref(), Some(Path::new("audio/a.wav")));
    assert_eq!(resolve_audio(Some(&file), &p.tracks[0]), Some(audio.clone()));

    let moved = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(moved.path().join("audio")).unwrap();
    std::fs::rename(&audio, moved.path().join("audio/a.wav")).unwrap();
    let moved_file = moved.path().join("p.fretscribe");
    assert_eq!(
        resolve_audio(Some(&moved_file), &p.tracks[0]),
        Some(moved.path().join("audio/a.wav"))
    );
    assert_eq!(resolve_audio(None, &p.tracks[0]), None);
}

#[test]
fn falls_back_to_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("a.wav");
    std::fs::write(&audio, b"x").unwrap();
    let mut p = project(Some(audio.clone()));
    p.tracks[0].audio_rel_path = Some("gone.wav".into());
    assert_eq!(resolve_audio(Some(&dir.path().join("p.json")), &p.tracks[0]), Some(audio));
}
