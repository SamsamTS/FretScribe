use std::f32::consts::TAU;
use std::sync::Arc;

use fretscribe_core::{freq_to_midi_float, PitchRef};

use crate::lut::Lut;
use crate::{
    apply_gamma, compute_spectrogram, compute_spectrogram_streaming, Levels, Spectrogram,
    SpectrogramParams,
};

const RATE: u32 = 44_100;

fn tone(freq: f32, secs: f32) -> Vec<f32> {
    (0..(secs * RATE as f32) as usize)
        .map(|i| (TAU * freq * (i as f64 / f64::from(RATE)) as f32).sin())
        .collect()
}

fn mix(parts: &[Vec<f32>]) -> Vec<f32> {
    let len = parts.iter().map(Vec::len).max().unwrap_or(0);
    (0..len)
        .map(|i| {
            parts
                .iter()
                .map(|p| p.get(i).copied().unwrap_or(0.0) * 0.5)
                .sum()
        })
        .collect()
}

fn pr() -> PitchRef {
    PitchRef::default()
}

fn row_of(spec: &Spectrogram, hz: f32) -> f32 {
    spec.midi_to_row(freq_to_midi_float(hz, &pr()), &pr())
}

fn frame_at(spec: &Spectrogram, sec: f64) -> usize {
    spec.sec_to_frame(sec).round() as usize
}

fn column(spec: &Spectrogram, frame: usize) -> Vec<f32> {
    (0..spec.n_rows).map(|r| spec.value(r, frame)).collect()
}

fn local_peaks(values: &[f32], min_height: f32) -> Vec<usize> {
    (1..values.len() - 1)
        .filter(|&i| {
            values[i] >= min_height && values[i] > values[i - 1] && values[i] >= values[i + 1]
        })
        .collect()
}

fn peak_row(values: &[f32]) -> f32 {
    let at = values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i)
        .clamp(1, values.len() - 2);
    let (a, b, c) = (values[at - 1], values[at], values[at + 1]);
    let denom = a - 2.0 * b + c;
    at as f32
        + if denom.abs() > 1e-9 {
            0.5 * (a - c) / denom
        } else {
            0.0
        }
}

fn half_width_rows(values: &[f32]) -> usize {
    let peak = values.iter().copied().fold(0.0, f32::max);
    values.iter().filter(|&&v| v >= peak - 0.1).count()
}

#[test]
fn grid_geometry() {
    let spec = compute_spectrogram(&tone(440.0, 0.5), RATE, &SpectrogramParams::default());
    assert_eq!(spec.n_rows, 64 * 4 + 1);
    assert_eq!(spec.data.len(), spec.n_rows * spec.n_frames);
    assert_eq!(spec.n_frames, (RATE as usize / 2).div_ceil(512));
    assert!((spec.hop_sec - 512.0 / f64::from(RATE)).abs() < 1e-12);
    assert!((spec.max_midi - 100.0).abs() < 1e-6);
    assert!((spec.row_to_midi(132.0, &pr()) - 69.0).abs() < 1e-4);
    assert!((spec.midi_to_row(69.0, &pr()) - 132.0).abs() < 1e-3);
    let shifted = PitchRef::new(442.0).expect("reference");
    let round_trip = spec.midi_to_row(spec.row_to_midi(77.0, &shifted), &shifted);
    assert!((round_trip - 77.0).abs() < 1e-2);
    assert!(spec.row_to_midi(132.0, &shifted) < 69.0);
}

#[test]
fn sine_peaks_at_its_row() {
    for hz in [110.0, 440.0, 1000.0, 2000.0] {
        let spec = compute_spectrogram(&tone(hz, 1.5), RATE, &SpectrogramParams::default());
        let frame = frame_at(&spec, 0.75);
        let peak = peak_row(&column(&spec, frame));
        let midi = spec.row_to_midi(peak, &pr());
        let expected = freq_to_midi_float(hz, &pr());
        assert!(
            (midi - expected).abs() < 0.25,
            "{hz} Hz: {midi} vs {expected}"
        );
        assert!(
            spec.value(peak.round() as usize, frame) > 0.9,
            "{hz} Hz is too weak"
        );
    }
}

#[test]
fn two_sines_give_two_peaks() {
    for (f1, f2) in [(440.0, 660.0), (82.41, 87.31), (196.0, 207.65)] {
        let signal = mix(&[tone(f1, 2.5), tone(f2, 2.5)]);
        let spec = compute_spectrogram(&signal, RATE, &SpectrogramParams::default());
        let column = column(&spec, frame_at(&spec, 1.25));
        let peaks = local_peaks(&column, 0.8);
        assert_eq!(peaks.len(), 2, "{f1}/{f2}: {peaks:?}");
        for (peak, hz) in peaks.iter().zip([f1, f2]) {
            let expected = row_of(&spec, hz);
            assert!(
                (*peak as f32 - expected).abs() <= 1.0,
                "{hz}: {peak} vs {expected}"
            );
        }
        let valley = column[peaks[0]..=peaks[1]]
            .iter()
            .copied()
            .fold(1.0, f32::min);
        assert!(
            valley < column[peaks[0]].min(column[peaks[1]]) - 0.05,
            "{f1}/{f2} not separated"
        );
    }
}

fn burst_signal() -> Vec<f32> {
    let mut signal = vec![0.0; RATE as usize * 2];
    let burst = tone(440.0, 0.02);
    for start in [0.5, 0.6] {
        let offset = (start * f64::from(RATE)) as usize;
        signal[offset..offset + burst.len()].copy_from_slice(&burst);
    }
    signal
}

#[test]
fn bursts_100ms_apart_are_resolved_in_time() {
    let spec = compute_spectrogram(&burst_signal(), RATE, &SpectrogramParams::default());
    let row = row_of(&spec, 440.0).round() as usize;
    let line = spec.row(row);
    let peaks = local_peaks(line, 0.0);
    let strongest: Vec<usize> = peaks.into_iter().filter(|&i| line[i] > 0.3).collect();
    assert_eq!(strongest.len(), 2, "{strongest:?}");
    let separation = (strongest[1] - strongest[0]) as f64 * spec.hop_sec;
    assert!((separation - 0.1).abs() < 0.02, "{separation}");
    let valley = line[strongest[0]..=strongest[1]]
        .iter()
        .copied()
        .fold(1.0, f32::min);
    assert!(valley < line[strongest[0]].min(line[strongest[1]]) - 0.05);
}

#[test]
fn larger_q_is_sharper_in_pitch_and_blurrier_in_time() {
    let params = |q| SpectrogramParams {
        q,
        ..SpectrogramParams::default()
    };
    let sine = tone(440.0, 1.5);
    let narrow = compute_spectrogram(&sine, RATE, &params(80.0));
    let wide = compute_spectrogram(&sine, RATE, &params(20.0));
    let at = |s: &Spectrogram| half_width_rows(&column(s, frame_at(s, 0.75)));
    assert!(at(&narrow) < at(&wide), "{} vs {}", at(&narrow), at(&wide));

    let click = {
        let mut signal = vec![0.0; RATE as usize * 2];
        let burst = tone(440.0, 0.01);
        let offset = RATE as usize;
        signal[offset..offset + burst.len()].copy_from_slice(&burst);
        signal
    };
    let smear = |q| {
        let s = compute_spectrogram(&click, RATE, &params(q));
        let line = s.row(row_of(&s, 440.0).round() as usize);
        line.iter().filter(|&&v| v > 0.5).count()
    };
    assert!(
        smear(80.0) > smear(20.0),
        "{} vs {}",
        smear(80.0),
        smear(20.0)
    );
}

#[test]
fn equal_intervals_have_equal_row_distance() {
    let spec = compute_spectrogram(&tone(220.0, 0.2), RATE, &SpectrogramParams::default());
    let (a, b, c) = (
        row_of(&spec, 110.0),
        row_of(&spec, 220.0),
        row_of(&spec, 440.0),
    );
    assert!(((b - a) - (c - b)).abs() < 1e-2);
    assert!(((b - a) - 48.0).abs() < 1e-2);

    let measured: Vec<f32> = [110.0, 220.0, 440.0]
        .into_iter()
        .map(|hz| {
            let s = compute_spectrogram(&tone(hz, 1.5), RATE, &SpectrogramParams::default());
            peak_row(&column(&s, frame_at(&s, 0.75)))
        })
        .collect();
    assert!(
        ((measured[1] - measured[0]) - (measured[2] - measured[1])).abs() < 0.5,
        "{measured:?}"
    );
}

#[test]
fn streaming_matches_the_blocking_computation() {
    let signal: Arc<[f32]> = mix(&[tone(330.0, 3.0), tone(1200.0, 1.0)]).into();
    let params = SpectrogramParams::default();
    let stream = compute_spectrogram_streaming(signal.clone(), RATE, &params);
    let mut streamed = Spectrogram::empty(&stream.info);
    let mut next_start = 0;
    let mut count = 0;
    for chunk in stream.chunks.iter() {
        assert_eq!(chunk.start_frame, next_start);
        next_start += chunk.n_frames;
        count += 1;
        streamed.apply_chunk(&chunk);
    }
    assert!(count > 1);
    assert_eq!(next_start, streamed.n_frames);
    assert_eq!(streamed, compute_spectrogram(&signal, RATE, &params));
}

#[test]
fn degenerate_inputs_do_not_panic() {
    let params = SpectrogramParams::default();
    assert_eq!(compute_spectrogram(&[], RATE, &params).n_frames, 0);
    assert_eq!(compute_spectrogram(&[0.5], RATE, &params).n_frames, 1);
    let silent = compute_spectrogram(&vec![0.0; 10_000], RATE, &params);
    assert!(silent.data.iter().all(|&v| v == 0.0));
    let odd = SpectrogramParams {
        max_midi: 10.0,
        bins_per_semitone: 0,
        hop_size: 0,
        ..params
    };
    assert!(compute_spectrogram(&tone(440.0, 0.1), 22_050, &odd).n_rows > 0);
}

fn grey_lut() -> Lut {
    let mut lut = Lut::builtin();
    for (i, c) in lut.colors.iter_mut().enumerate() {
        *c = [i as u8, i as u8, i as u8, 255];
    }
    lut
}

fn one_pixel(spec: &Spectrogram, lut: &Lut, levels: Levels, gamma: f32) -> u8 {
    let frames = spec.n_frames as f64 * spec.hop_sec;
    let range = (spec.min_midi - 0.05, spec.min_midi + 0.05);
    spec.render_rgba((0.0, frames * 0.75), 1, 1, range, &pr(), lut, levels, gamma)[0]
}

fn hand_made(values: &[f32]) -> Spectrogram {
    Spectrogram {
        data: values.to_vec(),
        n_frames: values.len(),
        n_rows: 1,
        hop_sec: 0.01,
        min_midi: 60.0,
        max_midi: 60.0,
        bins_per_semitone: 4,
    }
}

#[test]
fn render_levels_and_gamma_are_display_only() {
    let signal = tone(440.0, 1.0);
    let spec = compute_spectrogram(&signal, RATE, &SpectrogramParams::default());
    let before = spec.data.clone();
    let lut = Lut::builtin();
    let render = |levels, gamma| {
        spec.render_rgba(
            (0.2, 0.8),
            64,
            128,
            (60.0, 80.0),
            &pr(),
            &lut,
            levels,
            gamma,
        )
    };
    let base = render(Levels::default(), 1.0);
    assert_eq!(base.len(), 64 * 128 * 4);
    assert_ne!(base, render(Levels::new(0.4, 1.0), 1.0));
    assert_ne!(base, render(Levels::default(), 0.5));
    assert_ne!(base, render(Levels::default(), 2.0));
    assert_eq!(spec.data, before);
}

#[test]
fn black_point_above_value_gives_first_colour() {
    let lut = Lut::builtin();
    let spec = hand_made(&[0.4, 0.4]);
    let pixel = |levels| {
        let image = spec.render_rgba((0.0, 0.02), 1, 1, (59.95, 60.05), &pr(), &lut, levels, 1.0);
        [image[0], image[1], image[2], image[3]]
    };
    assert_eq!(pixel(Levels::new(0.5, 1.0)), lut.colors[0]);
    assert_ne!(pixel(Levels::new(0.0, 1.0)), lut.colors[0]);
    assert_eq!(pixel(Levels::new(0.0, 0.2)), lut.colors[255]);
}

#[test]
fn gamma_is_applied_after_downsampling() {
    let lut = grey_lut();
    let spec = hand_made(&[0.0, 1.0]);
    for gamma in [0.5, 2.0, 3.0] {
        let got = one_pixel(&spec, &lut, Levels::default(), gamma);
        let curve_of_average = (apply_gamma(0.5, gamma) * 255.0).round() as u8;
        let average_of_curve = (0.5 * apply_gamma(1.0, gamma) * 255.0).round() as u8;
        assert_eq!(got, curve_of_average, "gamma {gamma}");
        assert_ne!(got, average_of_curve, "gamma {gamma}");
    }
}

#[test]
fn render_places_pitch_with_y_up_and_follows_reference_pitch() {
    let spec = compute_spectrogram(&tone(440.0, 1.0), RATE, &SpectrogramParams::default());
    let lut = grey_lut();
    let bright_midi = |reference: &PitchRef| {
        let image = spec.render_rgba(
            (0.4, 0.6),
            4,
            400,
            (60.0, 80.0),
            reference,
            &lut,
            Levels::default(),
            1.0,
        );
        let brightness =
            |y: usize| -> u32 { (0..4).map(|x| u32::from(image[(y * 4 + x) * 4])).sum() };
        let y = (0..400).max_by_key(|&y| brightness(y)).unwrap_or(0);
        80.0 - (y as f32 + 0.5) * 20.0 / 400.0
    };
    assert!((bright_midi(&pr()) - 69.0).abs() < 0.3);
    let sharp = PitchRef::new(450.0).expect("reference");
    let shifted = bright_midi(&sharp);
    assert!((shifted - 68.61).abs() < 0.2, "{shifted}");
}
