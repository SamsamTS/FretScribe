//! What the audio player must be told, as plain data: testable without opening an audio device.

use std::mem::{discriminant, Discriminant};

use fretscribe_audio::{Player, SpeedMode};
use fretscribe_core::{NoteTimes, PitchRef, Project, TempoMap, Tuning, UiState};

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerCall {
    FocusContext { tuning: Tuning, capo: u8 },
    ReferencePitch(PitchRef),
    GuitarFocus(bool),
    GuitarFocusStrength(f32),
    Region(Option<(f64, f64)>),
    Loop(bool),
    Speed(f32),
    SpeedMode(SpeedMode),
    TrackVolume { id: u64, volume: f32 },
    TrackMute { id: u64, muted: bool },
    TrackSolo { id: u64, solo: bool },
    TrackOffset { id: u64, offset_sec: f64 },
    NoteOverlay { notes: Vec<(u8, f64, f64)>, mix: f32 },
}

impl PlayerCall {
    fn key(&self) -> (Discriminant<Self>, u64) {
        let id = match self {
            Self::TrackVolume { id, .. }
            | Self::TrackMute { id, .. }
            | Self::TrackSolo { id, .. }
            | Self::TrackOffset { id, .. } => *id,
            _ => 0,
        };
        (discriminant(self), id)
    }
}

pub fn full_sync(
    project: &Project,
    ui: &UiState,
    speed: f32,
    keep_pitch: bool,
    overlay_mix: f32,
) -> Vec<PlayerCall> {
    use PlayerCall as C;
    let mut calls = vec![
        C::FocusContext { tuning: project.tuning.clone(), capo: project.capo },
        C::ReferencePitch(project.pitch_ref()),
        C::GuitarFocus(ui.focus_enabled),
        C::GuitarFocusStrength(ui.focus_strength),
        C::Region(ui.selected_region),
        C::Loop(ui.loop_enabled),
        C::Speed(speed),
        C::SpeedMode(if keep_pitch { SpeedMode::PreservePitch } else { SpeedMode::ChangePitch }),
    ];
    for t in &project.tracks {
        let id = t.id;
        calls.extend([
            C::TrackVolume { id, volume: t.volume },
            C::TrackMute { id, muted: t.muted },
            C::TrackSolo { id, solo: t.solo },
            C::TrackOffset { id, offset_sec: t.offset_sec },
        ]);
    }
    let notes = if overlay_mix > 0.0 {
        let times = NoteTimes::build(&project.notes, &TempoMap::new(&project.measures));
        project
            .notes
            .iter()
            .filter_map(|n| Some((n.midi, times.start_sec(n.id)?, times.end_sec(n.id)?)))
            .collect()
    } else {
        Vec::new()
    };
    calls.push(C::NoteOverlay { notes, mix: overlay_mix });
    calls
}

/// The calls of `next` that differ from the call with the same variant and track in `prev`.
pub fn diff(prev: &[PlayerCall], next: &[PlayerCall]) -> Vec<PlayerCall> {
    next.iter()
        .filter(|n| prev.iter().find(|p| p.key() == n.key()) != Some(n))
        .cloned()
        .collect()
}

pub fn apply(player: &Player, calls: &[PlayerCall]) {
    use PlayerCall as C;
    for call in calls {
        match call {
            C::FocusContext { tuning, capo } => player.set_focus_context(tuning, *capo),
            C::ReferencePitch(p) => player.set_reference_pitch(*p),
            C::GuitarFocus(on) => player.set_guitar_focus(*on),
            C::GuitarFocusStrength(s) => player.set_guitar_focus_strength(*s),
            C::Region(r) => player.set_region(*r),
            C::Loop(on) => player.set_loop(*on),
            C::Speed(s) => player.set_playback_speed(*s),
            C::SpeedMode(m) => player.set_speed_mode(*m),
            C::TrackVolume { id, volume } => player.set_track_volume(*id, *volume),
            C::TrackMute { id, muted } => player.set_track_mute(*id, *muted),
            C::TrackSolo { id, solo } => player.set_track_solo(*id, *solo),
            C::TrackOffset { id, offset_sec } => player.set_track_offset(*id, *offset_sec),
            C::NoteOverlay { notes, mix } => player.set_note_overlay(notes.clone(), *mix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{Note, TimePos, TimeSignature, Track, TrackRole};

    fn project() -> Project {
        let mut p = Project::new(Track::new(1, "a", TrackRole::Guitar), 10.0, 120.0, TimeSignature::COMMON);
        let mut b = Track::new(2, "b", TrackRole::Bass);
        b.muted = true;
        b.offset_sec = 0.5;
        p.add_track(b);
        p.tracks[0].solo = true;
        p.notes.push(Note::new(1, 64, TimePos::new(p.measures[0].id, 0.0), TimePos::new(p.measures[0].id, 1.0)));
        p
    }

    fn sync(p: &Project, mix: f32) -> Vec<PlayerCall> {
        full_sync(p, &UiState::default(), 1.0, true, mix)
    }

    #[test]
    fn full_sync_covers_tracks_and_overlay() {
        let p = project();
        let calls = sync(&p, 0.5);
        assert!(calls.contains(&PlayerCall::TrackSolo { id: 1, solo: true }));
        assert!(calls.contains(&PlayerCall::TrackMute { id: 2, muted: true }));
        assert!(calls.contains(&PlayerCall::TrackOffset { id: 2, offset_sec: 0.5 }));
        assert!(calls.contains(&PlayerCall::SpeedMode(SpeedMode::PreservePitch)));
        let overlay = calls.iter().find_map(|c| match c {
            PlayerCall::NoteOverlay { notes, .. } => Some(notes.clone()),
            _ => None,
        });
        assert_eq!(overlay, Some(vec![(64, 0.0, 0.5)]));
        assert!(sync(&p, 0.0).contains(&PlayerCall::NoteOverlay { notes: vec![], mix: 0.0 }));
    }

    #[test]
    fn diff_is_minimal() {
        let mut p = project();
        let before = sync(&p, 0.0);
        assert!(diff(&before, &before).is_empty());

        p.tuning = notation_drop_d();
        assert!(matches!(diff(&before, &sync(&p, 0.0))[..], [PlayerCall::FocusContext { .. }]));

        let mut q = project();
        q.reference_hz = 442.0;
        assert_eq!(diff(&before, &sync(&q, 0.0)), [PlayerCall::ReferencePitch(q.pitch_ref())]);

        let mut r = project();
        r.tracks[1].volume = 0.5;
        assert_eq!(diff(&before, &sync(&r, 0.0)), [PlayerCall::TrackVolume { id: 2, volume: 0.5 }]);

        assert_eq!(diff(&[], &before), before);
    }

    fn notation_drop_d() -> Tuning {
        Tuning { name: "Drop D".into(), open_strings: vec![38, 45, 50, 55, 59, 64] }
    }
}
