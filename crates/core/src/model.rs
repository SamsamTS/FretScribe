use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::measures::generate_measures;
use crate::pitch::PitchRef;

pub const MIN_BPM: f64 = 10.0;
pub const MAX_BPM: f64 = 500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const COMMON: Self = Self::new(4, 4);

    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub fn is_valid(self) -> bool {
        (1..=64).contains(&self.numerator) && matches!(self.denominator, 1 | 2 | 4 | 8 | 16 | 32 | 64)
    }

    pub fn beats(self) -> f64 {
        f64::from(self.numerator) * 4.0 / f64::from(self.denominator)
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub id: u64,
    pub start_sec: f64,
    pub tempo_bpm: f64,
    pub time_sig: TimeSignature,
    #[serde(default)]
    pub anchored: bool,
}

impl Measure {
    pub fn new(id: u64, start_sec: f64, tempo_bpm: f64, time_sig: TimeSignature) -> Self {
        Self {
            id,
            start_sec,
            tempo_bpm,
            time_sig,
            anchored: false,
        }
    }

    pub fn nominal_duration_sec(&self) -> f64 {
        self.time_sig.beats() * 60.0 / self.tempo_bpm
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuning {
    pub name: String,
    pub open_strings: Vec<u8>,
}

impl Tuning {
    pub fn standard() -> Self {
        Self {
            name: "Standard".to_owned(),
            open_strings: vec![40, 45, 50, 55, 59, 64],
        }
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self::standard()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimePos {
    pub measure: u64,
    pub beat: f64,
}

impl TimePos {
    pub const fn new(measure: u64, beat: f64) -> Self {
        Self { measure, beat }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    Slide,
    Bend,
    Harmonic,
    PalmMute,
    Tie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Legato {
    HammerOn,
    PullOff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegatoLink {
    pub kind: Legato,
    pub from: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraceKind {
    #[default]
    Acciaccatura,
    Appoggiatura,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraceNote {
    pub main: u64,
    pub kind: GraceKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    pub midi: u8,
    pub start: TimePos,
    pub end: TimePos,
    pub string_fret: Option<(u8, u8)>,
    pub locked_fret: bool,
    pub technique: Option<Technique>,
    pub link_to: Option<u64>,
    pub legato: Option<LegatoLink>,
    pub grace_of: Option<GraceNote>,
}

impl Note {
    pub fn new(id: u64, midi: u8, start: TimePos, end: TimePos) -> Self {
        Self {
            id,
            midi,
            start,
            end,
            string_fret: None,
            locked_fret: false,
            technique: None,
            link_to: None,
            legato: None,
            grace_of: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackRole {
    Guitar,
    Bass,
    Drums,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    pub role: TrackRole,
    pub audio_path: Option<PathBuf>,
    pub offset_sec: f64,
    pub volume: f32,
    pub muted: bool,
    pub solo: bool,
    pub spectrogram_collapsed: bool,
    pub use_for_beats: bool,
}

impl Track {
    pub const MAX_VOLUME: f32 = 1.5;

    pub fn new(id: u64, name: impl Into<String>, role: TrackRole) -> Self {
        let color = match role {
            TrackRole::Guitar => [240, 170, 60],
            TrackRole::Bass => [90, 170, 240],
            TrackRole::Drums => [230, 90, 110],
            TrackRole::Other => [150, 210, 140],
        };
        Self {
            id,
            name: name.into(),
            color,
            role,
            audio_path: None,
            offset_sec: 0.0,
            volume: 1.0,
            muted: false,
            solo: false,
            spectrogram_collapsed: false,
            use_for_beats: matches!(role, TrackRole::Drums | TrackRole::Bass),
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, Self::MAX_VOLUME)
        } else {
            1.0
        };
    }

    pub fn audio_to_project_time(&self, audio_sec: f64) -> f64 {
        audio_sec + self.offset_sec
    }

    pub fn project_to_audio_time(&self, project_sec: f64) -> f64 {
        project_sec - self.offset_sec
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub tracks: Vec<Track>,
    pub source_track: u64,
    pub measures: Vec<Measure>,
    pub next_measure_id: u64,
    pub tuning: Tuning,
    pub capo: u8,
    pub reference_hz: f32,
    pub notes: Vec<Note>,
}

impl Project {
    pub fn new(
        first_track: Track,
        audio_len_sec: f64,
        bpm: f64,
        time_sig: TimeSignature,
    ) -> Self {
        let mut next_measure_id = 1;
        let measures = generate_measures(0.0, audio_len_sec, bpm, time_sig, &mut next_measure_id);
        Self {
            source_track: first_track.id,
            tracks: vec![first_track],
            measures,
            next_measure_id,
            tuning: Tuning::standard(),
            capo: 0,
            reference_hz: PitchRef::default().a4_hz(),
            notes: Vec::new(),
        }
    }

    pub fn pitch_ref(&self) -> PitchRef {
        PitchRef::new(self.reference_hz).unwrap_or_default()
    }

    pub fn note(&self, id: u64) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub(crate) fn fresh_measure_id(&mut self) -> u64 {
        self.repair_counters();
        let id = self.next_measure_id;
        self.next_measure_id += 1;
        id
    }

    /// Restores the id-counter invariant after loading data written by an older or hand-edited file.
    pub(crate) fn repair_counters(&mut self) {
        let floor = self.measures.iter().map(|m| m.id + 1).max().unwrap_or(1);
        self.next_measure_id = self.next_measure_id.max(floor).max(1);
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        let invalid = |msg: String| Err(CoreError::Invalid(msg));
        if self.tracks.is_empty() {
            return invalid("no audio track".into());
        }
        if !self.tracks.iter().any(|t| t.id == self.source_track) {
            return invalid(format!("source track {} does not exist", self.source_track));
        }
        PitchRef::new(self.reference_hz)?;

        let mut seen = HashSet::new();
        if !self.tracks.iter().all(|t| seen.insert(t.id)) {
            return invalid("duplicate track id".into());
        }
        let mut measure_ids = HashSet::new();
        for m in &self.measures {
            if !measure_ids.insert(m.id) {
                return invalid(format!("duplicate measure id {}", m.id));
            }
            if m.id >= self.next_measure_id {
                return invalid(format!("measure id {} is not below next_measure_id", m.id));
            }
            if !m.time_sig.is_valid() || !(MIN_BPM..=MAX_BPM).contains(&m.tempo_bpm) {
                return invalid(format!("measure {} has an invalid tempo or time signature", m.id));
            }
        }
        let mut note_ids = HashSet::new();
        for n in &self.notes {
            if !note_ids.insert(n.id) {
                return invalid(format!("duplicate note id {}", n.id));
            }
            if !measure_ids.contains(&n.start.measure) || !measure_ids.contains(&n.end.measure) {
                return invalid(format!("note {} refers to a missing measure", n.id));
            }
        }
        Ok(())
    }
}
