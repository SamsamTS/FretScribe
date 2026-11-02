use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("cannot read image: {0}")]
    Image(#[from] image::ImageError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid palette: {0}")]
    InvalidPalette(String),
}
