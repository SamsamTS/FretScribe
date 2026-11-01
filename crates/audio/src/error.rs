use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot decode audio: {0}")]
    Decode(#[from] symphonia::core::errors::Error),
    #[error("the file contains no audio track")]
    NoAudioTrack,
    #[error("the file contains no audio samples")]
    Empty,
    #[error("unsupported audio stream: {0}")]
    Unsupported(String),
    #[error("resampling failed: {0}")]
    Resample(String),
    #[error("duplicate track id {0}")]
    DuplicateTrack(u64),
    #[error("no audio output device available")]
    NoOutputDevice,
    #[error("audio output error: {0}")]
    Output(String),
}

pub type Result<T> = std::result::Result<T, AudioError>;
