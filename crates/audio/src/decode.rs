use std::fmt;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{AudioError, Result};

#[derive(Clone)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: u16,
    /// Channel average, for analysis.
    pub mono: Vec<f32>,
    /// Original channel layout, for playback.
    pub interleaved: Vec<f32>,
    pub duration_sec: f64,
}

impl fmt::Debug for DecodedAudio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedAudio")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("frames", &self.mono.len())
            .field("duration_sec", &self.duration_sec)
            .finish()
    }
}

impl DecodedAudio {
    pub fn from_interleaved(sample_rate: u32, channels: u16, interleaved: Vec<f32>) -> Result<Self> {
        if sample_rate == 0 || channels == 0 {
            return Err(AudioError::Unsupported(format!(
                "{sample_rate} Hz, {channels} channel(s)"
            )));
        }
        let width = usize::from(channels);
        let frames = interleaved.len() / width;
        if frames == 0 {
            return Err(AudioError::Empty);
        }
        let mut interleaved = interleaved;
        interleaved.truncate(frames * width);
        let mono = interleaved
            .chunks_exact(width)
            .map(|frame| frame.iter().sum::<f32>() / f32::from(channels))
            .collect();
        Ok(Self {
            sample_rate,
            channels,
            mono,
            interleaved,
            duration_sec: frames as f64 / f64::from(sample_rate),
        })
    }

    pub fn frames(&self) -> usize {
        self.mono.len()
    }
}

pub fn decode_file(path: impl AsRef<Path>) -> Result<DecodedAudio> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format_options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let probed = symphonia::default::get_probe().format(
        &hint,
        stream,
        &format_options,
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(AudioError::NoAudioTrack)?;
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut layout: Option<(u32, u16)> = None;
    let mut samples: Vec<f32> = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(reason)) => {
                log::warn!("skipping undecodable packet: {reason}");
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let found = (spec.rate, u16::try_from(spec.channels.count()).unwrap_or(u16::MAX));
        match layout {
            None => layout = Some(found),
            Some(expected) if expected != found => {
                return Err(AudioError::Unsupported(
                    "the stream changes sample rate or channel count".to_owned(),
                ));
            }
            Some(_) => {}
        }
        let needed = decoded.capacity() as u64;
        let sample_buffer = match &mut buffer {
            Some(b) if b.capacity() as u64 >= needed * u64::from(found.1) => b,
            slot => slot.insert(SampleBuffer::new(needed, spec)),
        };
        sample_buffer.copy_interleaved_ref(decoded);
        samples.extend_from_slice(sample_buffer.samples());
    }

    let (sample_rate, channels) = layout.ok_or(AudioError::Empty)?;
    DecodedAudio::from_interleaved(sample_rate, channels, samples)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    pub fn wav_bytes(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    fn sine_i16(freq: f64, rate: u32, frames: usize, amp: f64) -> Vec<i16> {
        (0..frames)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                (amp * (std::f64::consts::TAU * freq * t).sin() * 32767.0).round() as i16
            })
            .collect()
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
        let path = write(dir.path(), "mono.wav", &wav_bytes(44_100, 1, &pcm));
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
        let path = write(dir.path(), "stereo.wav", &wav_bytes(48_000, 2, &pcm));
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
        let empty = write(dir.path(), "empty.wav", &wav_bytes(44_100, 1, &[]));
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
}
