use thiserror::Error;

use crate::model::TimeSignature;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed project file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("project file version {found} is newer than the supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("project file has no valid version field")]
    MissingVersion,
    #[error("migration from version {from} failed: {reason}")]
    Migration { from: u32, reason: String },
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("reference pitch {0} Hz is outside 400..=480 Hz")]
    PitchRefOutOfRange(f32),
}

#[derive(Debug, Error, PartialEq)]
pub enum EditError {
    #[error("measure index {index} is out of range (project has {len})")]
    MeasureOutOfRange { index: usize, len: usize },
    #[error("tempo {0} BPM is not valid")]
    InvalidTempo(f64),
    #[error("time signature {}/{} is not valid", .0.numerator, .0.denominator)]
    InvalidTimeSignature(TimeSignature),
    #[error("time {0} is not valid here")]
    InvalidTime(f64),
    #[error("track {0} does not exist")]
    UnknownTrack(u64),
    #[error("track position {index} is out of range (project has {len})")]
    TrackIndexOutOfRange { index: usize, len: usize },
    #[error("a project needs at least one track")]
    LastTrack,
    #[error("a project needs at least one measure")]
    LastMeasure,
    #[error("deleting this measure also deletes {notes} note(s); confirmation required")]
    NeedsConfirmation { notes: usize },
    #[error("the measure cannot be split at that beat")]
    InvalidSplit,
    #[error("the measures cannot be merged: their time signatures are incompatible")]
    IncompatibleTimeSignatures,
    #[error("the project has no measure grid")]
    NoGrid,
    #[error("note {0} does not exist")]
    UnknownNote(u64),
    #[error(transparent)]
    Link(#[from] crate::links::LinkError),
}
