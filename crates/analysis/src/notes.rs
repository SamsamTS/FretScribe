//! Pitch refinement of hand-placed notes.

use fretscribe_core::PitchRef;

use crate::Spectrogram;

const SEARCH_SEMITONES: f32 = 0.5;
const WINDOW_SEC: f64 = 0.03;
const MIN_ENERGY: f32 = 1e-4;

/// Local energy peak within half a semitone of `approx_midi`, averaged over about +-30 ms around
/// `time_sec` (audio time of the spectrogram). Returns `approx_midi` when there is nothing to find.
pub fn refine_pitch(spec: &Spectrogram, pitch_ref: &PitchRef, time_sec: f64, approx_midi: f32) -> f32 {
    if spec.n_frames == 0 || spec.n_rows == 0 || !approx_midi.is_finite() || !time_sec.is_finite() {
        return approx_midi;
    }
    let frame = spec.sec_to_frame(time_sec).round();
    if frame < 0.0 || frame >= spec.n_frames as f64 {
        return approx_midi;
    }
    let half = (WINDOW_SEC / spec.hop_sec).ceil().max(1.0) as usize;
    let frames = (frame as usize).saturating_sub(half)..(frame as usize + half + 1).min(spec.n_frames);
    let energy = |row: usize| frames.clone().map(|f| spec.value(row, f)).sum::<f32>() / frames.len() as f32;

    let last = (spec.n_rows - 1) as f32;
    let lo = spec.midi_to_row(approx_midi - SEARCH_SEMITONES, pitch_ref).ceil().clamp(0.0, last) as usize;
    let hi = spec.midi_to_row(approx_midi + SEARCH_SEMITONES, pitch_ref).floor().clamp(0.0, last) as usize;
    if lo > hi {
        return approx_midi;
    }
    let (peak, best) = (lo..=hi)
        .map(|row| (row, energy(row)))
        .fold((lo, f32::MIN), |acc, cur| if cur.1 > acc.1 { cur } else { acc });
    if best < MIN_ENERGY {
        return approx_midi;
    }

    let mut offset = 0.0;
    if peak > 0 && peak < spec.n_rows - 1 {
        let (a, c) = (energy(peak - 1), energy(peak + 1));
        let curvature = a - 2.0 * best + c;
        if curvature < 0.0 {
            offset = (0.5 * (a - c) / curvature).clamp(-0.5, 0.5);
        }
    }
    let midi = spec.row_to_midi(peak as f32 + offset, pitch_ref);
    midi.clamp(approx_midi - SEARCH_SEMITONES, approx_midi + SEARCH_SEMITONES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::ref_from_cents;

    const BINS: u8 = 4;
    const HOP: f64 = 0.01;

    fn spec(frames: usize) -> Spectrogram {
        let (min_midi, max_midi) = (40.0, 80.0);
        let n_rows = ((max_midi - min_midi) * f32::from(BINS)) as usize + 1;
        Spectrogram {
            data: vec![0.0; n_rows * frames],
            n_frames: frames,
            n_rows,
            hop_sec: HOP,
            min_midi,
            max_midi,
            bins_per_semitone: BINS,
        }
    }

    /// Gaussian ridge of the given pitch (in project midi) with an amplitude per frame.
    fn paint(s: &mut Spectrogram, pitch_ref: &PitchRef, midi: f32, amp: impl Fn(f64) -> f32) {
        let centre = s.midi_to_row(midi, pitch_ref);
        for frame in 0..s.n_frames {
            let a = amp(frame as f64 * HOP);
            for row in 0..s.n_rows {
                let d = (row as f32 - centre) / (0.4 * f32::from(BINS));
                s.data[row * s.n_frames + frame] += a * (-0.5 * d * d).exp();
            }
        }
    }

    fn near(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn sustained_tone_is_centred_from_a_rough_click() {
        let pr = PitchRef::default();
        let mut s = spec(100);
        paint(&mut s, &pr, 64.0, |_| 0.8);
        for approx in [63.6, 63.8, 64.0, 64.3, 64.45] {
            near(refine_pitch(&s, &pr, 0.5, approx), 64.0, 0.05);
        }
    }

    #[test]
    fn decaying_pluck_is_found_late_in_its_decay() {
        let pr = PitchRef::default();
        let mut s = spec(200);
        paint(&mut s, &pr, 57.0, |t| 0.9 * (-3.0 * t as f32).exp());
        near(refine_pitch(&s, &pr, 0.1, 57.3), 57.0, 0.05);
        near(refine_pitch(&s, &pr, 1.5, 56.7), 57.0, 0.05);
    }

    #[test]
    fn off_grid_pitch_is_interpolated_between_rows() {
        let pr = PitchRef::default();
        let mut s = spec(50);
        paint(&mut s, &pr, 60.12, |_| 1.0);
        near(refine_pitch(&s, &pr, 0.25, 60.0), 60.12, 0.04);
    }

    #[test]
    fn never_leaves_half_a_semitone_and_ignores_the_neighbour_semitone() {
        let pr = PitchRef::default();
        let mut s = spec(50);
        paint(&mut s, &pr, 62.0, |_| 1.0);
        let refined = refine_pitch(&s, &pr, 0.25, 62.7);
        assert!((refined - 62.7).abs() <= 0.5 + 1e-4);
        let mut s = spec(50);
        paint(&mut s, &pr, 62.0, |_| 1.0);
        paint(&mut s, &pr, 63.0, |_| 0.5);
        near(refine_pitch(&s, &pr, 0.25, 62.2), 62.0, 0.05);
    }

    #[test]
    fn follows_a_non_default_reference_pitch() {
        let pr = ref_from_cents(30.0);
        let mut s = spec(50);
        paint(&mut s, &pr, 64.0, |_| 1.0);
        near(refine_pitch(&s, &pr, 0.25, 64.2), 64.0, 0.05);
    }

    #[test]
    fn silence_and_out_of_range_keep_the_input() {
        let pr = PitchRef::default();
        let s = spec(50);
        assert_eq!(refine_pitch(&s, &pr, 0.25, 64.2), 64.2);
        let mut s = spec(50);
        paint(&mut s, &pr, 64.0, |_| 1.0);
        assert_eq!(refine_pitch(&s, &pr, -1.0, 64.2), 64.2);
        assert_eq!(refine_pitch(&s, &pr, 99.0, 64.2), 64.2);
        assert_eq!(refine_pitch(&s, &pr, f64::NAN, 64.2), 64.2);
    }

    #[test]
    fn pitch_at_the_edge_of_the_spectrogram_does_not_panic() {
        let pr = PitchRef::default();
        let mut s = spec(50);
        paint(&mut s, &pr, 40.0, |_| 1.0);
        near(refine_pitch(&s, &pr, 0.25, 40.1), 40.0, 0.06);
        near(refine_pitch(&s, &pr, 0.25, 39.9), 40.0, 0.06);
    }
}
