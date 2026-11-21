//! Audio decoding and multi-track playback for FretScribe.

mod decode;
mod engine;
mod error;
mod focus;
mod metadata;
mod player;
mod ramp;
mod resample;
mod stretch;
mod synth;
pub mod wav;
#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/focus_tests.rs"]
mod focus_tests;
#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/mixer_tests.rs"]
mod mixer_tests;
#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/audio/testutil.rs"]
mod testutil;

pub use decode::{decode_file, DecodedAudio};
pub use engine::{SpeedMode, MAX_SPEED, MIN_SPEED};
pub use error::{AudioError, Result};
pub use focus::{focus_band, FocusBand};
pub use metadata::{read_tags, Tags};
pub use player::{Player, PreparedTrack};
pub use resample::PreparedAudio;
pub use synth::{NoteSynth, SynthNote};
