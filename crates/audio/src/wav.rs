//! Minimal 16-bit PCM WAV encoder, for fixtures and examples.

/// A canonical RIFF/WAVE file holding interleaved 16-bit samples.
pub fn encode(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
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
    out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    out
}

/// Mono file from float samples (clamped to -1..1).
pub fn encode_mono_f32(sample_rate: u32, samples: &[f32]) -> Vec<u8> {
    let pcm: Vec<i16> = samples.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
    encode(sample_rate, 1, &pcm)
}
