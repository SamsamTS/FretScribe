//! Writes the same phrase once per synth preset, for tuning by ear:
//! `cargo run --release -p fretscribe-demos --example synth_presets [out_dir]`

use std::path::PathBuf;

use fretscribe_audio::{wav, NoteSynth, SynthPreset};
use fretscribe_core::perform::performance;
use fretscribe_core::{Note, PitchRef, Project, Technique, TimePos, TimeSignature, Track, TrackRole};

const RATE: u32 = 48_000;
const SECONDS: f64 = 10.0;

/// `(midi, start beat, length in beats)`: low and high single notes, a chord, a bend, palm mutes.
#[rustfmt::skip]
const PHRASE: &[(u8, f64, f64)] = &[
    (40, 0.0, 2.0), (76, 2.0, 2.0),
    (40, 4.0, 4.0), (47, 4.0, 4.0), (52, 4.0, 4.0), (56, 4.0, 4.0),
    (67, 8.0, 1.0), (69, 9.0, 3.0),
    (40, 12.0, 0.5), (40, 12.5, 0.5), (40, 13.0, 0.5), (40, 13.5, 0.5), (47, 14.0, 0.5), (45, 14.5, 0.5),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let mut project = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), SECONDS, 120.0, TimeSignature::COMMON);
    let measure = project.measures[0].id;
    for (id, &(midi, start, len)) in (1..).zip(PHRASE) {
        let mut note = Note::new(id, midi, TimePos::at(measure, start), TimePos::at(measure, start + len));
        if midi == 67 {
            (note.technique, note.link_to) = (Some(Technique::Bend), Some(id + 1));
        } else if start >= 12.0 {
            note.technique = Some(Technique::PalmMute);
        }
        project.notes.push(note);
    }
    let played = performance(&project);
    for preset in SynthPreset::all() {
        let audio = NoteSynth::render_offline(&played, preset, RATE, PitchRef::default(), SECONDS);
        let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
        let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let path = dir.join(format!("{}.wav", preset.name().replace(' ', "_").to_lowercase()));
        std::fs::write(&path, wav::encode_mono_f32(RATE, &audio))?;
        println!("{}: rms {rms:.3}, peak {peak:.2}", path.display());
    }
    Ok(())
}
