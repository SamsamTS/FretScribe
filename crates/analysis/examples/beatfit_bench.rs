//! Mean beat error of a rough grid before/after fitting, on synthetic drifting click tracks.
//!
//! cargo run --release -p fretscribe-analysis --example beatfit_bench [-- <seeds>]

use std::time::Instant;

use fretscribe_analysis::beatfit::synth::*;
use fretscribe_analysis::beatfit::{estimate_tempo, fit_measures, fitted_grid, onset_envelope};

const BPM: f64 = 120.0;
const LEN_SEC: f64 = 33.0;

fn main() {
    let seeds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    println!("seed  before(ms)  after(ms)  tempo  fit(ms)");
    for seed in 1..=seeds {
        let beats = drifting_beats(64, BPM, 0.05, 0.02, seed);
        let audio = render_hits(&beats.iter().map(|&t| (t, 1000.0)).collect::<Vec<_>>(), LEN_SEC);
        let env = onset_envelope(&audio, SAMPLE_RATE);
        let grid = rough_grid(BPM, LEN_SEC);
        let start = Instant::now();
        let fits = fit_measures(&env, &grid, 0.25);
        let elapsed = start.elapsed().as_secs_f64() * 1e3;
        let mut fitted = grid.clone();
        for (m, f) in fitted.iter_mut().zip(fitted_grid(&grid, &fits)) {
            (m.start_sec, m.tempo_bpm) = (f.start_sec, f.tempo_bpm);
        }
        let tempo = estimate_tempo(&env, (60.0, 240.0)).first().map_or(0.0, |c| c.0);
        println!(
            "{seed:>4}  {:>10.1}  {:>9.1}  {tempo:>5.1}  {elapsed:>7.2}",
            mean_beat_error(&grid, &beats) * 1e3,
            mean_beat_error(&fitted, &beats) * 1e3,
        );
    }
}
