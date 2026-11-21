use super::*;
use std::f64::consts::PI;

const SR: u32 = 44_100;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32 - 0.5
    }
}

const MELODY: [u32; 8] = [45, 52, 57, 60, 64, 67, 71, 76];

/// Decaying harmonic notes (partials 1..=8 at 1/n) tuned to `a4`, optionally with vibrato.
fn tones(a4: f64, transpose: u32, vibrato_cents: f64) -> Vec<f32> {
    let (note_len, rate) = (1.2, f64::from(SR));
    let mut out = vec![0.0_f32; (MELODY.len() as f64 * note_len * rate) as usize];
    for (k, midi) in MELODY.iter().enumerate() {
        let f0 = a4 * 2f64.powf((f64::from(midi + transpose) - 69.0) / 12.0);
        let mut phase = 0.0;
        for j in 0..(note_len * rate) as usize {
            let t = j as f64 / rate;
            let bend = 2f64.powf(vibrato_cents / 1200.0 * (2.0 * PI * 5.0 * t).sin());
            phase += 2.0 * PI * f0 * bend / rate;
            let env = (1.0 - (-t / 0.01).exp()) * (-t / 0.8).exp();
            let sum: f64 = (1..=8).map(|n| (phase * f64::from(n)).sin() / f64::from(n)).sum();
            out[k * (note_len * rate) as usize + j] += (0.3 * env * sum) as f32;
        }
    }
    out
}

fn estimate(x: &[f32]) -> PitchRefEstimate {
    estimate_reference(x, SR, None, PitchRefOptions::default())
}

fn cents_of(hz: f64) -> f64 {
    1200.0 * (hz / 440.0).log2()
}

#[test]
fn recovers_references_within_a_cent() {
    for hz in [432.0, 442.0, 448.0, 440.0] {
        let e = estimate(&tones(hz, 0, 0.0));
        assert!((f64::from(e.cents) - cents_of(hz)).abs() < 1.0, "{hz}: {e:?}");
        assert!((f64::from(e.a4_hz) - hz).abs() < 0.2, "{hz}: {e:?}");
        assert!(e.confidence > 0.6, "{hz}: {}", e.confidence);
    }
}

#[test]
fn a_whole_semitone_up_reads_zero() {
    let e = estimate(&tones(440.0, 1, 0.0));
    assert!(e.cents.abs() < 1.0, "{e:?}");
}

#[test]
fn noise_and_vibrato_lower_the_confidence() {
    let clean = estimate(&tones(432.0, 0, 0.0)).confidence;
    let mut rng = Lcg(3);
    let mut noisy = tones(432.0, 0, 0.0);
    noisy.iter_mut().for_each(|s| *s += 40.0 * rng.next());
    let noisy = estimate(&noisy).confidence;
    assert!(noisy < clean - 0.1, "clean {clean} noisy {noisy}");
    let vibrato = estimate(&tones(432.0, 0, 30.0)).confidence;
    assert!(vibrato < clean - 0.2, "clean {clean} vibrato {vibrato}");
}

#[test]
fn noise_and_drums_have_low_confidence() {
    let mut rng = Lcg(11);
    let noise: Vec<f32> = (0..10 * SR as usize).map(|_| rng.next()).collect();
    assert!(estimate(&noise).confidence < 0.3, "{}", estimate(&noise).confidence);

    let mut drums = vec![0.0_f32; 10 * SR as usize];
    for hit in 0..40 {
        for j in 0..(0.15 * f64::from(SR)) as usize {
            let t = j as f32 / SR as f32;
            let kick =
                (2.0 * std::f32::consts::PI * (120.0 * (-t * 20.0).exp() + 50.0) * t).sin();
            let body = if hit % 2 == 0 { kick } else { rng.next() * 2.0 };
            drums[hit * SR as usize / 4 + j] += body * (-t / 0.03).exp();
        }
    }
    assert!(estimate(&drums).confidence < 0.3, "{}", estimate(&drums).confidence);
}

#[test]
fn region_limits_the_analysis() {
    let mut x = tones(432.0, 0, 0.0);
    x.extend(tones(448.0, 0, 0.0));
    let seconds = MELODY.len() as f64 * 1.2;
    let opts = PitchRefOptions::default();
    let first = estimate_reference(&x, SR, Some((0.5, seconds - 0.5)), opts);
    let second = estimate_reference(&x, SR, Some((seconds + 0.5, 2.0 * seconds - 0.5)), opts);
    assert!((f64::from(first.cents) - cents_of(432.0)).abs() < 1.0, "{first:?}");
    assert!((f64::from(second.cents) - cents_of(448.0)).abs() < 1.0, "{second:?}");
}

#[test]
fn long_files_use_the_tonal_stretch() {
    let mut rng = Lcg(5);
    let mut x: Vec<f32> = (0..12 * SR as usize).map(|_| 0.3 * rng.next()).collect();
    x.extend(tones(432.0, 0, 0.0));
    let opts = PitchRefOptions { max_analysis_sec: 10.0, ..PitchRefOptions::default() };
    let e = estimate_reference(&x, SR, None, opts);
    assert!((f64::from(e.cents) - cents_of(432.0)).abs() < 1.0, "{e:?}");
}

#[test]
fn degenerate_input_is_inert() {
    for x in [&[][..], &[0.0; 100][..], &vec![0.0; 50_000][..]] {
        let e = estimate(x);
        assert_eq!((e.cents, e.confidence, e.histogram.len()), (0.0, 0.0, HISTOGRAM_BINS));
    }
    assert_eq!(
        estimate_reference(&[0.5; 50_000], 0, None, PitchRefOptions::default()).confidence,
        0.0
    );
}

#[test]
fn histogram_is_normalised() {
    let e = estimate(&tones(442.0, 0, 0.0));
    assert!((e.histogram.iter().sum::<f32>() - 1.0).abs() < 1e-4);
}
