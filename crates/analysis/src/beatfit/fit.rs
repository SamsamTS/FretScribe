use fretscribe_core::{valid_tempo, BeatKind, FittedMeasure, Measure, TempoMap};

use super::OnsetEnvelope;

const DOWNBEAT_WEIGHT: f64 = 2.0;
const BEAT_WEIGHT: f64 = 1.0;
const SUBDIVISION_WEIGHT: f64 = 0.5;
const MIN_LENGTH_RATIO: f64 = 0.5;
const MIN_CONFIDENCE: f64 = 0.2;
/// Penalties per squared beat of shift change / of absolute shift.
const SMOOTHNESS: f64 = 2.0;
const PRIOR: f64 = 0.05;
const LENGTH_EPS: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureFit {
    pub index: usize,
    pub new_start_sec: f64,
    pub confidence: f32,
}

pub fn fit_measures(
    env: &OnsetEnvelope,
    measures: &[Measure],
    max_shift_beats: f64,
) -> Vec<MeasureFit> {
    let n = measures.len();
    let keep = |confidence: f64| -> Vec<MeasureFit> {
        measures
            .iter()
            .enumerate()
            .map(|(index, m)| MeasureFit { index, new_start_sec: m.start_sec, confidence: confidence as f32 })
            .collect()
    };
    if n == 0 || env.values.is_empty() || env.hop_sec <= 0.0 || !max_shift_beats.is_finite() || max_shift_beats <= 0.0 {
        return keep(0.0);
    }

    let map = TempoMap::new(measures);
    let step = env.hop_sec / 2.0;
    let sec_per_beat: Vec<f64> = (0..n).map(|i| 60.0 / map.effective_bpm(i)).collect();
    let radius: Vec<isize> = sec_per_beat
        .iter()
        .map(|spb| (max_shift_beats * spb / step).floor() as isize)
        .collect();
    let r = radius.iter().copied().max().unwrap_or(0);
    let states = (2 * r + 1) as usize;
    let offset = |j: usize| (j as isize - r) as f64 * step;

    let emission: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let lines: Vec<(f64, f64)> = map
                .beat_lines(i)
                .iter()
                .map(|l| {
                    let weight = match l.kind {
                        BeatKind::Measure => DOWNBEAT_WEIGHT,
                        BeatKind::MainBeat => BEAT_WEIGHT,
                        BeatKind::Subdivision => SUBDIVISION_WEIGHT,
                    };
                    (l.time_sec, weight)
                })
                .collect();
            let total: f64 = lines.iter().map(|l| l.1).sum();
            (0..states)
                .map(|j| {
                    if (j as isize - r).abs() > radius[i] {
                        return f64::NEG_INFINITY;
                    }
                    let sum: f64 = lines
                        .iter()
                        .map(|&(t, w)| w * f64::from(env.at(t + offset(j))))
                        .sum();
                    sum / total
                })
                .collect()
        })
        .collect();

    let unary = |i: usize, j: usize| emission[i][j] - PRIOR * (offset(j) / sec_per_beat[i]).powi(2);
    let mut score: Vec<f64> = (0..states).map(|j| unary(0, j)).collect();
    let mut back = vec![vec![0usize; states]; n];
    for i in 1..n {
        let gap = measures[i].start_sec - measures[i - 1].start_sec;
        let mut next = vec![f64::NEG_INFINITY; states];
        for j in 0..states {
            if emission[i][j] == f64::NEG_INFINITY {
                continue;
            }
            for (k, &previous) in score.iter().enumerate() {
                let shrunk = gap + offset(j) - offset(k) < MIN_LENGTH_RATIO * gap - LENGTH_EPS;
                if previous == f64::NEG_INFINITY || shrunk {
                    continue;
                }
                let jump = (offset(j) - offset(k)) / sec_per_beat[i];
                let total = previous - SMOOTHNESS * jump * jump + unary(i, j);
                if total > next[j] {
                    next[j] = total;
                    back[i][j] = k;
                }
            }
        }
        score = next;
    }

    let mut state = (0..states)
        .max_by(|&a, &b| score[a].total_cmp(&score[b]))
        .unwrap_or(r as usize);
    let mut chosen = vec![0usize; n];
    for i in (0..n).rev() {
        chosen[i] = state;
        state = back[i][state];
    }

    let confidence: Vec<f64> = (0..n)
        .map(|i| {
            let valid: Vec<f64> = emission[i].iter().copied().filter(|e| e.is_finite()).collect();
            let mean = valid.iter().sum::<f64>() / valid.len() as f64;
            let best = emission[i][chosen[i]];
            ((best - mean) / (best + 1e-12)).clamp(0.0, 1.0)
        })
        .collect();
    let mut shift: Vec<f64> = (0..n)
        .map(|i| if confidence[i] >= MIN_CONFIDENCE { offset(chosen[i]) } else { 0.0 })
        .collect();

    // Reverting a low-confidence neighbour can break the length rule; undo both ends until stable.
    loop {
        let mut changed = false;
        for i in 1..n {
            let gap = measures[i].start_sec - measures[i - 1].start_sec;
            if gap + shift[i] - shift[i - 1] < MIN_LENGTH_RATIO * gap - LENGTH_EPS {
                for k in [i - 1, i] {
                    changed |= shift[k] != 0.0;
                    shift[k] = 0.0;
                }
            }
        }
        if !changed {
            break;
        }
    }

    (0..n)
        .map(|i| MeasureFit {
            index: i,
            new_start_sec: measures[i].start_sec + shift[i],
            confidence: confidence[i] as f32,
        })
        .collect()
}

/// Grid for `Project::auto_fit`: every tempo is refitted to the span between fitted starts (the
/// last measure keeps its tempo).
pub fn fitted_grid(measures: &[Measure], fits: &[MeasureFit]) -> Vec<FittedMeasure> {
    measures
        .iter()
        .zip(fits)
        .enumerate()
        .map(|(i, (m, fit))| {
            let refit = fits
                .get(i + 1)
                .map(|next| m.time_sig.beats() * 60.0 / (next.new_start_sec - fit.new_start_sec))
                .filter(|&bpm| valid_tempo(bpm));
            FittedMeasure {
                start_sec: fit.new_start_sec,
                tempo_bpm: refit.unwrap_or(m.tempo_bpm),
                time_sig: m.time_sig,
            }
        })
        .collect()
}
