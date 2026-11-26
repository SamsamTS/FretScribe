use super::*;
use fretscribe_core::{Note, TimePos, TimeSignature, Track, TrackRole};

fn project() -> Project {
    let mut p = Project::new(Track::new(1, "a", TrackRole::Guitar), 10.0, 120.0, TimeSignature::COMMON);
    let mut b = Track::new(2, "b", TrackRole::Bass);
    b.muted = true;
    b.offset_sec = 0.5;
    p.add_track(b);
    p.tracks[0].solo = true;
    p.notes.push(Note::new(1, 64, TimePos::at(p.measures[0].id, 0.0), TimePos::at(p.measures[0].id, 1.0)));
    p
}

fn sync(p: &Project, mix: f32) -> Vec<PlayerCall> {
    full_sync(p, &UiState::default(), 1.0, true, mix, &fretscribe_core::perform::performance(p).into())
}

#[test]
fn full_sync_covers_tracks_and_overlay() {
    let p = project();
    let calls = sync(&p, 0.5);
    assert!(calls.contains(&PlayerCall::TrackSolo { id: 1, solo: true }));
    assert!(calls.contains(&PlayerCall::TrackMute { id: 2, muted: true }));
    assert!(calls.contains(&PlayerCall::TrackOffset { id: 2, offset_sec: 0.5 }));
    assert!(calls.contains(&PlayerCall::SpeedMode(SpeedMode::PreservePitch)));
    assert_eq!(synth_notes(&calls), [(64, 0.0, 0.5)]);
    assert!(synth_notes(&sync(&p, 0.0)).is_empty());
}

fn synth_notes(calls: &[PlayerCall]) -> Vec<(u8, f64, f64)> {
    let notes = calls.iter().find_map(|c| match c {
        PlayerCall::Performance { notes, .. } => Some(notes),
        _ => None,
    });
    notes.unwrap().iter().map(|n| (n.midi, n.start_sec, n.end_sec)).collect()
}

#[test]
fn project_state_becomes_preset_reference_and_performance_calls() {
    let mut p = project();
    p.reference_hz = 432.0;
    let mut ui = UiState { synth_preset: "Distortion".into(), ..UiState::default() };
    let played = fretscribe_core::perform::performance(&p).into();
    let calls = full_sync(&p, &ui, 1.0, true, 0.5, &played);
    assert!(calls.contains(&PlayerCall::SynthPreset(SynthPreset::Distortion)));
    assert!(calls.contains(&PlayerCall::ReferencePitch(p.pitch_ref())));
    assert!(calls.contains(&PlayerCall::Performance { notes: played.clone(), mix: 0.5 }));
    ui.synth_preset = "From the future".into();
    let next = full_sync(&p, &ui, 1.0, true, 0.5, &played);
    assert_eq!(diff(&calls, &next), [PlayerCall::SynthPreset(SynthPreset::Acoustic)]);
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
