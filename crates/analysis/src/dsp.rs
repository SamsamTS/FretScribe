//! Small signal-processing helpers shared by the analysis modules.

use std::f64::consts::TAU;

/// Periodic Hann window of `n` samples.
pub(crate) fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| (0.5 - 0.5 * (TAU * i as f64 / n as f64).cos()) as f32).collect()
}
