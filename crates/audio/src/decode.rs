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

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/decode.rs"]
pub(crate) mod tests;
