use super::*;

#[test]
fn untagged_or_unreadable_files_give_empty_tags() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("a.wav");
    std::fs::write(&wav, crate::wav::encode(44_100, 1, &[0; 100])).unwrap();
    assert_eq!(read_tags(&wav), Tags::default());
    assert_eq!(read_tags(&dir.path().join("missing.mp3")), Tags::default());
    let junk = dir.path().join("junk.mp3");
    std::fs::write(&junk, b"not audio").unwrap();
    assert_eq!(read_tags(&junk), Tags::default());
}
