//! Audio analysis for FretScribe: the log-frequency spectrogram, its display pipeline, waveform peaks,
//! onset detection and beat-grid fitting.

pub mod beatfit;
mod cqt;
mod display;
mod error;
pub mod lut;
pub mod notes;
pub mod pitchref;
pub mod sync;
mod render;
mod spectrogram;
#[cfg(test)]
mod spectrogram_tests;
pub mod waveform;

pub use display::{
    apply_gamma, apply_levels, clamp_gamma, Levels, APP_GAMMA, APP_WHITE, GAMMA_DEFAULT, GAMMA_MAX,
    GAMMA_MIN, MIN_LEVELS_GAP,
};
pub use error::AnalysisError;
pub use lut::{discover_luts, Lut, LUT_SIZE};
pub use spectrogram::{
    compute_spectrogram, compute_spectrogram_streaming, Spectrogram, SpectrogramChunk,
    SpectrogramInfo, SpectrogramParams, SpectrogramStream, WindowKind,
};
pub use waveform::{render_rgba as render_waveform_rgba, WaveformPyramid, WAVEFORM_GAMMA};
