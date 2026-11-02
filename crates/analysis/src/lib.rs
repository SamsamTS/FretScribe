//! Audio analysis for FretScribe: the log-frequency spectrogram and its display pipeline.

mod cqt;
mod display;
mod error;
pub mod lut;
mod render;
mod spectrogram;
#[cfg(test)]
mod spectrogram_tests;

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
