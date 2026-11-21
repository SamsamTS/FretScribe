use super::*;

fn pitch_by_autocorrelation(x: &[f32], rate: u32, low: f64, high: f64) -> f64 {
    let lag_min = (f64::from(rate) / high) as usize;
    let lag_max = (f64::from(rate) / low) as usize;
    let corr = |lag: usize| -> f64 {
        x.iter().zip(&x[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum()
    };
    let best = (lag_min..=lag_max)
        .max_by(|&a, &b| corr(a).total_cmp(&corr(b)))
        .unwrap();
    let (y0, y1, y2) = (corr(best - 1), corr(best), corr(best + 1));
    let shift = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
    f64::from(rate) / (best as f64 + shift)
}

fn cents(measured: f64, expected: f64) -> f64 {
    1200.0 * (measured / expected).log2()
}

#[test]
fn notes_are_in_tune_for_every_reference_pitch() {
    for (a4, reference) in [(440.0, PitchRef::default()), (432.0, PitchRef::new(432.0).unwrap())] {
        for (midi, ratio) in [(45u8, 0.25), (57, 0.5), (69, 1.0), (81, 2.0)] {
            let audio = NoteSynth::render_offline(&[(midi, 0.0, 1.0)], 48_000, reference, 1.0);
            let expected = a4 * ratio;
            let measured = pitch_by_autocorrelation(&audio[2_000..8_000], 48_000, expected * 0.8, expected * 1.25);
            assert!(cents(measured, expected).abs() < 3.0, "{midi} {measured} {expected}");
        }
    }
}

#[test]
fn plucks_start_on_time_and_stop_after_release() {
    let audio = NoteSynth::render_offline(&[(60, 0.5, 0.75)], 48_000, PitchRef::default(), 1.5);
    let first = audio.iter().position(|s| s.abs() > 1e-4).unwrap();
    assert!((first as i64 - 24_000).abs() <= 64, "{first}");
    let tail = &audio[(0.75 * 48_000.0) as usize + 2_000..];
    assert!(tail.iter().all(|s| s.abs() < 1e-6));
    assert!(audio.iter().all(|s| s.abs() < 1.0));
}

#[test]
fn invalid_notes_are_dropped_and_overlaps_are_polyphonic() {
    let mut synth = NoteSynth::new(48_000);
    synth.set_notes([(200, 0.0, 1.0), (60, f64::NAN, 1.0), (60, 0.2, 0.1), (64, 0.0, 1.0)]);
    assert_eq!(synth.notes.len(), 2);
    let chord = NoteSynth::render_offline(&[(60, 0.0, 1.0), (64, 0.0, 1.0), (67, 0.0, 1.0)], 48_000, PitchRef::default(), 0.5);
    let single = NoteSynth::render_offline(&[(60, 0.0, 1.0)], 48_000, PitchRef::default(), 0.5);
    let energy = |x: &[f32]| x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
    assert!(energy(&chord) > 2.0 * energy(&single));
}

#[test]
fn many_simultaneous_notes_steal_the_oldest_voice() {
    let notes: Vec<(u8, f64, f64)> = (0..40).map(|i| (40 + i as u8, 0.0, 1.0)).collect();
    let audio = NoteSynth::render_offline(&notes, 48_000, PitchRef::default(), 0.2);
    assert!(audio.iter().any(|s| s.abs() > 0.01));
    assert!(audio.iter().all(|s| s.is_finite()));
}

#[test]
fn seek_skips_earlier_notes() {
    let mut synth = NoteSynth::new(48_000);
    synth.set_notes([(60, 0.1, 1.0), (62, 0.5, 1.0)]);
    synth.seek(0.3);
    let mut out = vec![0.0; 64];
    synth.render(0.0, 0.0013, &mut out);
    assert!(out.iter().all(|s| *s == 0.0));
    synth.render(0.5, 0.5013, &mut out);
    assert!(out.iter().any(|s| *s != 0.0));
}
