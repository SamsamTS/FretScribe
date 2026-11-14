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
    pub spectrogram_ready: bool,
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use fretscribe_audio::DecodedAudio;
    use fretscribe_core::{TimeSignature, Track, TrackRole};

    pub fn data(onsets: Vec<f64>, seconds: f64) -> TrackData {
        let rate = 8000;
        let mono: Arc<[f32]> = vec![0.5; (seconds * f64::from(rate)) as usize].into();
        let decoded = DecodedAudio::from_interleaved(rate, 1, mono.to_vec()).unwrap();
        TrackData {
            prepared: PreparedAudio::new(&decoded, 48_000).unwrap(),
            pyramid: Arc::new(WaveformPyramid::build(&mono, rate)),
            mono,
            sample_rate: rate,
            duration_sec: seconds,
            spectrogram: Spectrogram::empty(&fretscribe_analysis::SpectrogramInfo {
                n_frames: 2,
                n_rows: 2,
                hop_sec: 0.01,
                min_midi: 40.0,
                max_midi: 41.0,
                bins_per_semitone: 1,
            }),
            spectrogram_ready: false,
            envelope: OnsetEnvelope { values: vec![0.0, 1.0], hop_sec: 0.01 },
            onsets,
        }
    }

    fn project() -> Project {
        let mut p = Project::new(Track::new(1, "G", TrackRole::Guitar), 4.0, 120.0, TimeSignature::COMMON);
        p.add_track(Track::new(0, "D", TrackRole::Drums));
        p.add_track(Track::new(0, "B", TrackRole::Bass));
        p
    }

    #[test]
    fn prepared_and_inputs_follow_project_order_and_skip_missing() {
        let mut p = project();
        let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
        p.tracks[1].offset_sec = 0.25;
        p.tracks[1].muted = true;
        let mut s = AudioStore::default();
        s.tracks.insert(ids[1], data(vec![], 1.0));
        s.tracks.insert(ids[0], data(vec![], 1.0));
        let prepared = s.prepared_tracks(&p);
        assert_eq!(prepared.iter().map(|t| t.id).collect::<Vec<_>>(), ids[..2]);
        assert!(prepared[1].muted && prepared[1].offset_sec == 0.25);
        // Guitar is excluded from beats by default; the drums are in.
        assert_eq!(s.onset_inputs(&p).len(), 1);
        assert_eq!(s.shared_audio_end(&p), 1.0);
        s.remove(ids[1]);
        assert_eq!(s.prepared_tracks(&p).len(), 1);
    }

    #[test]
    fn beat_times_and_snapping() {
        let p = project();
        let mut s = AudioStore::default();
        s.set_beat_times_from(&p);
        assert!(s.beat_times.len() >= 4 && s.beat_times.windows(2).all(|w| w[1] > w[0]));
        s.tracks.insert(1, data(vec![0.1], 4.0));
        s.tracks.insert(2, data(vec![0.2], 4.0));
        s.reference = (1, 0.0);
        let c = s.snap_candidates(2, 0.28, 0.1);
        assert!(c.iter().any(|o| (o - 0.3).abs() < 1e-9), "{c:?}");
        assert_eq!(s.peaks(2, (0.0, 1.0), 10).len(), 10);
        assert_eq!(s.peaks(99, (0.0, 1.0), 10).len(), 10);
        assert_eq!(s.duration_sec(2), 4.0);
    }
}
