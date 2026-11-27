use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid palette: {0}")]
    InvalidPalette(String),
}
