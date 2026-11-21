//! Multi-track helpers: there are several audio tracks but a single transcription.

use std::collections::HashMap;

use crate::edit::ProjectEdit;
use crate::error::EditError;
use crate::model::{Project, Track};
use crate::tempo::{NoteTimes, TempoMap};

impl Project {
    /// Assigns a fresh id (the track's own id is ignored) and returns it.
    pub fn add_track(&mut self, mut track: Track) -> u64 {
        track.id = self.tracks.iter().map(|t| t.id + 1).max().unwrap_or(1);
        let id = track.id;
        self.tracks.push(track);
        id
    }

    /// The last track cannot be removed; removing the source track hands that role to the first
    /// remaining track.
    pub fn remove_track(&mut self, track_id: u64) -> Result<Track, EditError> {
        let index = self.track_index(track_id)?;
        if self.tracks.len() == 1 {
            return Err(EditError::LastTrack);
        }
        let removed = self.tracks.remove(index);
        if self.source_track == track_id {
            self.source_track = self.tracks[0].id;
        }
        Ok(removed)
    }

    pub fn move_track(&mut self, track_id: u64, new_index: usize) -> Result<(), EditError> {
        let index = self.track_index(track_id)?;
        if new_index >= self.tracks.len() {
            return Err(EditError::TrackIndexOutOfRange {
                index: new_index,
                len: self.tracks.len(),
            });
        }
        let track = self.tracks.remove(index);
        self.tracks.insert(new_index, track);
        Ok(())
    }

    pub fn track(&self, track_id: u64) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == track_id)
    }

    fn track_index(&self, track_id: u64) -> Result<usize, EditError> {
        self.tracks
            .iter()
            .position(|t| t.id == track_id)
            .ok_or(EditError::UnknownTrack(track_id))
    }
}

pub fn next_note_id(project: &Project) -> u64 {
    project.notes.iter().map(|n| n.id + 1).max().unwrap_or(1)
}

/// If any track is soloed only the soloed ones play (muted or not); otherwise every unmuted track.
pub fn audible_tracks(project: &Project) -> Vec<u64> {
    let any_solo = project.tracks.iter().any(|t| t.solo);
    project
        .tracks
        .iter()
        .filter(|t| if any_solo { t.solo } else { !t.muted })
        .map(|t| t.id)
        .collect()
}

/// End of the latest track in project time; `durations` maps track id to audio length in seconds.
pub fn project_end_sec(project: &Project, durations: &HashMap<u64, f64>) -> f64 {
    project
        .tracks
        .iter()
        .filter_map(|t| durations.get(&t.id).map(|d| t.audio_to_project_time(*d)))
        .fold(0.0, f64::max)
}

/// Sets the offset of a track. With `move_notes`, and only for the source track, every note
/// (grace notes included) moves by the same delta so it stays on the audio it was transcribed from.
pub fn shift_track(
    project: &mut Project,
    track_id: u64,
    new_offset_sec: f64,
    move_notes: bool,
) -> Result<ProjectEdit, EditError> {
    if !new_offset_sec.is_finite() {
        return Err(EditError::InvalidTime(new_offset_sec));
    }
    project.edit_notes("Shift track", |p| {
        let index = p.track_index(track_id)?;
        let delta = new_offset_sec - p.tracks[index].offset_sec;
        p.tracks[index].offset_sec = new_offset_sec;
        if move_notes && track_id == p.source_track && delta != 0.0 && !p.measures.is_empty() {
            let map = TempoMap::new(&p.measures);
            let old = NoteTimes::build(&p.notes, &map);
            for note in &mut p.notes {
                if let (Some(start), Some(end)) = (old.start_sec(note.id), old.end_sec(note.id)) {
                    note.start = map.sec_to_pos(start + delta);
                    note.end = map.sec_to_pos(end + delta);
                }
            }
        }
        Ok(())
    })
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/tracks.rs"]
mod tests;
