use super::*;
use crate::testutil::sine_frames;

pub(crate) fn zero_crossing_freq(x: &[f32], rate: u32) -> f64 {
    let mut crossings = Vec::new();
    for i in 1..x.len() {
        if x[i - 1] < 0.0 && x[i] >= 0.0 {
            crossings.push(i as f64 - 1.0 + f64::from(-x[i - 1]) / f64::from(x[i] - x[i - 1]));
        }
    }
    let n = crossings.len() - 1;
    f64::from(rate) * n as f64 / (crossings[n] - crossings[0])
}

/// The original whole-buffer implementation, kept to prove the streaming one is equivalent.
fn reference_resample(audio: &DecodedAudio, to: u32) -> Vec<f32> {
    let width = usize::from(audio.channels);
    let right = width.min(2) - 1;
    let (left, right): (Vec<f32>, Vec<f32>) = audio
        .interleaved
        .chunks_exact(width)
        .map(|frame| (frame[0], frame[right]))
        .unzip();
    let mut resampler =
        FftFixedIn::<f32>::new(audio.sample_rate as usize, to as usize, CHUNK_FRAMES, 2, 2).unwrap();
    let frames = left.len();
    let delay = resampler.output_delay();
    let expected = (frames as f64 * f64::from(to) / f64::from(audio.sample_rate)).round() as usize;
    let mut out = [Vec::new(), Vec::new()];
    let mut pos = 0;
    while pos + resampler.input_frames_next() <= frames {
        let need = resampler.input_frames_next();
        let input = [&left[pos..pos + need], &right[pos..pos + need]];
        for (dst, src) in out.iter_mut().zip(resampler.process(&input, None).unwrap()) {
            dst.extend(src);
        }
        pos += need;
    }
    if pos < frames {
        let input = [&left[pos..], &right[pos..]];
        for (dst, src) in out.iter_mut().zip(resampler.process_partial(Some(&input), None).unwrap()) {
            dst.extend(src);
        }
    }
    while out[0].len() < expected + delay {
        for (dst, src) in out
            .iter_mut()
            .zip(resampler.process_partial(None::<&[Vec<f32>]>, None).unwrap())
        {
            dst.extend(src);
        }
    }
    let [l, r] = out;
    l[delay..delay + expected]
        .iter()
        .zip(&r[delay..delay + expected])
        .flat_map(|(&a, &b)| [a, b])
        .collect()
}

#[test]
fn streaming_resample_is_identical_to_the_reference_implementation() {
    let cases = [
        (44_100, 48_000, 1, 100_000),
        (44_100, 48_000, 2, 100_001),
        (22_050, 48_000, 2, 5_000),
        (48_000, 44_100, 2, 70_001),
        (96_000, 48_000, 3, 30_000),
        (44_100, 48_000, 2, 700),
        (44_100, 48_000, 2, 1_024),
    ];
    for (from, to, channels, frames) in cases {
        let samples: Vec<f32> = (0..frames * channels)
            .map(|i| ((i * 7919) % 2003) as f32 / 1001.5 - 1.0)
            .collect();
        let audio = DecodedAudio::from_interleaved(from, channels as u16, samples).unwrap();
        let streamed = resample_to_stereo(&audio, to).unwrap();
        let reference = reference_resample(&audio, to);
        assert_eq!(streamed.len(), reference.len(), "{from}->{to} ch{channels} n{frames}");
        let worst = streamed
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1e-6, "{from}->{to} ch{channels} n{frames}: {worst}");
    }
}

#[test]
fn same_rate_preparation_is_a_plain_conversion() {
    let stereo = DecodedAudio::from_interleaved(48_000, 2, vec![0.1, 0.2, 0.3, 0.4]).unwrap();
    assert_eq!(to_stereo(&stereo), [0.1, 0.2, 0.3, 0.4]);
    let mono = DecodedAudio::from_interleaved(48_000, 1, vec![0.5, 0.6]).unwrap();
    assert_eq!(to_stereo(&mono), [0.5, 0.5, 0.6, 0.6]);
    let handle = PreparedAudio::new(&stereo, 48_000).unwrap();
    assert_eq!((handle.sample_rate(), handle.frames()), (48_000, 2));
    assert!((handle.duration_sec() - 2.0 / 48_000.0).abs() < 1e-12);
}

#[test]
fn resampling_keeps_length_and_pitch() {
    let mono = sine_frames(440.0, 44_100, 44_100, 1.0);
    let audio = DecodedAudio::from_interleaved(44_100, 1, mono).unwrap();
    let prepared = Prepared::from_decoded(&audio, 48_000).unwrap();
    assert_eq!(prepared.frames(), 48_000);
    let left: Vec<f32> = (0..48_000).map(|i| prepared.frame(i)[0]).collect();
    assert!((zero_crossing_freq(&left[2_000..46_000], 48_000) - 440.0).abs() < 0.2);
    let peak = left[2_000..46_000].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    assert!((peak - 1.0).abs() < 0.02, "{peak}");
}

#[test]
fn resampling_stays_aligned_with_the_source() {
    let mut mono = vec![0.0; 44_100];
    mono[22_050] = 1.0;
    let audio = DecodedAudio::from_interleaved(44_100, 1, mono).unwrap();
    let prepared = Prepared::from_decoded(&audio, 48_000).unwrap();
    let loudest = (0..prepared.frames() as i64)
        .max_by(|&a, &b| prepared.frame(a)[0].abs().total_cmp(&prepared.frame(b)[0].abs()))
        .unwrap();
    assert!((loudest - 24_000).abs() <= 1, "{loudest}");
}

#[test]
fn channel_layouts() {
    let mono = DecodedAudio::from_interleaved(48_000, 1, vec![0.25, 0.5]).unwrap();
    let p = Prepared::from_decoded(&mono, 48_000).unwrap();
    assert_eq!(p.frame(1), [0.5, 0.5]);
    let surround = DecodedAudio::from_interleaved(48_000, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
    let p = Prepared::from_decoded(&surround, 48_000).unwrap();
    assert_eq!((p.frame(0), p.frame(1)), ([1.0, 2.0], [4.0, 5.0]));
}

#[test]
fn fill_pads_with_silence_and_cubic_is_exact_on_samples() {
    let p = Prepared::from_channels(&[1.0, 2.0, 3.0], &[-1.0, -2.0, -3.0]);
    let mut out = [9.0; 10];
    p.fill(-1, &mut out);
    assert_eq!(out, [0.0, 0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 0.0, 0.0]);
    let mut out = [9.0; 4];
    p.fill(10, &mut out);
    assert_eq!(out, [0.0; 4]);
    assert_eq!(p.cubic(1.0), [2.0, -2.0]);

    let ramp = Prepared::from_channels(&[0.0, 1.0, 2.0, 3.0, 4.0], &[0.0, -1.0, -2.0, -3.0, -4.0]);
    let mid = ramp.cubic(2.25);
    assert!((mid[0] - 2.25).abs() < 1e-5 && (mid[1] + 2.25).abs() < 1e-5);
}
