use super::*;
use std::io::Write;
use crate::testutil::sine_frames;
use tempfile::tempdir;

fn sine_i16(freq: f64, rate: u32, frames: usize, amp: f32) -> Vec<i16> {
    sine_frames(freq, rate, frames, amp).iter().map(|v| (v * 32767.0).round() as i16).collect()
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    File::create(&path).unwrap().write_all(bytes).unwrap();
    path
}

#[test]
fn decodes_mono_wav() {
    let dir = tempdir().unwrap();
    let pcm = sine_i16(440.0, 44_100, 22_050, 0.5);
    let path = write(dir.path(), "mono.wav", &crate::wav::encode(44_100, 1, &pcm));
    let audio = decode_file(&path).unwrap();
    assert_eq!(audio.sample_rate, 44_100);
    assert_eq!(audio.channels, 1);
    assert_eq!(audio.frames(), 22_050);
    assert!((audio.duration_sec - 0.5).abs() < 1e-9);
    assert_eq!(audio.interleaved, audio.mono);
    for (decoded, &raw) in audio.mono.iter().zip(&pcm) {
        assert!((decoded - f32::from(raw) / 32768.0).abs() < 1e-3);
    }
}

#[test]
fn decodes_stereo_wav_and_mixes_down() {
    let dir = tempdir().unwrap();
    let frames = 4_800;
    let mut pcm = Vec::new();
    for i in 0..frames {
        pcm.push(16_384);
        pcm.push(if i % 2 == 0 { -16_384 } else { 0 });
    }
    let path = write(dir.path(), "stereo.wav", &crate::wav::encode(48_000, 2, &pcm));
    let audio = decode_file(&path).unwrap();
    assert_eq!((audio.sample_rate, audio.channels), (48_000, 2));
    assert_eq!(audio.frames(), frames);
    assert_eq!(audio.interleaved.len(), frames * 2);
    assert!((audio.duration_sec - 0.1).abs() < 1e-9);
    assert!((audio.interleaved[0] - 0.5).abs() < 1e-3);
    assert!((audio.interleaved[1] + 0.5).abs() < 1e-3);
    assert!(audio.mono[0].abs() < 1e-3);
    assert!((audio.mono[1] - 0.25).abs() < 1e-3);
}

#[test]
fn reports_missing_and_garbage_files() {
    let dir = tempdir().unwrap();
    assert!(matches!(
        decode_file(dir.path().join("nope.wav")),
        Err(AudioError::Io(_))
    ));
    let junk = write(dir.path(), "junk.mp3", &[7u8; 4096]);
    assert!(matches!(decode_file(&junk), Err(AudioError::Decode(_))));
    let empty = write(dir.path(), "empty.wav", &crate::wav::encode(44_100, 1, &[]));
    assert!(decode_file(&empty).is_err());
}

#[test]
fn from_interleaved_validates_layout() {
    assert!(DecodedAudio::from_interleaved(0, 1, vec![0.0]).is_err());
    assert!(DecodedAudio::from_interleaved(44_100, 0, vec![0.0]).is_err());
    assert!(matches!(
        DecodedAudio::from_interleaved(44_100, 2, vec![0.0]),
        Err(AudioError::Empty)
    ));
    let audio = DecodedAudio::from_interleaved(10, 2, vec![1.0, 3.0, 2.0, 2.0, 9.0]).unwrap();
    assert_eq!(audio.mono, [2.0, 2.0]);
    assert_eq!(audio.interleaved.len(), 4);
}
