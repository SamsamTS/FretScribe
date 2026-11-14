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
    project.transact("Shift track", |p| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measures::tests::close;
    use crate::model::{
        GraceKind, GraceNote, Legato, LegatoLink, Note, Technique, TimePos, TimeSignature,
        TrackRole,
    };

    fn project() -> Project {
        let mut p = Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            16.0,
            120.0,
            TimeSignature::COMMON,
        );
        p.add_track(Track::new(0, "Drums", TrackRole::Drums));
        p.add_track(Track::new(0, "Bass", TrackRole::Bass));
        p
    }

    fn add_note(p: &mut Project, start: (u64, f64), end: (u64, f64)) -> u64 {
        let id = next_note_id(p);
        p.notes.push(Note::new(
            id,
            60,
            TimePos::new(start.0, start.1),
            TimePos::new(end.0, end.1),
        ));
        id
    }

    fn secs(p: &Project, id: u64) -> (f64, f64) {
        let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
        (times.start_sec(id).unwrap(), times.end_sec(id).unwrap())
    }

    #[test]
    fn add_track_assigns_unique_ids() {
        let mut p = project();
        assert_eq!(p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
        let id = p.add_track(Track::new(1, "Click", TrackRole::Other));
        assert_eq!(id, 4);
        p.remove_track(2).unwrap();
        let id = p.add_track(Track::new(1, "Click 2", TrackRole::Other));
        assert_eq!(id, 5);
        p.validate().unwrap();
    }

    #[test]
    fn remove_track_rules() {
        let mut p = project();
        assert_eq!(p.remove_track(9).unwrap_err(), EditError::UnknownTrack(9));
        let removed = p.remove_track(1).unwrap();
        assert_eq!(removed.name, "Guitar");
        assert_eq!(p.source_track, 2);
        p.remove_track(2).unwrap();
        assert_eq!(p.remove_track(3).unwrap_err(), EditError::LastTrack);
        assert_eq!(p.tracks.len(), 1);
    }

    #[test]
    fn removing_a_helper_track_keeps_the_source() {
        let mut p = project();
        p.remove_track(3).unwrap();
        assert_eq!(p.source_track, 1);
    }

    #[test]
    fn move_track_reorders() {
        let mut p = project();
        p.move_track(3, 0).unwrap();
        assert_eq!(p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [3, 1, 2]);
        p.move_track(3, 2).unwrap();
        assert_eq!(p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(p.move_track(1, 3).is_err());
        assert!(p.move_track(8, 0).is_err());
    }

    #[test]
    fn note_ids_are_unique_across_the_project() {
        let mut p = project();
        let ids: Vec<u64> = (0..5)
            .map(|_| add_note(&mut p, (1, 0.0), (1, 1.0)))
            .collect();
        assert_eq!(ids, [1, 2, 3, 4, 5]);
        assert_eq!(next_note_id(&p), 6);
        p.validate().unwrap();
    }

    #[test]
    fn nothing_soloed_respects_mute() {
        let mut p = project();
        p.tracks[1].muted = true;
        assert_eq!(audible_tracks(&p), [1, 3]);
        p.tracks[0].muted = true;
        p.tracks[2].muted = true;
        assert!(audible_tracks(&p).is_empty());
    }

    #[test]
    fn solo_overrides_mute() {
        let mut p = project();
        p.tracks[1].muted = true;
        p.tracks[1].solo = true;
        assert_eq!(audible_tracks(&p), [2]);
        p.tracks[2].solo = true;
        assert_eq!(audible_tracks(&p), [2, 3]);
        p.tracks[0].muted = true;
        assert_eq!(audible_tracks(&p), [2, 3]);
    }

    #[test]
    fn offsets_convert_both_ways_including_negative() {
        let mut t = Track::new(1, "x", TrackRole::Guitar);
        t.offset_sec = 0.25;
        close(t.audio_to_project_time(1.0), 1.25);
        close(t.project_to_audio_time(1.25), 1.0);
        t.offset_sec = -0.5;
        close(t.audio_to_project_time(1.0), 0.5);
        close(t.project_to_audio_time(0.25), 0.75);
        close(t.project_to_audio_time(t.audio_to_project_time(3.3)), 3.3);
    }

    #[test]
    fn project_end_takes_offsets_into_account() {
        let mut p = project();
        p.tracks[1].offset_sec = 2.0;
        p.tracks[2].offset_sec = -1.0;
        let durations = HashMap::from([(1, 10.0), (2, 9.0), (3, 12.0)]);
        close(project_end_sec(&p, &durations), 11.0);
        close(project_end_sec(&p, &HashMap::new()), 0.0);
        let only_first = HashMap::from([(1, 10.0)]);
        close(project_end_sec(&p, &only_first), 10.0);
    }

    #[test]
    fn volume_is_clamped() {
        let mut t = Track::new(1, "x", TrackRole::Guitar);
        t.set_volume(9.0);
        assert_eq!(t.volume, 1.5);
        t.set_volume(-1.0);
        assert_eq!(t.volume, 0.0);
        t.set_volume(f32::NAN);
        assert_eq!(t.volume, 1.0);
    }

    #[test]
    fn shifting_the_source_track_moves_every_note_including_grace_notes() {
        let mut p = project();
        let main = add_note(&mut p, (2, 1.0), (2, 2.0));
        let grace = add_note(&mut p, (2, 0.9), (2, 1.0));
        p.notes[1].grace_of = Some(GraceNote {
            main,
            kind: GraceKind::Acciaccatura,
        });
        let before = [secs(&p, main), secs(&p, grace)];
        let edit = shift_track(&mut p, 1, 0.3, true).unwrap();
        close(p.tracks[0].offset_sec, 0.3);
        for (id, (s, e)) in [main, grace].into_iter().zip(before) {
            let (s2, e2) = secs(&p, id);
            close(s2, s + 0.3);
            close(e2, e + 0.3);
        }
        let shifted = p.clone();
        edit.undo(&mut p);
        close(p.tracks[0].offset_sec, 0.0);
        close(secs(&p, main).0, before[0].0);
        edit.redo(&mut p);
        assert_eq!(p, shifted);
    }

    #[test]
    fn shifting_by_a_negative_delta_and_across_measures() {
        let mut p = project();
        let n = add_note(&mut p, (2, 3.0), (3, 1.0));
        let (s, e) = secs(&p, n);
        shift_track(&mut p, 1, -1.2, true).unwrap();
        let (s2, e2) = secs(&p, n);
        close(s2, s - 1.2);
        close(e2, e - 1.2);
        shift_track(&mut p, 1, 0.0, true).unwrap();
        close(secs(&p, n).0, s);
        close(secs(&p, n).1, e);
        p.validate().unwrap();
    }

    #[test]
    fn shift_is_relative_to_the_previous_offset() {
        let mut p = project();
        p.tracks[0].offset_sec = 1.0;
        let n = add_note(&mut p, (2, 0.0), (2, 1.0));
        let s = secs(&p, n).0;
        shift_track(&mut p, 1, 1.5, true).unwrap();
        close(secs(&p, n).0, s + 0.5);
    }

    #[test]
    fn notes_can_move_before_the_first_measure() {
        let mut p = project();
        let n = add_note(&mut p, (1, 0.0), (1, 1.0));
        shift_track(&mut p, 1, -1.0, true).unwrap();
        close(secs(&p, n).0, -1.0);
        assert!(p.note(n).unwrap().start.beat < 0.0);
    }

    #[test]
    fn helper_tracks_never_move_notes() {
        let mut p = project();
        let n = add_note(&mut p, (2, 1.0), (2, 2.0));
        let before = p.notes.clone();
        shift_track(&mut p, 2, 0.7, true).unwrap();
        close(p.tracks[1].offset_sec, 0.7);
        assert_eq!(p.notes, before);
        assert!(p.note(n).is_some());
    }

    #[test]
    fn move_notes_false_only_changes_the_offset() {
        let mut p = project();
        add_note(&mut p, (2, 1.0), (2, 2.0));
        let before = p.notes.clone();
        shift_track(&mut p, 1, 0.7, false).unwrap();
        close(p.tracks[0].offset_sec, 0.7);
        assert_eq!(p.notes, before);
    }

    #[test]
    fn shift_rejects_bad_input_without_side_effects() {
        let mut p = project();
        let before = p.clone();
        assert_eq!(
            shift_track(&mut p, 9, 1.0, true).unwrap_err(),
            EditError::UnknownTrack(9)
        );
        assert!(shift_track(&mut p, 1, f64::NAN, true).is_err());
        assert!(shift_track(&mut p, 1, f64::INFINITY, true).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn three_track_project_round_trips_through_json() {
        let mut p = project();
        p.tracks[0].offset_sec = -0.125;
        p.tracks[1].offset_sec = 0.5;
        p.tracks[1].muted = true;
        p.tracks[2].solo = true;
        p.tracks[2].audio_path = Some("bass.flac".into());
        p.tracks[0].set_volume(1.25);
        p.capo = 2;
        p.reference_hz = 432.0;

        let a = add_note(&mut p, (1, 0.0), (1, 1.0));
        let b = add_note(&mut p, (1, 1.0), (2, 0.5));
        let g = add_note(&mut p, (2, 1.9), (2, 2.0));
        let c = add_note(&mut p, (2, 2.0), (2, 3.0));
        p.notes[1].technique = Some(Technique::Slide);
        p.notes[1].link_to = Some(c);
        p.notes[1].string_fret = Some((2, 7));
        p.notes[1].locked_fret = true;
        p.notes[2].grace_of = Some(GraceNote {
            main: c,
            kind: GraceKind::Appoggiatura,
        });
        p.notes[3].legato = Some(LegatoLink {
            kind: Legato::PullOff,
            from: g,
        });
        p.notes[0].technique = Some(Technique::PalmMute);
        assert!(p.note(a).is_some() && p.note(b).is_some());

        let json = serde_json::to_string_pretty(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        assert_eq!(
            back.notes[3].legato,
            Some(LegatoLink {
                kind: Legato::PullOff,
                from: g
            })
        );
        assert_eq!(
            back.notes[2].grace_of,
            Some(GraceNote {
                main: c,
                kind: GraceKind::Appoggiatura
            })
        );
        back.validate().unwrap();
    }
}
