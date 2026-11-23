use super::*;

const SR: u32 = 22_050;
const SECONDS: f64 = 20.0;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[derive(Clone, Copy)]
enum Timbre {
    Sine,
    Noise,
}

/// Sparse hits at irregular times, so the correlation peak is unique.
fn sparse() -> Vec<(f64, f32)> {
    let mut rng = Lcg(5);
    let mut t = 0.5;
    let mut hits = Vec::new();
    while t < SECONDS - 4.0 {
        hits.push((t, 0.4 + 0.6 * rng.next()));
        t += 0.15 + 0.6 * f64::from(rng.next());
    }
    hits
}

/// Random hits on a `step`-second grid: a steady tempo.
fn rhythm(step: f64) -> Vec<(f64, f32)> {
    let mut rng = Lcg(7);
    let mut hits = Vec::new();
    for k in 0..(SECONDS / step) as usize {
        if rng.next() < 0.5 {
            hits.push((k as f64 * step, 0.4 + 0.6 * rng.next()));
        }
    }
    hits
}

/// Hit at reference time `t` appears at `t - offset` in the rendered audio.
fn render(hits: &[(f64, f32)], offset: f64, timbre: Timbre) -> Vec<f32> {
    let mut x = vec![0.0_f32; (SECONDS * f64::from(SR)) as usize];
    let mut rng = Lcg(99);
    for &(t, amp) in hits {
        let start = ((t - offset) * f64::from(SR)).round();
        if start < 0.0 {
            continue;
        }
        for j in 0..(0.1 * f64::from(SR)) as usize {
            let tau = j as f64 / f64::from(SR);
            let body = match timbre {
                Timbre::Sine => (2.0 * PI * 1000.0 * tau).sin() as f32,
                Timbre::Noise => rng.next() * 2.0 - 1.0,
            };
            if let Some(slot) = x.get_mut(start as usize + j) {
                *slot += amp * body * (-tau / 0.02).exp() as f32;
            }
        }
    }
    x
}

fn noise(seed: u64) -> Vec<f32> {
    let mut rng = Lcg(seed);
    (0..(SECONDS * f64::from(SR)) as usize).map(|_| rng.next() - 0.5).collect()
}

fn best(reference: &[f32], other: &[f32]) -> SyncCandidate {
    estimate_offset(reference, other, SR, SyncOptions::default()).remove(0)
}

#[test]
fn recovers_known_offsets() {
    let hits = sparse();
    let reference = render(&hits, 0.0, Timbre::Sine);
    for offset in [0.25, -0.08, 3.2] {
        let c = best(&reference, &render(&hits, offset, Timbre::Sine));
        assert!((c.offset_sec - offset).abs() < 0.002, "{offset}: {c:?}");
        assert!(c.confidence > 0.5, "{offset}: {c:?}");
    }
}

#[test]
fn different_timbre_is_still_aligned() {
    let hits = sparse();
    let c = best(&render(&hits, 0.0, Timbre::Sine), &render(&hits, 0.25, Timbre::Noise));
    assert!((c.offset_sec - 0.25).abs() < 0.015, "{c:?}");
}

#[test]
fn equal_durations_are_assumed_synced() {
    assert!(assume_synced(10.0, 10.0, DEFAULT_TOLERANCE_MS));
    assert!(assume_synced(10.0, 10.015, DEFAULT_TOLERANCE_MS));
    assert!(!assume_synced(10.0, 10.5, DEFAULT_TOLERANCE_MS));
}

#[test]
fn unrelated_noise_has_low_confidence() {
    let c = best(&noise(11), &noise(0x9E37_79B9_7F4A_7C15));
    assert!(c.confidence < 0.3, "{c:?}");
}

#[test]
fn tempo_mismatch_adds_a_drift_note() {
    let reference = render(&rhythm(0.25), 0.0, Timbre::Sine);
    let c = best(&reference, &render(&rhythm(0.25 * 1.06), 0.0, Timbre::Sine));
    assert!(c.note.is_some_and(|n| n.contains("drift")));
    assert!(best(&reference, &reference).note.is_none_or(|n| !n.contains("drift")));
}

#[test]
fn periodic_pattern_is_ambiguous_and_the_grid_resolves_it() {
    let drums: Vec<(f64, f32)> = (0..40).map(|k| (k as f64 * 0.5, 1.0)).collect();
    let bass: Vec<(f64, f32)> = (0..10).map(|k| (k as f64 * 2.0, 1.0)).collect();
    let (reference, other) =
        (render(&drums, 0.0, Timbre::Sine), render(&bass, 0.0, Timbre::Sine));
    let plain = estimate_offset(&reference, &other, SR, SyncOptions::default());
    assert!(plain[0].note.as_ref().is_some_and(|n| n.contains("ambiguous")), "{plain:?}");
    assert!(plain[0].confidence < 0.5, "{plain:?}");

    let grid = fretscribe_core::generate_measures(
        0.0,
        SECONDS,
        120.0,
        fretscribe_core::TimeSignature::COMMON,
        &mut 1,
    );
    let opts = SyncOptions { grid_hint: Some(&grid), ..SyncOptions::default() };
    let hinted = estimate_offset(&reference, &other, SR, opts);
    let bar = (hinted[0].offset_sec / 2.0).round() * 2.0;
    assert!((hinted[0].offset_sec - bar).abs() < 0.005, "{hinted:?}");
}

#[test]
fn short_or_invalid_input_gives_no_candidates() {
    assert!(estimate_offset(&[0.0; 10], &[0.0; 10], SR, SyncOptions::default()).is_empty());
    assert!(estimate_offset(&noise(11), &noise(12), 0, SyncOptions::default()).is_empty());
}

#[test]
fn snap_offsets_are_nearest_first_and_bounded() {
    let offsets = snap_offsets(&[1.0, 2.0], &[1.3, 2.5, 5.0], 0.3, 0.3);
    assert_eq!(offsets.len(), 2);
    assert!(
        (offsets[0] - 0.3).abs() < 1e-12 && (offsets[1] - 0.5).abs() < 1e-12,
        "{offsets:?}"
    );
    assert!(snap_offsets(&[1.0], &[1.0, 1.00005], 0.0, 0.1).len() == 1);
    assert!(snap_offsets(&[], &[1.0], 0.0, 1.0).is_empty());
}
