//! Audio decoding and multi-track playback for FretScribe.

mod decode;
mod engine;
mod error;
mod parallel;
mod player;
mod resample;
mod stretch;
mod synth;
#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod testutil;

pub use decode::{decode_file, decode_files, DecodedAudio};
pub use engine::{SpeedMode, MAX_SPEED, MIN_SPEED};
pub use error::{AudioError, Result};
pub use player::{Player, PreparedTrack, TrackAudio};
pub use resample::PreparedAudio;
pub use synth::{NoteSynth, SynthNote};
