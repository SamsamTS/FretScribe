//! Per-track audio-side data (everything derived from the decoded file), keyed by track id.

use std::collections::HashMap;
use std::sync::Arc;

use fretscribe_analysis::beatfit::{envelope_input, EnvelopeInput, OnsetEnvelope};
use fretscribe_analysis::sync::snap_offsets;
use fretscribe_analysis::{Spectrogram, WaveformPyramid};
use fretscribe_audio::{PreparedAudio, PreparedTrack};
use fretscribe_core::{Project, TempoMap};
use fretscribe_ui::spectro::Peak;
use fretscribe_ui::LaneSource;

pub struct TrackData {
    pub prepared: PreparedAudio,
    /// Mono mixdown at the file's native rate.
    pub mono: Arc<[f32]>,
    pub sample_rate: u32,
    pub duration_sec: f64,
    pub spectrogram: Spectrogram,
    pub pyramid: Arc<WaveformPyramid>,
    pub envelope: OnsetEnvelope,
    pub onsets: Vec<f64>,
}

#[derive(Default)]
pub struct AudioStore {
    pub tracks: HashMap<u64, TrackData>,
    /// Bumped on every in-place spectrogram change; feed it to the spectrogram cache.
    pub revision: u64,
    pub beat_times: Vec<f64>,
    /// `(track id, offset_sec)` of the reference track, whose onsets are snap targets.
    pub reference: (u64, f64),
}

impl AudioStore {
    pub fn remove(&mut self, id: u64) {
        self.tracks.remove(&id);
        self.revision += 1;
    }

    /// Project track order; tracks without data are skipped.
    pub fn prepared_tracks(&self, project: &Project) -> Vec<PreparedTrack> {
        project
            .tracks
            .iter()
            .filter_map(|t| {
                let data = self.tracks.get(&t.id)?;
                Some(PreparedTrack {
                    id: t.id,
                    audio: data.prepared.clone(),
                    offset_sec: t.offset_sec,
                    volume: t.volume,
                    muted: t.muted,
                    solo: t.solo,
                })
            })
            .collect()
    }

    pub fn set_beat_times_from(&mut self, project: &Project) {
        let map = TempoMap::new(&project.measures);
        self.beat_times =
            (0..map.len()).flat_map(|i| map.beat_lines(i)).map(|l| l.time_sec).collect();
    }

    /// Only tracks with `use_for_beats`.
    pub fn onset_inputs(&self, project: &Project) -> Vec<EnvelopeInput> {
        project
            .tracks
            .iter()
            .filter_map(|t| envelope_input(t, self.tracks.get(&t.id)?.envelope.clone()))
            .collect()
    }

    /// End of the audio all tracks share, in project time (shifted by each offset); 0 without tracks.
    pub fn shared_audio_end(&self, project: &Project) -> f64 {
        project
            .tracks
            .iter()
            .filter_map(|t| Some(self.tracks.get(&t.id)?.duration_sec + t.offset_sec))
            .reduce(f64::min)
            .unwrap_or(0.0)
    }
}

impl LaneSource for AudioStore {
    fn spectrogram(&self, id: u64) -> Option<&Spectrogram> {
        self.tracks.get(&id).map(|t| &t.spectrogram)
    }

    fn spectrogram_revision(&self, _: u64) -> u64 {
        self.revision
    }

    fn peaks(&self, id: u64, range: (f64, f64), columns: usize) -> Vec<Peak> {
        match self.tracks.get(&id) {
            Some(t) => t.pyramid.peaks(range, columns),
            None => vec![(0.0, 0.0, 0.0); columns],
        }
    }

    fn duration_sec(&self, id: u64) -> f64 {
        self.tracks.get(&id).map_or(0.0, |t| t.duration_sec)
    }

    fn onsets(&self, id: u64) -> &[f64] {
        self.tracks.get(&id).map_or(&[], |t| &t.onsets)
    }

    fn snap_candidates(&self, id: u64, raw_offset_sec: f64, max_dist_sec: f64) -> Vec<f64> {
        let mut targets = self.beat_times.clone();
        if id != self.reference.0 {
            let shift = self.reference.1;
            targets.extend(self.onsets(self.reference.0).iter().map(|o| o + shift));
        }
        snap_offsets(self.onsets(id), &targets, raw_offset_sec, max_dist_sec)
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/store.rs"]
pub(crate) mod tests;
