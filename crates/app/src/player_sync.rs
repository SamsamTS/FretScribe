//! What the audio player must be told, as plain data: testable without opening an audio device.

use std::mem::{discriminant, Discriminant};
use std::sync::Arc;

use fretscribe_audio::{Player, SpeedMode, SynthPreset};
use fretscribe_core::perform::PlayedNote;
use fretscribe_core::{PitchRef, Project, Tuning, UiState};

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
    SynthPreset(SynthPreset),
    Performance { notes: Arc<[PlayedNote]>, mix: f32 },
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
    performance: &Arc<[PlayedNote]>,
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
    let notes = if overlay_mix > 0.0 { performance.clone() } else { Arc::default() };
    calls.extend([C::SynthPreset(SynthPreset::from_name(&ui.synth_preset)), C::Performance { notes, mix: overlay_mix }]);
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
            C::SynthPreset(p) => player.set_synth_preset(*p),
            C::Performance { notes, mix } => player.set_performance(notes.to_vec(), *mix),
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/player_sync.rs"]
mod tests;
